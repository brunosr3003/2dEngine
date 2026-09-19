//! Persistencia de personagens em Postgres.
//!
//! Arquitetura:
//! - Server abre pool Postgres no startup, roda schema, carrega todos os
//!   personagens pra memoria.
//! - O tick loop le/escreve no cache sincronamente (sem locks complicados).
//! - Uma task background recebe `SaveBatch` via mpsc e escreve no DB, nunca
//!   bloqueando o tick.
//!
//! Fase 4 minimo: persiste nome, posicao e HP. Inventario/XP vem depois.

use anyhow::Result;
use glam::Vec2;
use shared::Health;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Estado do barco persistido junto com o character — quando setado, o
/// player desconectou perto/em cima do barco e queremos recriar o barco
/// no login com o estado completo (sail, anchor, etc).
#[derive(Debug, Clone, Copy)]
pub struct PersistedBoat {
    /// boat_kind (0=Lylian Leutard).
    pub kind: u16,
    /// Posicao do casco (em world coords).
    pub pos: Vec2,
    /// Direcao do casco (0..7) — derivado do yaw, mantido pra compat
    /// com BoatRenderer 2D atual.
    pub dir: u8,
    /// Heading float (rad). Quando carregado de DB legado (NULL), eh
    /// derivado de `dir * PI/4`.
    pub yaw: f32,
    /// Posicao da vela: 0=raised, 1=half, 2=full.
    pub sail_position: u8,
    /// Angulo da vela em rad relativo ao casco.
    pub sail_angle: f32,
    /// Ancora dropada.
    pub anchor_dropped: bool,
}

#[derive(Debug, Clone)]
pub struct CharacterRow {
    pub name: String,
    pub pos: Vec2,
    pub hp: Health,
    pub xp: u64,
    /// Moeda corrente. Não ocupa slot de inventário (currency separado).
    pub gold: u64,
    /// Quando Some, player estava montado num barco no ultimo save. Login
    /// recria o barco e re-mount.
    pub boat: Option<PersistedBoat>,
    /// Posicao do player no deck local (relativa ao centro do barco) no
    /// ultimo save. None = nao estava em cima do barco. Some = re-mountar
    /// na mesma posicao do deck.
    pub mounted_local: Option<Vec2>,
    /// Vec com INVENTORY_SLOTS entradas (slots vazios = qty==0).
    pub inventory: Vec<shared::InventorySlot>,
    pub equipment: shared::Equipment,
    /// Vault persistente (INVENTORY_SLOTS slots igual o inv do player).
    pub vault: Vec<shared::InventorySlot>,
    /// Fame: pontuacao de prestigio; ganha matando players/bosses.
    pub fame: u64,
    /// Aura/Poise: pontos ganhos SOMENTE em vitorias PvP. Perde ao ser
    /// morto por outro player. Base pra sistema de stagger futuro.
    pub aura: u64,
    /// XP por proficiencia (sword/staff/etc). Indice = Proficiency as u8.
    pub proficiencies: [u64; shared::PROF_COUNT],
    /// Pontos de atributo ainda nao distribuidos (ganhos via level-up).
    pub unspent_points: u32,
    /// Pontos ja alocados em cada stat [FOR, DES, INT, VIT, SPD]
    /// (5 stats — refactor M5 do design classless).
    pub allocated_points: [u32; shared::STAT_COUNT],
    /// SP totais ganhos (cumulative ao longo dos levels). Skills Phase 1.
    pub skill_points_earned: u32,
    /// SP já gastos em learn + rank-up.
    pub skill_points_spent: u32,
    /// Conta dona deste char (1:1, UNIQUE). None pra rows legacy nao migrados
    /// — interpretado como "linkado pelo nome" (backfill ja roda).
    pub account_id: Option<i64>,
    /// VisualConfig escolhido na criacao (skin race + tone, outfit + color,
    /// hair + color, body tint). None = usa default por classe.
    pub visual: Option<shared::VisualConfig>,
    /// Níveis de skill de coleta. Default 1 (sem bônus). Crescem ao colher.
    /// Facção escolhida na criação (Morganeers/Peacemain). Persistida como
    /// TEXT. Default Peacemain pra rows legacy sem a coluna.
    pub faction: shared::Faction,
    /// Estado de quests do personagem (character_quests).
    pub quests: Vec<crate::quests::CharQuest>,
    /// Pontos de facção (moeda das quests de facção).
    pub faction_points: u32,
    /// Epoch (segundos) de quando concluiu o tutorial pela última vez. None =
    /// nunca concluiu → no login no mundo, é redirecionado pro tutorial.
    pub last_tutorial_completed: Option<i64>,
    /// Mana e stamina no ultimo save. None = row antiga: entra cheio.
    pub mp: Option<f32>,
    pub stamina: Option<f32>,
    /// Ilha (zona) onde a posicao vale. None = row antiga. Sem isto, quem
    /// entrava num canal de outra ilha usava coordenadas que eram de la'.
    pub zona: Option<String>,
    /// Pocao de Experiencia: bonus de XP ate' este instante (unix secs; 0 =
    /// nenhum). Absoluto, entao sobrevive a relog e reinicio.
    pub xp_bonus_ate: i64,
    /// Pocoes de Fortuna e de Sorte: buff ate' este instante (unix secs).
    pub fortuna_ate: i64,
    pub sorte_ate: i64,
    /// Barra de itens configurada (`barra::para_json`). Vazio = padrao.
    pub barra_json: String,
    /// Mortes com XP recuperavel (`morte::MorteRecuperavel` em JSON).
    pub mortes_json: String,
    /// Dia UTC (`morte::dia`) e quantas recuperacoes gratis ja' saiu nele.
    pub recuperacoes_dia: i64,
    pub recuperacoes_usadas: i32,
    /// Preferencias de tela (`preferencias::para_json`). Vazio = padrao.
    pub preferencias_json: String,
    /// Dungeons do personagem (`shared::dungeon::DadosDungeon`): entradas,
    /// estagios liberados, baus abertos, correio. Vai no MESMO save da bolsa.
    pub dungeon_json: String,
    /// Dungeons da CONTA (`shared::dungeon::DadosConta`): 1ª vitoria semanal e
    /// teto de Selo. Vazio = nao grava (`dungeon_contas`, por `account_id`).
    pub conta_dungeon_json: String,
    /// Resgates de presenca ja' entregues neste personagem
    /// (`presenca_resgates.id`): marcados `aplicado` na MESMA transacao da
    /// bolsa (docs/CALENDARIO.md). Nao e' carregado do banco.
    pub presenca_aplicados: Vec<String>,
    pub correio_recibos: Vec<crate::correio_admin::Recibo>,
}

/// Abre o pool Postgres, garante schema criado.
/// Pre-passada: cria TODAS as tabelas antes de qualquer migration.
///
/// As migrations inline de `open_pool` (M7..M11, ALTERs de `enemy_kinds`)
/// rodam antes dos `CREATE TABLE` das tabelas que elas tocam. Em DB que ja
/// existe isso passa despercebido; em DB novo o server aborta no boot. Esta
/// funcao roda os CREATEs na ordem em que aparecem (que ja respeita as FKs),
/// deixando `open_pool` idempotente a partir de um banco vazio.
async fn create_tables(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
            id             BIGSERIAL PRIMARY KEY,
            username       TEXT NOT NULL UNIQUE,
            email          TEXT NOT NULL UNIQUE,
            password_hash  TEXT NOT NULL,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS characters (
            name     TEXT PRIMARY KEY,
            x        REAL NOT NULL,
            y        REAL NOT NULL,
            hp       INTEGER NOT NULL,
            max_hp   INTEGER NOT NULL,
            updated  BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proficiencies (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            prof_kind      INTEGER NOT NULL,
            xp             BIGINT  NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, prof_kind)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS inventory (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS equipment (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           TEXT NOT NULL,
            item_id        INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vault (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy_version (
            id          SMALLINT PRIMARY KEY DEFAULT 1,
            version     BIGINT NOT NULL DEFAULT 1,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CHECK (id = 1)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS enemy_kinds (
            kind             INTEGER PRIMARY KEY,
            name             TEXT    NOT NULL,
            hp_max           INTEGER NOT NULL,
            speed            REAL    NOT NULL,
            attack_damage    INTEGER NOT NULL,
            attack_cooldown  REAL    NOT NULL,
            detect_range     REAL    NOT NULL,
            attack_range     REAL    NOT NULL,
            kite_dist        REAL,
            proj_count       INTEGER NOT NULL DEFAULT 1,
            xp_reward        BIGINT  NOT NULL,
            defense          INTEGER NOT NULL DEFAULT 0,
            size_scale       REAL    NOT NULL DEFAULT 1.0,
            tint_r           REAL    NOT NULL DEFAULT 1.0,
            tint_g           REAL    NOT NULL DEFAULT 1.0,
            tint_b           REAL    NOT NULL DEFAULT 1.0,
            tint_a           REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS item_drops_log (
            id          BIGSERIAL PRIMARY KEY,
            ts          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            enemy_kind  INTEGER     NOT NULL,
            item_id     INTEGER     NOT NULL,
            qty         INTEGER     NOT NULL,
            rarity      SMALLINT    NOT NULL DEFAULT 0,
            item_level  INTEGER     NOT NULL DEFAULT 1,
            refinement  SMALLINT    NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS farm_node_drops (
            id       SERIAL PRIMARY KEY,
            kind     TEXT    NOT NULL,
            tier     INTEGER NOT NULL,
            item_id  INTEGER NOT NULL,
            qty_min  INTEGER NOT NULL,
            qty_max  INTEGER NOT NULL,
            chance   REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn open_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;

    // ── Trava de schema ───────────────────────────────────────────────────
    // Com canais (varios processos do mesmo binario), todos sobem juntos e
    // rodam as mesmas migrations ao mesmo tempo — o Postgres responde com
    // "tuple concurrently updated" e um dos processos morre no boot.
    //
    // O advisory lock serializa: o primeiro cria o schema, os outros esperam e
    // encontram tudo pronto (as migrations sao idempotentes). A trava e' da
    // SESSAO, entao ela cai sozinha se o processo morrer no meio.
    //
    // A trava vive numa conexao PROPRIA, tirada do pool e segurada ate' o fim.
    // Rodando `execute(&pool)` o lock e o unlock caem em conexoes quaisquer:
    // o unlock nao encontra a trava, devolve `false` em silencio, e a conexao
    // que travou volta pro pool AINDA SEGURANDO o lock — pra sempre, porque
    // ninguem fecha conexao de pool. Visto em producao: 11 canais subindo
    // juntos, 3 no ar e 8 parados em `pg_advisory_lock` por minutos.
    // Com um canal so' isso nunca aparece; ninguem espera na fila.
    let mut trava = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(728431)")
        .execute(&mut *trava)
        .await?;
    let r = init_schema_travado(&pool).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock(728431)")
        .execute(&mut *trava)
        .await;
    drop(trava);
    r?;

    Ok(pool)
}

async fn init_schema_travado(pool: &PgPool) -> Result<()> {
    // Cria o schema base antes das migrations inline (ver create_tables).
    create_tables(pool).await?;

    // `accounts` e mantida pelo crate `web`; aqui so garantimos que existe
    // (idempotente) para o caso do game server subir antes do web.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
            id             BIGSERIAL PRIMARY KEY,
            username       TEXT NOT NULL UNIQUE,
            email          TEXT NOT NULL UNIQUE,
            password_hash  TEXT NOT NULL,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS class TEXT NOT NULL DEFAULT 'warrior'",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS characters (
            name     TEXT PRIMARY KEY,
            x        REAL NOT NULL,
            y        REAL NOT NULL,
            hp       INTEGER NOT NULL,
            max_hp   INTEGER NOT NULL,
            updated  BIGINT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Migracao inline: colunas de progressao. IF NOT EXISTS para idempotencia.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS xp BIGINT NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS fame BIGINT NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS aura BIGINT NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    // Pontos de atributo: unspent counter + array de 6 alocados.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS unspent_points INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS allocated_points INTEGER[] NOT NULL DEFAULT '{0,0,0,0,0,0}'")
        .execute(pool)
        .await?;
    // Pontos de SKILL. Estavam faltando aqui: o codigo le' e grava as duas
    // (`load_all`, o upsert do save e o `world`), mas nenhuma migracao as
    // criava. Banco que veio evoluindo tem as colunas e nao reclama; banco
    // NOVO subia ate' o fim do seed e morria em "column skill_points_earned
    // does not exist" — ou seja, criar um realm do zero estava quebrado, e so'
    // aparecia pra quem tentasse.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS skill_points_earned INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS skill_points_spent INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;

    // Boat state — quando player desconecta montado, salvamos o tipo do
    // barco + pos + direcao. Re-spawn no login. NULL = nao tava montado.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_kind SMALLINT NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_x REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_y REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_dir SMALLINT NULL")
        .execute(pool)
        .await?;
    // Boat 2.5D: estado completo (yaw float, sail/anchor) + posicao do
    // player no deck local. Tudo NULL pra rows legacy (load fall-back).
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_yaw REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_sail_pos SMALLINT NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_sail_angle REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_anchor_dropped BOOLEAN NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS mounted_local_x REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS mounted_local_y REAL NULL")
        .execute(pool)
        .await?;
    // Character creation: account_id liga char a conta (1:1, UNIQUE).
    // visual_json armazena VisualConfig serializado (skin/race/outfit/hair/color).
    // starting_weapon = item_id escolhido na criacao (informativo; weapon ja
    // ta em inventory+equipment do save).
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS account_id BIGINT NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS visual_json TEXT NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS starting_weapon SMALLINT NULL")
        .execute(pool)
        .await?;
    // Backfill account_id pra chars antigos (linka pelo username = char name).
    sqlx::query(
        "UPDATE characters c SET account_id = a.id
         FROM accounts a
         WHERE c.account_id IS NULL AND a.username = c.name",
    )
    .execute(pool)
    .await?;
    // Indice nao-unico em account_id pra lookup rapido de chars por conta.
    // (Multi-char per account: removida constraint UNIQUE de versao anterior.)
    sqlx::query("DROP INDEX IF EXISTS idx_characters_account_unique")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_characters_account ON characters(account_id)")
        .execute(pool)
        .await?;
    // Migration M6: design atual tem 6 stats (FOR/DES/INT/VIT/SPD/RES). Linhas
    // antigas com 5 elementos ganham um 0 no slot RES, preservando pontos ja
    // alocados. Idempotente — arrays de 6 nao sao tocados.
    sqlx::query(
        "UPDATE characters
         SET allocated_points = allocated_points || ARRAY[0]::INTEGER[]
         WHERE array_length(allocated_points, 1) = 5",
    )
    .execute(pool)
    .await?;

    // Migration M7: escudo (item_id=7) sai do slot 'armor' e vai pra 'offhand'.
    // Idempotente: se ja moveu, UPDATE nao acha mais nada.
    sqlx::query(
        "UPDATE equipment SET slot = 'offhand'
         WHERE slot = 'armor' AND item_id = 7",
    )
    .execute(pool)
    .await?;

    // Migration M8: deixa escudo comprável no shop. So aplica se o DB ja
    // tinha shield com buy_price=NULL (preserva tweaks manuais que o user
    // fez via SQL).
    sqlx::query(
        "UPDATE items SET buy_price = 50, shop_order = 9
         WHERE id = 7 AND buy_price IS NULL AND shop_order IS NULL",
    )
    .execute(pool)
    .await?;

    // Migration M9: garante que o Mercador (shop_id=1, vendor Klaus no mapa)
    // venda escudo. ON CONFLICT DO NOTHING pra ser idempotente em DBs onde
    // ja foi adicionado.
    sqlx::query(
        // WHERE EXISTS: em DB novo a loja 1 ainda nao foi semeada (isso
        // acontece depois, no seed_economy_if_needed) — sem o guard o INSERT
        // viola a FK e o server nao sobe.
        "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order)
         SELECT 1, 7, 9
         WHERE EXISTS (SELECT 1 FROM vendor_shops WHERE shop_id = 1)
         ON CONFLICT DO NOTHING",
    )
    .execute(pool)
    .await?;

    // Migration M10: chars com shield (item_id=7) no offhand E weapon two-handed
    // (great_sword=13, bow=14, staff=6, wand=15) ficaram com combo invalido —
    // a M7 anterior moveu shield pro offhand sem checar a weapon. Apaga o offhand
    // pra esses casos (shield perdido — raro; tradeoff aceitavel pro fix de design).
    sqlx::query(
        "DELETE FROM equipment e
         WHERE e.slot = 'offhand' AND e.item_id = 7
         AND EXISTS (
             SELECT 1 FROM equipment w
             WHERE w.character_name = e.character_name
               AND w.slot = 'weapon'
               AND w.item_id IN (13, 14, 6, 15)
         )",
    )
    .execute(pool)
    .await?;

    // Migration M11: gold vira moeda (não-item). Coluna `characters.gold` +
    // backfill somando todo item_id=1 de inventory + vault, depois apaga as
    // rows. Idempotente: se rodar de novo, sum() vira 0 (nada pra somar).
    // Estado que sumia no reinicio: mana, stamina e a zona da posicao.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS mp REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS stamina REAL NULL")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS zona TEXT NULL")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS xp_bonus_ate BIGINT NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    // Pocoes de Fortuna e de Sorte, e a barra de itens configurada.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS fortuna_ate BIGINT NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS sorte_ate BIGINT NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS barra_json TEXT NOT NULL DEFAULT ''",
    )
    .execute(pool)
    .await?;
    // Morte: XP recuperavel e as recuperacoes gratis do dia.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS mortes_json TEXT NOT NULL DEFAULT ''",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS recuperacoes_dia BIGINT NOT NULL DEFAULT 0")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS recuperacoes_usadas INTEGER NOT NULL DEFAULT 0")
        .execute(pool).await?;
    // Preferencias de tela: skills AUTO, filtros do mapa, zooms.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS dungeon_json TEXT NOT NULL DEFAULT ''",
    )
    .execute(pool)
    .await?;
    // Dungeons da conta (docs/DUNGEONS_E_RAIDS.md): o que vale pra qualquer
    // personagem dela. Gravada na mesma transacao do save do personagem.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS dungeon_contas (
            account_id BIGINT PRIMARY KEY,
            dados_json TEXT NOT NULL DEFAULT '',
            updated    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;
    // Calendario de presenca (docs/CALENDARIO.md): uma linha por resgate.
    crate::presenca::criar_tabelas(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS preferencias_json TEXT NOT NULL DEFAULT ''")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS gold BIGINT NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE characters c SET gold = c.gold + COALESCE((
            SELECT SUM(qty)::BIGINT FROM inventory i
            WHERE i.character_name = c.name AND i.item_id = 1
        ), 0) + COALESCE((
            SELECT SUM(qty)::BIGINT FROM vault v
            WHERE v.character_name = c.name AND v.item_id = 1
        ), 0)",
    )
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM inventory WHERE item_id = 1")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM vault     WHERE item_id = 1")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proficiencies (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            prof_kind      INTEGER NOT NULL,
            xp             BIGINT  NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, prof_kind)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS inventory (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    // Migration Fase A: instance_data armazena ItemInstance serializada
    // como JSON. NULL pra stackáveis e itens legacy.
    sqlx::query("ALTER TABLE inventory ADD COLUMN IF NOT EXISTS instance_data TEXT NULL")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS equipment (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           TEXT NOT NULL,
            item_id        INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE equipment ADD COLUMN IF NOT EXISTS instance_data TEXT NULL")
        .execute(pool)
        .await?;

    // Vault: bau persistente por personagem. Estrutura igual a inventory.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vault (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE vault ADD COLUMN IF NOT EXISTS instance_data TEXT NULL")
        .execute(pool)
        .await?;

    // ── Economy tables ──────────────────────────────────────────────────────
    // Migration M23: as ferramentas deixaram de existir. Coleta e' automatica
    // e nao tem portao, entao machado/foice/picareta/vara nao sao mais item.
    // Tira da loja, do equipamento e do catalogo — item inativo some da UI
    // sem quebrar linha de inventario antiga que ainda referencie o id.
    for q in [
        "DELETE FROM vendor_shop_items WHERE item_id BETWEEN 80 AND 95",
        "DELETE FROM equipment WHERE item_id BETWEEN 80 AND 95",
        "UPDATE items SET active = FALSE WHERE id BETWEEN 80 AND 95",
    ] {
        let _ = sqlx::query(q).execute(pool).await;
    }

    // Bumpa `economy_version.version` em qualquer ferramenta SQL pra forçar
    // hot-reload no servidor.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy_version (
            id          SMALLINT PRIMARY KEY DEFAULT 1,
            version     BIGINT NOT NULL DEFAULT 1,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CHECK (id = 1)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO economy_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await?;
    // Fase F — campos editáveis pelo admin (slot, level, icon, stat ranges).
    // Cada coluna idempotente; backfill abaixo popula valores hardcoded em
    // items existentes na primeira boot pós-upgrade.
    for col in &[
        "ADD COLUMN IF NOT EXISTS equip_slot TEXT",
        "ADD COLUMN IF NOT EXISTS item_level INTEGER NOT NULL DEFAULT 1",
        "ADD COLUMN IF NOT EXISTS icon_col INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS icon_row INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS icon_path TEXT",
        "ADD COLUMN IF NOT EXISTS hp_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS hp_max INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS mp_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS mp_max INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS atk_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS atk_max INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS def_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS def_max INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS dex_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS dex_max INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS wis_min INTEGER NOT NULL DEFAULT 0",
        "ADD COLUMN IF NOT EXISTS wis_max INTEGER NOT NULL DEFAULT 0",
        // Inativo: server não dropa, não equipa, não usa. Pode vender/guardar.
        "ADD COLUMN IF NOT EXISTS active BOOLEAN NOT NULL DEFAULT TRUE",
        // Vinculado: fora do mercado global (docs/MERCADO.md).
        "ADD COLUMN IF NOT EXISTS vinculado BOOLEAN NOT NULL DEFAULT FALSE",
    ] {
        sqlx::query(&format!("ALTER TABLE items {col}"))
            .execute(pool)
            .await?;
    }
    // Recompensa de missao nasce vinculada (a Pocao de XP das missoes de area).
    // Uma vez so': depois disso quem manda e' a coluna.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS migracoes_de_dados (nome TEXT PRIMARY KEY, feita TIMESTAMPTZ NOT NULL DEFAULT NOW())",
    ).execute(pool).await?;
    let marcou = sqlx::query("INSERT INTO migracoes_de_dados (nome) VALUES ('mercado_vinculados_v1') ON CONFLICT DO NOTHING")
        .execute(pool).await?.rows_affected();
    if marcou > 0 {
        sqlx::query("UPDATE items SET vinculado = TRUE WHERE id = $1")
            .bind(shared::item_id::XP_POTION as i32)
            .execute(pool)
            .await?;
    }
    // Marcas e Selo da Tempestade nascem vinculados (docs/DUNGEONS_E_RAIDS.md).
    // Roda depois do seed dos itens; a linha que ainda nao existe entra la'.
    let dungeon_v1 = sqlx::query("INSERT INTO migracoes_de_dados (nome) VALUES ('dungeon_vinculados_v1') ON CONFLICT DO NOTHING")
        .execute(pool).await?.rows_affected();
    if dungeon_v1 > 0 {
        sqlx::query("UPDATE items SET vinculado = TRUE WHERE id = ANY($1)")
            .bind(vec![
                shared::item_id::MARCAS_TEMPESTADE as i32,
                shared::item_id::SELO_TEMPESTADE as i32,
            ])
            .execute(pool)
            .await?;
    }
    // Premio de presenca e' vinculado (docs/CALENDARIO.md): Fortuna e Sorte so'
    // saem de recompensa (diaria e calendario) e nao vao ao mercado.
    let presenca_v1 = sqlx::query("INSERT INTO migracoes_de_dados (nome) VALUES ('presenca_vinculados_v1') ON CONFLICT DO NOTHING")
        .execute(pool).await?.rows_affected();
    if presenca_v1 > 0 {
        sqlx::query("UPDATE items SET vinculado = TRUE WHERE id = ANY($1)")
            .bind(vec![
                shared::item_id::FORTUNA_POTION as i32,
                shared::item_id::SORTE_POTION as i32,
            ])
            .execute(pool)
            .await?;
    }
    // Override de item_level no drop por enemy_kind (era hardcoded em
    // world.rs::spawn_loot_drops). NULL = usa items.item_level como fallback.
    // Phase 5 enemy refactor: build "playerizado" — colunas opcionais pra
    // weapon/offhand/armor + level. NULL = usa defaults hardcoded em
    // enemy_builds.rs. Schema soft (nao quebra se NULL); admin pode editar
    // via SQL ate ter UI dedicada.
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_level INTEGER")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_weapon SMALLINT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_offhand SMALLINT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_armor SMALLINT")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_alloc_points INTEGER[]")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds DROP COLUMN IF EXISTS build_learned_skills")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS loot_item_level INTEGER")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS enemy_kinds (
            kind             INTEGER PRIMARY KEY,
            name             TEXT    NOT NULL,
            hp_max           INTEGER NOT NULL,
            speed            REAL    NOT NULL,
            attack_damage    INTEGER NOT NULL,
            attack_cooldown  REAL    NOT NULL,
            detect_range     REAL    NOT NULL,
            attack_range     REAL    NOT NULL,
            kite_dist        REAL,
            proj_count       INTEGER NOT NULL DEFAULT 1,
            xp_reward        BIGINT  NOT NULL,
            defense          INTEGER NOT NULL DEFAULT 0,
            size_scale       REAL    NOT NULL DEFAULT 1.0,
            tint_r           REAL    NOT NULL DEFAULT 1.0,
            tint_g           REAL    NOT NULL DEFAULT 1.0,
            tint_b           REAL    NOT NULL DEFAULT 1.0,
            tint_a           REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_loot_drops_kind ON loot_drops(enemy_kind)")
        .execute(pool)
        .await?;

    // Log de cada drop emitido pelo server. Cresce monotonicamente — admin
    // usa pra observabilidade (quantidade dropada por mob/item, frequência
    // de raridades). Sem TTL hoje; futuramente vacuumar > 30d via cron.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS item_drops_log (
            id          BIGSERIAL PRIMARY KEY,
            ts          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            enemy_kind  INTEGER     NOT NULL,
            item_id     INTEGER     NOT NULL,
            qty         INTEGER     NOT NULL,
            rarity      SMALLINT    NOT NULL DEFAULT 0,
            item_level  INTEGER     NOT NULL DEFAULT 1,
            refinement  SMALLINT    NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_ts   ON item_drops_log(ts DESC)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_item ON item_drops_log(item_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_kind ON item_drops_log(enemy_kind)")
        .execute(pool)
        .await?;

    // Farm node drops — análogo a loot_drops, mas keyed por (kind, tier).
    // kind = 'Tree' | 'Rock' | 'Flower'; tier = 1..4. Cada linha rola
    // independente: chance > rand → dropa qty em [qty_min, qty_max].
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS farm_node_drops (
            id       SERIAL PRIMARY KEY,
            kind     TEXT    NOT NULL,
            tier     INTEGER NOT NULL,
            item_id  INTEGER NOT NULL,
            qty_min  INTEGER NOT NULL,
            qty_max  INTEGER NOT NULL,
            chance   REAL    NOT NULL DEFAULT 1.0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_farm_node_drops_kt \
                 ON farm_node_drops(kind, tier)",
    )
    .execute(pool)
    .await?;

    // Vendor shops — cada vendor tem um shop_id que aponta pra uma lista
    // curada de itens. Permite "espadeiro" que só vende espadas, "alquimista"
    // que só vende poções, etc., independente da categoria genérica.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    )
    .execute(pool)
    .await?;

    // ── Skills (Phase 1 / M11) ──────────────────────────────────────────────
    // Migration: ADD COLUMN knockback caso DB antigo nao tenha. Default 0.5
    // pra qualquer skill direcional ter um shove leve sem precisar tunar.
    // As migracoes da tabela de skills antiga sairam junto com ela: 44
    // colunas de rank, afinidade e payload pra um sistema que nao existe.

    // M24: coleta nao tem mais nivel — nem de lenhador, nem de minerador, nem
    // de coletor. O que rende agora e' o LUGAR, e lugar nao cabe em coluna de
    // personagem. As tres colunas da M12 saem.
    for c in ["woodcutting_lvl", "mining_lvl", "gathering_lvl"] {
        let _ = sqlx::query(&format!("ALTER TABLE characters DROP COLUMN IF EXISTS {c}"))
            .execute(pool)
            .await;
    }
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS faction TEXT NOT NULL DEFAULT 'peacemain'",
    )
    .execute(pool)
    .await?;
    // Quando o personagem concluiu o tutorial pela ultima vez (NULL = nunca).
    // Usado pra UI ("ja fez tutorial") e pro fluxo de re-treino. Nao gateia
    // nada de forma rigida — tutorial e' repetivel.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS last_tutorial_completed TIMESTAMPTZ NULL",
    )
    .execute(pool)
    .await?;

    seed_economy_if_needed(pool).await?;

    Ok(())
}

/// Seed inicial: insere defaults pros itens/enemies que ainda não estão no DB.
/// Usa ON CONFLICT DO NOTHING — preserva tweaks manuais. Loot só seeda se a
/// tabela inteira estiver vazia (evita duplicar drops).
async fn seed_economy_if_needed(pool: &PgPool) -> Result<()> {
    use shared::item_id;

    // Seed completo dos 44 itens + campos editáveis (slot, level, icon, ranges).
    //
    // Idempotência:
    //  - INSERT ON CONFLICT (id) DO UPDATE atualiza só os campos novos
    //    (slot/level/icon/ranges), mantendo nome/preço/stack existentes.
    //  - WHERE items.icon_col = 0 AND items.icon_row = 0 só rodar update na
    //    primeira boot pós-migration (sentinel "ainda não backfilled"). Após
    //    admin editar via web, icon_col != 0 e o seed vira no-op.
    //  - Itens 24-44 (Phase D/E) que ainda não tinham row são inseridos do zero.
    struct S {
        id: i32,
        name: &'static str,
        sell: i32,
        buy: Option<i32>,
        ord: Option<i32>,
        stack: i32,
        slot: Option<&'static str>,
        lvl: i32,
        ic: i32,
        ir: i32,
        hp: (i32, i32),
        mp: (i32, i32),
        atk: (i32, i32),
        def: (i32, i32),
        dex: (i32, i32),
        wis: (i32, i32),
    }
    let seed: &[S] = &[
        // Consumíveis / materiais — sem slot/range
        S {
            id: item_id::GOLD as i32,
            name: "Gold",
            sell: 0,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: 15,
            ir: 9,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::HEALTH_POTION as i32,
            name: "HP Potion",
            sell: 5,
            buy: Some(10),
            ord: Some(0),
            stack: 20,
            slot: None,
            lvl: 1,
            ic: 3,
            ir: 17,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::MANA_POTION as i32,
            name: "MP Potion",
            sell: 7,
            buy: Some(15),
            ord: Some(1),
            stack: 20,
            slot: None,
            lvl: 1,
            ic: 10,
            ir: 7,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::GREATER_HEAL as i32,
            name: "HP Potion+",
            sell: 20,
            buy: Some(40),
            ord: Some(2),
            stack: 20,
            slot: None,
            lvl: 1,
            ic: 11,
            ir: 17,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::GREATER_MANA as i32,
            name: "MP Potion+",
            sell: 25,
            buy: Some(50),
            ord: Some(3),
            stack: 20,
            slot: None,
            lvl: 1,
            ic: 9,
            ir: 7,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::STAMINA_POTION as i32,
            name: "Stamina Potion",
            sell: 10,
            buy: Some(20),
            ord: Some(4),
            stack: 20,
            slot: None,
            lvl: 1,
            ic: 8,
            ir: 7,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // So' recompensa de missao de area: sem preco de compra, venda simbolica.
        S {
            id: item_id::XP_POTION as i32,
            name: "Poção de Experiência",
            sell: 1,
            buy: None,
            ord: None,
            stack: 20,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Recompensa de diaria de oficina: sem preco de compra, venda simbolica.
        S {
            id: item_id::FORTUNA_POTION as i32,
            name: "Poção de Fortuna",
            sell: 1,
            buy: None,
            ord: None,
            stack: 20,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::SORTE_POTION as i32,
            name: "Poção de Sorte",
            sell: 1,
            buy: None,
            ord: None,
            stack: 20,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Armas
        // Fase F — armas tier 2 (gate de char_lvl + prof_lvl). Item lvl 10 marca o tier.
        // Fase F — armas tier 3 (char_lvl 20, sword prof 10). Item lvl 20.
        // Fase F — armas tier 2 das outras 5 profs (char_lvl 10, prof respectiva 5).
        // Armaduras / escudos
        // Acessórios
        // Phase E — slots novos
        // Embarcacao — usavel na margem (consumida ao usar; volta no dismount).
        // Sem equip slot. Stack 1 (item unico). Icone re-aproveitado de barril
        // ate ter art proprio.
        S {
            id: item_id::BOAT_LYLIAN_LEUTARD as i32,
            name: "Lylian Leutard",
            sell: 0,
            buy: Some(500),
            ord: Some(50),
            stack: 1,
            slot: None,
            lvl: 1,
            ic: 0,
            ir: 138,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // === Materiais de coleta e craft. Nunca estiveram na tabela: a coleta
        // entregava um item sem nome e sem `stack_max`, que caia em stack de 1 e
        // enchia a bolsa com dezenas de linhas de uma madeira cada. Achado com os
        // bots coletando na ilha.
        // === O equipamento (docs/COMBATE.md): um id por peca; grau e refino
        // moram na instancia. As faixas de stat sao as do template
        // (`items::item_template`). ===
        S {
            id: item_id::ESPADA_E_ESCUDO as i32,
            name: "Espada e Escudo",
            sell: 60,
            buy: Some(960),
            ord: Some(20),
            stack: 1,
            slot: Some("Weapon"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (10, 30),
            mp: (0, 0),
            atk: (8, 16),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::KATANA as i32,
            name: "Katana",
            sell: 60,
            buy: Some(960),
            ord: Some(21),
            stack: 1,
            slot: Some("Weapon"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (8, 15),
            def: (0, 0),
            dex: (5, 12),
            wis: (0, 0),
        },
        S {
            id: item_id::PISTOLAS as i32,
            name: "Duas Pistolas",
            sell: 60,
            buy: Some(960),
            ord: Some(22),
            stack: 1,
            slot: Some("Weapon"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (7, 14),
            def: (0, 0),
            dex: (6, 13),
            wis: (0, 0),
        },
        S {
            id: item_id::ANEL_MAGICO as i32,
            name: "Anel Mágico",
            sell: 60,
            buy: Some(960),
            ord: Some(23),
            stack: 1,
            slot: Some("Weapon"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (30, 70),
            atk: (6, 13),
            def: (0, 0),
            dex: (0, 0),
            wis: (4, 10),
        },
        S {
            id: item_id::MANTO_DO_GUERREIRO as i32,
            name: "Manto do Guerreiro",
            sell: 40,
            buy: Some(640),
            ord: Some(24),
            stack: 1,
            slot: Some("Offhand"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (20, 50),
            mp: (0, 0),
            atk: (0, 0),
            def: (3, 8),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::BAINHA as i32,
            name: "Bainha",
            sell: 40,
            buy: Some(640),
            ord: Some(25),
            stack: 1,
            slot: Some("Offhand"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (1, 4),
            def: (0, 0),
            dex: (3, 8),
            wis: (0, 0),
        },
        S {
            id: item_id::COLDRE as i32,
            name: "Coldre",
            sell: 40,
            buy: Some(640),
            ord: Some(26),
            stack: 1,
            slot: Some("Offhand"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (2, 5),
            def: (0, 0),
            dex: (3, 7),
            wis: (0, 0),
        },
        S {
            id: item_id::MANTO_DO_MAGO as i32,
            name: "Manto do Mago",
            sell: 40,
            buy: Some(640),
            ord: Some(27),
            stack: 1,
            slot: Some("Offhand"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (25, 60),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (3, 8),
        },
        S {
            id: item_id::ARMADURA_LEVE as i32,
            name: "Armadura Leve",
            sell: 50,
            buy: Some(800),
            ord: Some(28),
            stack: 1,
            slot: Some("Armor"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (15, 35),
            mp: (0, 0),
            atk: (0, 0),
            def: (1, 4),
            dex: (2, 6),
            wis: (0, 0),
        },
        S {
            id: item_id::ARMADURA_MEDIA as i32,
            name: "Armadura Média",
            sell: 70,
            buy: Some(1120),
            ord: Some(29),
            stack: 1,
            slot: Some("Armor"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (30, 60),
            mp: (0, 0),
            atk: (0, 0),
            def: (4, 9),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::ARMADURA_PESADA as i32,
            name: "Armadura Pesada",
            sell: 90,
            buy: Some(1440),
            ord: Some(30),
            stack: 1,
            slot: Some("Armor"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (60, 120),
            mp: (0, 0),
            atk: (0, 0),
            def: (8, 16),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::BRINCO as i32,
            name: "Brinco",
            sell: 35,
            buy: Some(560),
            ord: Some(31),
            stack: 1,
            slot: Some("Earring"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (1, 4),
            def: (0, 0),
            dex: (2, 6),
            wis: (0, 0),
        },
        S {
            id: item_id::AMULETO as i32,
            name: "Amuleto",
            sell: 35,
            buy: Some(560),
            ord: Some(32),
            stack: 1,
            slot: Some("Necklace"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (20, 50),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (2, 6),
        },
        S {
            id: item_id::BRACELETE as i32,
            name: "Bracelete",
            sell: 35,
            buy: Some(560),
            ord: Some(33),
            stack: 1,
            slot: Some("Bracelet"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (2, 5),
            def: (1, 3),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::CINTO as i32,
            name: "Cinto",
            sell: 35,
            buy: Some(560),
            ord: Some(34),
            stack: 1,
            slot: Some("Belt"),
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (20, 45),
            mp: (0, 0),
            atk: (0, 0),
            def: (1, 3),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::WOOD_T1 as i32,
            name: "Madeira T1",
            sell: 2,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::WOOD_T2 as i32,
            name: "Madeira T2",
            sell: 6,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::WOOD_T3 as i32,
            name: "Madeira T3",
            sell: 18,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::WOOD_T4 as i32,
            name: "Madeira T4",
            sell: 54,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::LEATHER_T1 as i32,
            name: "Couro T1",
            sell: 3,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::LEATHER_T2 as i32,
            name: "Couro T2",
            sell: 9,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::LEATHER_T3 as i32,
            name: "Couro T3",
            sell: 27,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::LEATHER_T4 as i32,
            name: "Couro T4",
            sell: 81,
            buy: None,
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // === Materiais de craft. Poucos nomes girando muito: arma e sub-arma
        // gastam o mesmo, armaduras entre si idem, acessorios idem. Cada um
        // existe nas quatro cores, e a cor E' o tier.
        S {
            id: item_id::na_cor(item_id::STEEL, 1) as i32,
            name: "Aço Cinza",
            sell: 3,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::STEEL, 2) as i32,
            name: "Aço Verde",
            sell: 12,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::STEEL, 3) as i32,
            name: "Aço Azul",
            sell: 48,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::STEEL, 4) as i32,
            name: "Aço Roxa",
            sell: 192,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::DARK_HEART_STONE, 1) as i32,
            name: "Pedra do Coração Negro Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::DARK_HEART_STONE, 2) as i32,
            name: "Pedra do Coração Negro Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::DARK_HEART_STONE, 3) as i32,
            name: "Pedra do Coração Negro Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::DARK_HEART_STONE, 4) as i32,
            name: "Pedra do Coração Negro Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::MOON_SHADOW_STONE, 1) as i32,
            name: "Pedra Sombra-da-Lua Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::MOON_SHADOW_STONE, 2) as i32,
            name: "Pedra Sombra-da-Lua Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::MOON_SHADOW_STONE, 3) as i32,
            name: "Pedra Sombra-da-Lua Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::MOON_SHADOW_STONE, 4) as i32,
            name: "Pedra Sombra-da-Lua Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::QUINTESSENCE, 1) as i32,
            name: "Quintessência Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::QUINTESSENCE, 2) as i32,
            name: "Quintessência Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::QUINTESSENCE, 3) as i32,
            name: "Quintessência Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::QUINTESSENCE, 4) as i32,
            name: "Quintessência Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::EXORCISM_BAUBLE, 1) as i32,
            name: "Berloque de Exorcismo Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::EXORCISM_BAUBLE, 2) as i32,
            name: "Berloque de Exorcismo Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::EXORCISM_BAUBLE, 3) as i32,
            name: "Berloque de Exorcismo Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::EXORCISM_BAUBLE, 4) as i32,
            name: "Berloque de Exorcismo Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::PLATINUM, 1) as i32,
            name: "Platina Cinza",
            sell: 3,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::PLATINUM, 2) as i32,
            name: "Platina Verde",
            sell: 12,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::PLATINUM, 3) as i32,
            name: "Platina Azul",
            sell: 48,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::PLATINUM, 4) as i32,
            name: "Platina Roxa",
            sell: 192,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ILLUMINATING_FRAGMENT, 1) as i32,
            name: "Fragmento Iluminante Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ILLUMINATING_FRAGMENT, 2) as i32,
            name: "Fragmento Iluminante Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ILLUMINATING_FRAGMENT, 3) as i32,
            name: "Fragmento Iluminante Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ILLUMINATING_FRAGMENT, 4) as i32,
            name: "Fragmento Iluminante Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ANIMA_STONE, 1) as i32,
            name: "Pedra de Ânima Cinza",
            sell: 5,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ANIMA_STONE, 2) as i32,
            name: "Pedra de Ânima Verde",
            sell: 20,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ANIMA_STONE, 3) as i32,
            name: "Pedra de Ânima Azul",
            sell: 80,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::ANIMA_STONE, 4) as i32,
            name: "Pedra de Ânima Roxa",
            sell: 320,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::SCALE, 1) as i32,
            name: "Escama Cinza",
            sell: 40,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::SCALE, 2) as i32,
            name: "Escama Verde",
            sell: 160,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::SCALE, 3) as i32,
            name: "Escama Azul",
            sell: 640,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::SCALE, 4) as i32,
            name: "Escama Roxa",
            sell: 2560,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::CLAW, 1) as i32,
            name: "Garra Cinza",
            sell: 40,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::CLAW, 2) as i32,
            name: "Garra Verde",
            sell: 160,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::CLAW, 3) as i32,
            name: "Garra Azul",
            sell: 640,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::CLAW, 4) as i32,
            name: "Garra Roxa",
            sell: 2560,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::HORN, 1) as i32,
            name: "Chifre Cinza",
            sell: 40,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::HORN, 2) as i32,
            name: "Chifre Verde",
            sell: 160,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::HORN, 3) as i32,
            name: "Chifre Azul",
            sell: 640,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::na_cor(item_id::HORN, 4) as i32,
            name: "Chifre Roxa",
            sell: 2560,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::COPPER as i32,
            name: "Cobre",
            sell: 1,
            buy: None,
            ord: None,
            stack: 999999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::DARKSTEEL as i32,
            name: "Darksteel",
            sell: 4,
            buy: None,
            ord: None,
            stack: 999999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::GLITTERING_POWDER as i32,
            name: "Pó Cintilante",
            sell: 60,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Dungeons: Marcas (conclusao) e Selo (entrada do topo). Vinculados.
        S {
            id: item_id::MARCAS_TEMPESTADE as i32,
            name: "Marcas da Tempestade",
            sell: 1,
            buy: None,
            ord: None,
            stack: 3000,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::SELO_TEMPESTADE as i32,
            name: "Selo da Tempestade",
            sell: 1,
            buy: None,
            ord: None,
            stack: 20,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Pergaminho de Teleporte: Alquimista, em cobre (`shared::viagem`).
        S {
            id: item_id::PERGAMINHO_TELEPORTE as i32,
            name: "Pergaminho de Teleporte",
            sell: 25,
            buy: Some(shared::viagem::PRECO_PERGAMINHO as i32),
            ord: None,
            stack: 999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Chaves lendarias (cor 5): so' chefe/raid de nivel 80+ (`shared::chaves`).
        S {
            id: item_id::SCALE_LENDARIA as i32,
            name: "Escama Lendária",
            sell: 10240,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::CLAW_LENDARIA as i32,
            name: "Garra Lendária",
            sell: 10240,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::HORN_LENDARIA as i32,
            name: "Chifre Lendário",
            sell: 10240,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::HIDE_LENDARIA as i32,
            name: "Couro Lendário",
            sell: 10240,
            buy: None,
            ord: None,
            stack: 9999,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        // Peixes (drop da pesca) — stackáveis, sem slot. icon_path setado
        // explicitamente abaixo pros sprites de Fish/ (ic/ir são sentinela -1
        // pra NÃO virar Items/r###_c## no backfill de icon_path).
        S {
            id: item_id::FISH_ANCHOVY as i32,
            name: "Anchova",
            sell: 8,
            buy: None,
            ord: None,
            stack: 99,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::FISH_CLOWNFISH as i32,
            name: "Peixe-palhaço",
            sell: 18,
            buy: None,
            ord: None,
            stack: 99,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::FISH_SURGEONFISH as i32,
            name: "Peixe-cirurgião",
            sell: 35,
            buy: None,
            ord: None,
            stack: 99,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
        S {
            id: item_id::FISH_PUFFERFISH as i32,
            name: "Baiacu",
            sell: 60,
            buy: None,
            ord: None,
            stack: 99,
            slot: None,
            lvl: 1,
            ic: -1,
            ir: -1,
            hp: (0, 0),
            mp: (0, 0),
            atk: (0, 0),
            def: (0, 0),
            dex: (0, 0),
            wis: (0, 0),
        },
    ];
    for s in seed {
        sqlx::query(
            "INSERT INTO items \
              (id, name, sell_price, buy_price, shop_order, stack_max, \
               equip_slot, item_level, icon_col, icon_row, \
               hp_min, hp_max, mp_min, mp_max, atk_min, atk_max, \
               def_min, def_max, dex_min, dex_max, wis_min, wis_max) \
             VALUES \
              ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) \
             ON CONFLICT (id) DO UPDATE SET \
               equip_slot = EXCLUDED.equip_slot, \
               item_level = EXCLUDED.item_level, \
               icon_col = EXCLUDED.icon_col, icon_row = EXCLUDED.icon_row, \
               hp_min = EXCLUDED.hp_min, hp_max = EXCLUDED.hp_max, \
               mp_min = EXCLUDED.mp_min, mp_max = EXCLUDED.mp_max, \
               atk_min = EXCLUDED.atk_min, atk_max = EXCLUDED.atk_max, \
               def_min = EXCLUDED.def_min, def_max = EXCLUDED.def_max, \
               dex_min = EXCLUDED.dex_min, dex_max = EXCLUDED.dex_max, \
               wis_min = EXCLUDED.wis_min, wis_max = EXCLUDED.wis_max \
             WHERE items.icon_col = 0 AND items.icon_row = 0",
        )
        .bind(s.id)
        .bind(s.name)
        .bind(s.sell)
        .bind(s.buy)
        .bind(s.ord)
        .bind(s.stack)
        .bind(s.slot)
        .bind(s.lvl)
        .bind(s.ic)
        .bind(s.ir)
        .bind(s.hp.0)
        .bind(s.hp.1)
        .bind(s.mp.0)
        .bind(s.mp.1)
        .bind(s.atk.0)
        .bind(s.atk.1)
        .bind(s.def.0)
        .bind(s.def.1)
        .bind(s.dex.0)
        .bind(s.dex.1)
        .bind(s.wis.0)
        .bind(s.wis.1)
        .execute(pool)
        .await?;

        // Force-update name pra refletir traduções PT->EN. ON CONFLICT do
        // INSERT acima não atualiza name (preserva edits do admin), então
        // garantimos aqui que o seed sobrescreve o nome em DBs existentes.
        sqlx::query("UPDATE items SET name = $1 WHERE id = $2")
            .bind(s.name)
            .bind(s.id)
            .execute(pool)
            .await?;
    }
    // Backfill icon_path apontando pros PNGs extraídos em
    // MMORPG/Assets/_Project/Resources/Items/r{row}_c{col}.png — naming
    // gerado direto a partir do (icon_col, icon_row) já populados.
    // Só seta se ainda for NULL (admin pode editar sem ser sobrescrito).
    sqlx::query(
        "UPDATE items \
         SET icon_path = 'Items/r' || lpad(icon_row::text, 3, '0') || \
                          '_c' || lpad(icon_col::text, 2, '0') \
         WHERE icon_path IS NULL AND icon_col >= 0 AND icon_row >= 0",
    )
    .execute(pool)
    .await?;

    // Peixes: icon vem dos sprites Fish/<Nome> (RemoteContent), não do
    // spritesheet de Items. Só seta se NULL (admin pode sobrescrever).
    for (id, addr) in [
        (item_id::FISH_ANCHOVY, "Fish/Anchovy"),
        (item_id::FISH_CLOWNFISH, "Fish/Clownfish"),
        (item_id::FISH_SURGEONFISH, "Fish/Surgeonfish"),
        (item_id::FISH_PUFFERFISH, "Fish/Pufferfish"),
    ] {
        sqlx::query("UPDATE items SET icon_path = $1 WHERE id = $2 AND icon_path IS NULL")
            .bind(addr)
            .bind(id as i32)
            .execute(pool)
            .await?;
    }

    // Enemy kinds — espelha o array hardcoded antigo. Tuple muito grande;
    // usa struct local pra clareza.
    // A tabela mora em `economy::KINDS_INICIAIS`: o simulador de balanceamento
    // le' os mesmos numeros.
    let kinds = &crate::economy::KINDS_INICIAIS;
    for e in kinds {
        sqlx::query(
            "INSERT INTO enemy_kinds (kind, name, hp_max, speed, attack_damage, attack_cooldown, \
             detect_range, attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
             tint_r, tint_g, tint_b, tint_a) VALUES \
             ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT (kind) DO NOTHING",
        )
        .bind(e.kind)
        .bind(e.name)
        .bind(e.hp)
        .bind(e.sp)
        .bind(e.dmg)
        .bind(e.cd)
        .bind(e.det)
        .bind(e.rng)
        .bind(e.kite)
        .bind(e.proj)
        .bind(e.xp)
        .bind(e.def)
        .bind(e.sz)
        .bind(e.t[0])
        .bind(e.t[1])
        .bind(e.t[2])
        .bind(e.t[3])
        .execute(pool)
        .await?;
    }
    // balanceamento_hp_mobs_v1: os mobs comuns ganham ~2,4x de HP (o chefe
    // fica). Com o HP antigo tudo morria em dois golpes e quem atirava matava
    // antes de o bicho chegar — ver docs/COMBATE.md, "Balanceamento". Uma vez
    // so', e so' em linha que AINDA tem o HP antigo do seed: ajuste de admin
    // fica. Banco novo ja' nasce com o valor novo e o UPDATE nao acha nada.
    sqlx::query("CREATE TABLE IF NOT EXISTS economy_migrations (name TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())")
        .execute(pool).await?;
    let hp_nova = sqlx::query("INSERT INTO economy_migrations(name) VALUES ('balanceamento_hp_mobs_v1') ON CONFLICT DO NOTHING")
        .execute(pool).await?.rows_affected() > 0;
    if hp_nova {
        for (kind, velho) in [
            (0, 50),
            (1, 120),
            (2, 35),
            (3, 40),
            (4, 45),
            (5, 200),
            (6, 45),
        ] {
            let novo = crate::economy::KINDS_INICIAIS[kind as usize].hp;
            sqlx::query("UPDATE enemy_kinds SET hp_max = $1 WHERE kind = $2 AND hp_max = $3")
                .bind(novo)
                .bind(kind)
                .bind(velho)
                .execute(pool)
                .await?;
        }
        tracing::info!("balanceamento_hp_mobs_v1: HP dos mobs comuns atualizado");
    }
    // M26: os oito tipos antigos (Grunt, Tank, Ranger, Ninja, Berserker...)
    // saem do jogo. O `INSERT ... DO NOTHING` acima nao renomeia linha que ja'
    // existe, entao quem ja' tinha banco ficaria com os nomes velhos. So' mexe
    // em linha que AINDA tem o nome antigo — nome editado pelo admin fica.
    // A tinta volta ao neutro: ela existia pra o mesmo lobo parecer oito
    // bichos diferentes, e agora cada um tem modelo proprio.
    //
    // O chefe vira lobo grande e passa a MORDER: alcance de corpo a corpo, sem
    // fugir, sem os cinco projeteis. Vai antes do rename porque se apoia no
    // nome velho pra rodar uma vez so'.
    sqlx::query(
        "UPDATE enemy_kinds SET attack_range = 2.6, kite_dist = NULL, proj_count = 1 \
         WHERE kind = 7 AND name = 'Boss'",
    )
    .execute(pool)
    .await?;
    for (kind, velho, novo) in [
        (0, "Grunt", "Lobo"),
        (1, "Tank", "Urso"),
        (2, "Ranger", "Pistoleiro"),
        (3, "Ninja", "Tigre"),
        (4, "Mago", "Mago"),
        (5, "Berserker", "Owlbear"),
        (6, "Arqueiro", "Arqueiro"),
        (7, "Boss", "Lobo Grande"),
    ] {
        sqlx::query(
            "UPDATE enemy_kinds SET name = $3, tint_r = 1, tint_g = 1, tint_b = 1, tint_a = 1, \
             size_scale = CASE WHEN kind IN (2, 3, 4, 6) THEN 1.0 ELSE size_scale END \
             WHERE kind = $1 AND name = $2",
        )
        .bind(kind)
        .bind(velho)
        .bind(novo)
        .execute(pool)
        .await?;
    }

    // Backfill do loot_item_level — antes hardcoded em world.rs.
    // Só seta se NULL (admin pode editar via web admin sem ser sobrescrito).
    let item_levels: &[(i32, i32)] = &[
        (0, 10), // Lobo
        (1, 15), // Urso
        (2, 10), // Pistoleiro
        (3, 25), // Tigre
        (4, 20), // Mago
        (5, 30), // Owlbear
        (6, 10), // Arqueiro
        (7, 50), // Lobo Grande
    ];
    for (kind, lvl) in item_levels {
        sqlx::query("UPDATE enemy_kinds SET loot_item_level = $2 WHERE kind = $1 AND loot_item_level IS NULL")
            .bind(kind).bind(lvl).execute(pool).await?;
    }

    // O loot de mobs e migrado em loot_mobs, depois da conversao de IDs.

    // Seed dos shops dos vendors. shop_id=1 fica como generalista (legacy
    // do shop antigo). Demais são especializados.
    let shop_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendor_shops")
        .fetch_one(pool)
        .await?;
    if shop_count == 0 {
        let shops: &[(i32, &str, &[u16])] = &[
            (
                1,
                "Merchant",
                &[
                    item_id::HEALTH_POTION,
                    item_id::MANA_POTION,
                    item_id::GREATER_HEAL,
                    item_id::GREATER_MANA,
                    item_id::STAMINA_POTION,
                    item_id::ESPADA_E_ESCUDO,
                    item_id::KATANA,
                    item_id::PISTOLAS,
                    item_id::ANEL_MAGICO,
                    item_id::ARMADURA_MEDIA,
                ],
            ),
            (
                2,
                "Swordsmith",
                &[
                    item_id::ESPADA_E_ESCUDO,
                    item_id::KATANA,
                    item_id::MANTO_DO_GUERREIRO,
                    item_id::BAINHA,
                ],
            ),
            (
                3,
                "Alchemist",
                &[
                    item_id::HEALTH_POTION,
                    item_id::MANA_POTION,
                    item_id::GREATER_HEAL,
                    item_id::GREATER_MANA,
                    item_id::STAMINA_POTION,
                ],
            ),
            (
                4,
                "Blacksmith",
                &[
                    item_id::ARMADURA_LEVE,
                    item_id::ARMADURA_MEDIA,
                    item_id::ARMADURA_PESADA,
                ],
            ),
            (
                5,
                "Mage",
                &[
                    item_id::ANEL_MAGICO,
                    item_id::MANTO_DO_MAGO,
                    item_id::BRINCO,
                    item_id::AMULETO,
                    item_id::BRACELETE,
                    item_id::CINTO,
                ],
            ),
            (
                6,
                "Archer",
                &[item_id::PISTOLAS, item_id::COLDRE, item_id::STAMINA_POTION],
            ),
        ];
        for (sid, name, items) in shops {
            sqlx::query(
                "INSERT INTO vendor_shops (shop_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(sid)
            .bind(*name)
            .execute(pool)
            .await?;
            for (i, &item) in items.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) \
                     VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                )
                .bind(sid)
                .bind(item as i32)
                .bind(i as i32)
                .execute(pool)
                .await?;
            }
        }
        tracing::info!("economy seed: {} vendor shops inseridos", shops.len());
    }

    // Adiciona itens novos aos vendors existentes (idempotente via ON CONFLICT).
    let new_shop_items: &[(i32, u16)] = &[
        // O equipamento novo em cada loja (docs/COMBATE.md). O Mercador (1) e' o
        // unico que sempre existe no mundo, entao vende as quatro armas.
        (1, item_id::ESPADA_E_ESCUDO),
        (1, item_id::KATANA),
        (1, item_id::PISTOLAS),
        (1, item_id::ANEL_MAGICO),
        (1, item_id::ARMADURA_MEDIA),
        (2, item_id::ESPADA_E_ESCUDO),
        (2, item_id::KATANA),
        (2, item_id::MANTO_DO_GUERREIRO),
        (2, item_id::BAINHA),
        (4, item_id::ARMADURA_LEVE),
        (4, item_id::ARMADURA_MEDIA),
        (4, item_id::ARMADURA_PESADA),
        (5, item_id::ANEL_MAGICO),
        (5, item_id::MANTO_DO_MAGO),
        (5, item_id::BRINCO),
        (5, item_id::AMULETO),
        (5, item_id::BRACELETE),
        (5, item_id::CINTO),
        (6, item_id::PISTOLAS),
        (6, item_id::COLDRE),
        // O Alquimista da vila (loja de pocoes) vende o pergaminho de teleporte.
        (shared::vila::LOJA_DE_POCOES as i32, item_id::PERGAMINHO_TELEPORTE),
        // Recursos T1 vendaveis no Mercador — facilita early game.
        (1, item_id::WOOD_T1),
        (1, item_id::LEATHER_T1),
    ];
    for (sid, item) in new_shop_items {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vendor_shop_items WHERE shop_id = $1 AND item_id = $2",
        )
        .bind(sid)
        .bind(*item as i32)
        .fetch_one(pool)
        .await?;
        if exists == 0 {
            let next_order: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM vendor_shop_items WHERE shop_id = $1"
            )
            .bind(sid).fetch_one(pool).await?;
            sqlx::query(
                "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(sid)
            .bind(*item as i32)
            .bind(next_order)
            .execute(pool)
            .await?;
        }
    }

    // Recursos T1 — buy_price = sell_price * 3 (custa 3x o preco de venda).
    // Idempotente: so seta se ainda for NULL/0 — admin pode editar via web sem
    // ser sobrescrito.
    let t1_resources: &[u16] = &[item_id::WOOD_T1, item_id::LEATHER_T1];
    for &iid in t1_resources {
        sqlx::query(
            "UPDATE items SET buy_price = sell_price * 3 \
             WHERE id = $1 \
               AND sell_price > 0 \
               AND (buy_price IS NULL OR buy_price = 0)",
        )
        .bind(iid as i32)
        .execute(pool)
        .await?;
    }

    // Farm node drops — seed só se vazio. Replica o comportamento legado:
    // 1 row de material principal por (kind, tier) com qty escalando, +
    // 1 row de gold com chance proporcional ao tier.
    let farm_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM farm_node_drops")
        .fetch_one(pool)
        .await?;
    if farm_count == 0 {
        // (kind, tier, item_id, qty_min, qty_max, chance)
        // qty_base = 2 + tier; qty_extra = +0..2 → range [base, base+2].
        let farm_drops: &[(&str, i32, u16, i32, i32, f32)] = &[
            // Tree → Madeira
            ("Tree", 1, item_id::WOOD_T1, 3, 5, 1.0),
            ("Tree", 2, item_id::WOOD_T2, 4, 6, 1.0),
            ("Tree", 3, item_id::WOOD_T3, 5, 7, 1.0),
            ("Tree", 4, item_id::WOOD_T4, 6, 8, 1.0),
            // Rock → Mineral
            ("Rock", 1, item_id::na_cor(item_id::STEEL, 1), 3, 5, 1.0),
            ("Rock", 2, item_id::na_cor(item_id::STEEL, 2), 4, 6, 1.0),
            ("Rock", 3, item_id::na_cor(item_id::STEEL, 3), 5, 7, 1.0),
            ("Rock", 4, item_id::na_cor(item_id::STEEL, 4), 6, 8, 1.0),
            // Flower → Couro (default legado; admin troca pra herbal/etc.)
            ("Flower", 1, item_id::LEATHER_T1, 3, 5, 1.0),
            ("Flower", 2, item_id::LEATHER_T2, 4, 6, 1.0),
            ("Flower", 3, item_id::LEATHER_T3, 5, 7, 1.0),
            ("Flower", 4, item_id::LEATHER_T4, 6, 8, 1.0),
            // Coleta nao da OURO: ele ficou raro (chefe, dungeon, mercado,
            // venda ao NPC, calendario). A moeda que sai do chao e' o COBRE —
            // a pedra ja' dava, a arvore e a flor passam a dar (docs/ECONOMIA.md).
            ("Tree", 1, item_id::COPPER, 8, 16, 1.0),
            ("Tree", 2, item_id::COPPER, 12, 24, 1.0),
            ("Tree", 3, item_id::COPPER, 16, 32, 1.0),
            ("Tree", 4, item_id::COPPER, 20, 40, 1.0),
            ("Flower", 1, item_id::COPPER, 8, 16, 1.0),
            ("Flower", 2, item_id::COPPER, 12, 24, 1.0),
            ("Flower", 3, item_id::COPPER, 16, 32, 1.0),
            ("Flower", 4, item_id::COPPER, 20, 40, 1.0),
        ];
        for (kind, tier, item, qmin, qmax, chance) in farm_drops {
            sqlx::query(
                "INSERT INTO farm_node_drops (kind, tier, item_id, qty_min, qty_max, chance) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(*kind)
            .bind(tier)
            .bind(*item as i32)
            .bind(qmin)
            .bind(qmax)
            .bind(chance)
            .execute(pool)
            .await?;
        }
        tracing::info!(
            "economy seed: {} farm node drops inseridos",
            farm_drops.len()
        );
    }

    // M25: a pedra deixou de dar "Mineral" e passou a dar os materiais de
    // craft de verdade. Roda uma vez: se o Aco cinza ja' esta' na tabela, o
    // trabalho ja' foi feito e nao se mexe mais (admin pode ter ajustado).
    let ja: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM farm_node_drops WHERE kind = 'Rock' AND item_id = $1",
    )
    .bind(item_id::STEEL as i32)
    .fetch_one(pool)
    .await?;
    if ja == 0 {
        sqlx::query("DELETE FROM farm_node_drops WHERE kind = 'Rock'")
            .execute(pool)
            .await?;
        // A tabela mora em `economy::linhas_da_pedra` — a mesma que os testes
        // de proporcao usam. Ver docs/ECONOMIA_DE_CRAFT.md.
        let mut n = 0;
        for (tier, id, qmin, qmax, chance) in crate::economy::linhas_da_pedra() {
            sqlx::query(
                "INSERT INTO farm_node_drops (kind, tier, item_id, qty_min, qty_max, chance) \
                 VALUES ('Rock', $1, $2, $3, $4, $5)",
            )
            .bind(tier as i32)
            .bind(id as i32)
            .bind(qmin)
            .bind(qmax)
            .bind(chance)
            .execute(pool)
            .await?;
            n += 1;
        }
        tracing::info!("economy: {n} drops de pedra (materiais de craft) inseridos");
    }

    // M27: os itens do sistema antigo viram os do formato novo (docs/COMBATE.md)
    // — a arma pelo conjunto que ela ja' era, o escudo pela secundaria do
    // conjunto da arma, a armadura pelo peso, colar em amuleto, anel em
    // brinco ou bracelete, cinto em cinto. O que nao tem para onde ir (elmo,
    // calca, bota, luva, capa, os aneis de capitulo, gema, barra, escama de
    // dragao, minerio, ferramenta, as faixas de tier da Fase G) sai.
    //
    // Roda em todo boot e e' idempotente: depois da primeira vez nao sobra id
    // velho pra trocar nem pra apagar.
    {
        const PRA_NOVO: &str = "CASE \
            WHEN item_id IN (3,13,24,25,45,46,50) THEN 400 \
            WHEN item_id IN (12,26,51) THEN 401 \
            WHEN item_id IN (14,27,47) THEN 402 \
            WHEN item_id IN (6,15,48,49) THEN 403 \
            WHEN item_id IN (7,28) THEN 404 \
            WHEN item_id IN (16,18) THEN 408 \
            WHEN item_id = 4 THEN 409 \
            WHEN item_id = 17 THEN 410 \
            WHEN item_id = 5 THEN 411 \
            WHEN item_id IN (19,29,30,43,44) THEN 412 \
            WHEN item_id = 20 THEN 413 \
            WHEN item_id IN (39,40) THEN 414 END";
        const TEM_NOVO: &str = "item_id IN (3,4,5,6,7,12,13,14,15,16,17,18,19,20,24,25,26,27,28,29,30,39,40,43,44,45,46,47,48,49,50,51)";
        const SO_VELHO: &str = "((item_id BETWEEN 3 AND 51 AND item_id NOT IN (8,9,10,11)) \
            OR item_id BETWEEN 68 AND 71 OR item_id BETWEEN 80 AND 95 OR item_id BETWEEN 102 AND 268)";
        let na_coluna = |sql: &str, col: &str| sql.replace("item_id", col);
        let mut feitos = 0u64;
        // Banco novo: `craft_recipes` so' nasce no `recipes::init`, depois
        // daqui. Sem esta guarda o servidor nao subia num banco vazio.
        let tem_receitas: bool =
            sqlx::query_scalar("SELECT to_regclass('craft_recipes') IS NOT NULL")
                .fetch_one(pool)
                .await?;
        for sql in [
            format!("UPDATE equipment SET item_id = {PRA_NOVO} WHERE {TEM_NOVO}"),
            format!("UPDATE inventory SET item_id = {PRA_NOVO} WHERE {TEM_NOVO}"),
            format!("UPDATE vault SET item_id = {PRA_NOVO} WHERE {TEM_NOVO}"),
            // o anel virou brinco ou bracelete: o slot acompanha
            "UPDATE equipment SET slot = 'earring' WHERE slot = 'ring' AND item_id = 411".to_string(),
            "UPDATE equipment SET slot = 'bracelet' WHERE slot = 'ring' AND item_id = 413".to_string(),
            // a secundaria e' a do conjunto da arma
            "UPDATE equipment e SET item_id = CASE (SELECT w.item_id FROM equipment w \
                 WHERE w.character_name = e.character_name AND w.slot = 'weapon') \
                 WHEN 401 THEN 405 WHEN 402 THEN 406 WHEN 403 THEN 407 ELSE 404 END \
             WHERE e.slot = 'offhand' AND e.item_id BETWEEN 404 AND 407".to_string(),
            "DELETE FROM equipment WHERE slot NOT IN ('weapon','offhand','armor','earring','necklace','bracelet','belt')".to_string(),
            format!("DELETE FROM equipment WHERE {SO_VELHO}"),
            format!("DELETE FROM inventory WHERE {SO_VELHO}"),
            format!("DELETE FROM vault WHERE {SO_VELHO}"),
            format!("DELETE FROM loot_drops WHERE {SO_VELHO}"),
            format!("DELETE FROM vendor_shop_items WHERE {SO_VELHO}"),
            format!("DELETE FROM farm_node_drops WHERE {SO_VELHO}"),
            // receita: so' os barcos ficam, e o Lylian troca minerio por aco
            // (as de equipamento, 1000+, sao semeadas pelo `recipes` e ficam)
            "DELETE FROM craft_recipes WHERE id NOT IN (200, 201) AND id < 1000".to_string(),
            "UPDATE craft_recipes SET inputs = '[[60,100],[61,100],[300,100],[65,100]]' WHERE id = 200".to_string(),
            format!("UPDATE characters SET starting_weapon = {} WHERE {}",
                na_coluna(PRA_NOVO, "starting_weapon"), na_coluna(TEM_NOVO, "starting_weapon")),
            format!("UPDATE enemy_kinds SET build_weapon = {} WHERE {}",
                na_coluna(PRA_NOVO, "build_weapon"), na_coluna(TEM_NOVO, "build_weapon")),
            format!("DELETE FROM items WHERE {}", na_coluna(SO_VELHO, "id")),
        ] {
            if !tem_receitas && sql.contains("craft_recipes") {
                continue;
            }
            feitos += sqlx::query(&sql).execute(pool).await?.rows_affected();
        }
        if feitos > 0 {
            tracing::info!("M27: {feitos} linhas migradas pro equipamento novo");
        }
    }

    // M28: o COBRE virou a moeda do dia a dia e o OURO ficou raro
    // (docs/ECONOMIA.md, decisao do dono de 17/09/2026).
    //
    //   * a loja do NPC cobra em cobre, entao o preco de equipamento sobe 4x:
    //     na escala do ouro ele custava menos que uma pedra rende de cobre;
    //   * coleta nao da mais ouro (pedra, arvore e flor) — arvore e flor
    //     passam a dar cobre, que e' o que elas rendiam de moeda;
    //   * o ouro passa a vir de chefe, bau de dungeon, mercado, venda ao NPC e
    //     calendario (esses tres ultimos ja' eram).
    //
    // Idempotente: marcado em `economy_migrations`.
    {
        let nova = sqlx::query("INSERT INTO economy_migrations(name) VALUES ('cobre_e_a_moeda_v1') ON CONFLICT DO NOTHING")
            .execute(pool).await?.rows_affected() > 0;
        if nova {
            sqlx::query("UPDATE items SET buy_price = buy_price * 4 WHERE buy_price IS NOT NULL AND id BETWEEN 400 AND 499")
                .execute(pool).await?;
            sqlx::query("DELETE FROM farm_node_drops WHERE item_id = $1")
                .bind(item_id::GOLD as i32)
                .execute(pool)
                .await?;
            for (kind, tier, qmin, qmax) in [
                ("Tree", 1, 8, 16),
                ("Tree", 2, 12, 24),
                ("Tree", 3, 16, 32),
                ("Tree", 4, 20, 40),
                ("Flower", 1, 8, 16),
                ("Flower", 2, 12, 24),
                ("Flower", 3, 16, 32),
                ("Flower", 4, 20, 40),
            ] {
                sqlx::query(
                    "INSERT INTO farm_node_drops (kind, tier, item_id, qty_min, qty_max, chance) VALUES ($1,$2,$3,$4,$5,1.0) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(kind).bind(tier).bind(item_id::COPPER as i32).bind(qmin).bind(qmax)
                .execute(pool).await?;
            }
            tracing::info!(
                "M28: cobre e' a moeda — loja 4x, coleta sem ouro, arvore e flor com cobre"
            );
        }
    }

    crate::loot_mobs::migrar(pool).await?;
    Ok(())
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    load(pool, None).await
}

/// Um personagem direto do banco — a verdade entre processos. A copia em
/// memoria de cada canal envelhece assim que o jogador joga noutro.
pub async fn load_one(pool: &PgPool, name: &str) -> Result<Option<CharacterRow>> {
    Ok(load(pool, Some(name)).await?.remove(name))
}

/// Os personagens da conta, direto do banco. E' a lista que o login mostra:
/// o cache de cada processo e' do momento em que ele subiu, e quem foi criado
/// DEPOIS noutro processo (outra ilha, outro canal) nao estava nele — viajar
/// pra Geleira mostrava "crie seu personagem" (19/09/2026).
pub async fn load_da_conta(pool: &PgPool, account_id: i64) -> Result<Vec<CharacterRow>> {
    let nomes: Vec<String> =
        sqlx::query_scalar("SELECT name FROM characters WHERE account_id = $1")
            .bind(account_id)
            .fetch_all(pool)
            .await?;
    let mut v = Vec::with_capacity(nomes.len());
    for n in nomes {
        if let Some(r) = load_one(pool, &n).await? {
            v.push(r);
        }
    }
    Ok(v)
}

async fn load(pool: &PgPool, so: Option<&str>) -> Result<HashMap<String, CharacterRow>> {
    let onde = if so.is_some() { " WHERE name = $1" } else { "" };
    macro_rules! busca {
        ($t:ty, $colunas:expr) => {{
            let sql = format!("SELECT {} FROM characters{onde}", $colunas);
            let mut q = sqlx::query_as::<_, $t>(&sql);
            if let Some(n) = so {
                q = q.bind(n);
            }
            q.fetch_all(pool).await?
        }};
    }
    let rows = busca!(
        (
            String,
            f32,
            f32,
            i32,
            i32,
            i64,
            i64,
            i64,
            i32,
            Vec<i32>,
            i32,
            i32,
            Option<i16>,
            Option<f32>,
            Option<f32>,
            Option<i16>
        ),
        "name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points, \
         skill_points_earned, skill_points_spent, boat_kind, boat_x, boat_y, boat_dir"
    );
    // Query separada pra account_id + visual_json + gold (tuple FromRow limit 16).
    let extras = busca!(
        (String, Option<i64>, Option<String>, i64),
        "name, account_id, visual_json, gold"
    );
    let extras_map: HashMap<String, (Option<i64>, Option<String>, u64)> = extras
        .into_iter()
        .map(|(n, a, v, g)| (n, (a, v, g.max(0) as u64)))
        .collect();
    // Faction — query separada (TEXT). Parse tolerante; default Peacemain.
    let faction_rows = busca!((String, String), "name, faction");
    let faction_map: HashMap<String, shared::Faction> = faction_rows
        .into_iter()
        .map(|(n, f)| (n, shared::Faction::from_str_lenient(&f).unwrap_or_default()))
        .collect();
    // Tutorial concluído (epoch). NULL = nunca → login no mundo redireciona pro tutorial.
    let tut_rows = busca!(
        (String, Option<i64>),
        "name, EXTRACT(EPOCH FROM last_tutorial_completed)::BIGINT"
    );
    let tut_map: HashMap<String, Option<i64>> = tut_rows.into_iter().collect();
    // Boat 2.5D extras: yaw/sail/anchor + mounted_local. Tudo opcional pra
    // compat com rows legacy (sao NULL quando antigos).
    type BoatExtras = (
        Option<f32>,
        Option<i16>,
        Option<f32>,
        Option<bool>,
        Option<f32>,
        Option<f32>,
    );
    let boat_extras = busca!(
        (String, Option<f32>, Option<i16>, Option<f32>, Option<bool>, Option<f32>, Option<f32>),
        "name, boat_yaw, boat_sail_pos, boat_sail_angle, boat_anchor_dropped, mounted_local_x, mounted_local_y"
    );
    let boat_extras_map: HashMap<String, BoatExtras> = boat_extras
        .into_iter()
        .map(|(n, y, sp, sa, a, lx, ly)| (n, (y, sp, sa, a, lx, ly)))
        .collect();
    // Mana, stamina e zona. NULL = row de antes das colunas.
    let vida = busca!(
        (String, Option<f32>, Option<f32>, Option<String>),
        "name, mp, stamina, zona"
    );
    let vida_map: HashMap<String, (Option<f32>, Option<f32>, Option<String>)> = vida
        .into_iter()
        .map(|(n, m, s, z)| (n, (m, s, z)))
        .collect();
    let bonus = busca!((String, i64), "name, xp_bonus_ate");
    let bonus_map: HashMap<String, i64> = bonus.into_iter().collect();
    let drop = busca!(
        (String, i64, i64, String),
        "name, fortuna_ate, sorte_ate, barra_json"
    );
    let drop_map: HashMap<String, (i64, i64, String)> = drop
        .into_iter()
        .map(|(n, f, s, b)| (n, (f, s, b)))
        .collect();
    let mortes = busca!(
        (String, String, i64, i32),
        "name, mortes_json, recuperacoes_dia, recuperacoes_usadas"
    );
    let dungeon = busca!((String, String), "name, dungeon_json");
    let dungeon_map: HashMap<String, String> = dungeon.into_iter().collect();
    // Dados da conta: por account_id, fora do `busca!` (e' outra tabela).
    let conta_map: HashMap<String, String> = {
        let sql = format!(
            "SELECT c.name, COALESCE(d.dados_json, '') FROM characters c LEFT JOIN dungeon_contas d ON d.account_id = c.account_id{}",
            if so.is_some() { " WHERE c.name = $1" } else { "" }
        );
        let mut q = sqlx::query_as::<_, (String, String)>(&sql);
        if let Some(n) = so {
            q = q.bind(n);
        }
        q.fetch_all(pool).await?.into_iter().collect()
    };
    let mortes_map: HashMap<String, (String, i64, i32)> = mortes
        .into_iter()
        .map(|(n, m, d, u)| (n, (m, d, u)))
        .collect();
    let prefs = busca!((String, String), "name, preferencias_json");
    let prefs_map: HashMap<String, String> = prefs.into_iter().collect();

    let mut out = HashMap::with_capacity(rows.len());
    for (
        name,
        x,
        y,
        hp,
        max_hp,
        xp,
        fame,
        aura,
        unspent,
        allocated_vec,
        sp_earned,
        sp_spent,
        boat_kind,
        boat_x,
        boat_y,
        boat_dir,
    ) in rows
    {
        let (account_id, visual_json, gold) =
            extras_map.get(&name).cloned().unwrap_or((None, None, 0));
        let faction = faction_map.get(&name).copied().unwrap_or_default();
        let inv = load_inventory(pool, &name).await?;
        let equip = load_equipment(pool, &name).await?;
        let vault = load_vault(pool, &name).await?;
        let profs = load_proficiencies(pool, &name).await?;
        let (quests, faction_points) = crate::quests::load_char(pool, &name)
            .await
            .unwrap_or_default();
        let mut allocated = [0u32; shared::STAT_COUNT];
        for (i, v) in allocated_vec
            .into_iter()
            .enumerate()
            .take(shared::STAT_COUNT)
        {
            allocated[i] = v.max(0) as u32;
        }
        // Boat extras (Boat 2.5D — yaw/sail/anchor + mounted_local).
        let extras = boat_extras_map.get(&name).copied();
        let (b_yaw, b_sail_pos, b_sail_angle, b_anchor, ml_x, ml_y) =
            extras.unwrap_or((None, None, None, None, None, None));
        let boat = match (boat_kind, boat_x, boat_y, boat_dir) {
            (Some(k), Some(bx), Some(by), Some(d)) => {
                let dir = d.max(0) as u8;
                // Fallback: derive yaw de dir se nao tem boat_yaw salvo.
                let yaw = b_yaw.unwrap_or((dir as f32) * std::f32::consts::FRAC_PI_4);
                Some(PersistedBoat {
                    kind: k.max(0) as u16,
                    pos: Vec2::new(bx, by),
                    dir,
                    yaw,
                    sail_position: b_sail_pos.map(|s| s.max(0) as u8).unwrap_or(0),
                    sail_angle: b_sail_angle.unwrap_or(0.0),
                    anchor_dropped: b_anchor.unwrap_or(true),
                })
            }
            _ => None,
        };
        let mounted_local = match (ml_x, ml_y) {
            (Some(x), Some(y)) => Some(Vec2::new(x, y)),
            _ => None,
        };
        let visual: Option<shared::VisualConfig> = visual_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok());
        let last_tut = tut_map.get(&name).cloned().flatten();
        let (mp, stamina, zona) = vida_map.get(&name).cloned().unwrap_or_default();
        let xp_bonus_ate = bonus_map.get(&name).copied().unwrap_or(0);
        let (fortuna_ate, sorte_ate, barra_json) = drop_map.get(&name).cloned().unwrap_or_default();
        let (mortes_json, recuperacoes_dia, recuperacoes_usadas) =
            mortes_map.get(&name).cloned().unwrap_or_default();
        let preferencias_json = prefs_map.get(&name).cloned().unwrap_or_default();
        let dungeon_json = dungeon_map.get(&name).cloned().unwrap_or_default();
        let conta_dungeon_json = conta_map.get(&name).cloned().unwrap_or_default();
        out.insert(
            name.clone(),
            CharacterRow {
                name,
                pos: Vec2::new(x, y),
                hp: Health {
                    current: hp,
                    max: max_hp,
                },
                xp: xp.max(0) as u64,
                gold,
                boat,
                mounted_local,
                inventory: inv,
                equipment: equip,
                vault,
                fame: fame.max(0) as u64,
                aura: aura.max(0) as u64,
                proficiencies: profs,
                unspent_points: unspent.max(0) as u32,
                allocated_points: allocated,
                skill_points_earned: sp_earned.max(0) as u32,
                skill_points_spent: sp_spent.max(0) as u32,
                account_id,
                visual,
                faction,
                quests,
                faction_points,
                last_tutorial_completed: last_tut,
                mp,
                stamina,
                zona,
                xp_bonus_ate,
                fortuna_ate,
                sorte_ate,
                barra_json,
                mortes_json,
                recuperacoes_dia,
                recuperacoes_usadas,
                preferencias_json,
                dungeon_json,
                conta_dungeon_json,
                presenca_aplicados: Vec::new(),
                correio_recibos: Vec::new(),
            },
        );
    }
    Ok(out)
}

/// Cria personagem inicial via tela de criacao do cliente. Insere row com
/// account_id + visual escolhido + arma inicial em inventory[0]. Retorna
/// `false` se name ja em uso (UNIQUE PK violation).
pub async fn create_character(
    pool: &PgPool,
    account_id: i64,
    name: &str,
    visual: &shared::VisualConfig,
    starting_weapon: u16,
    spawn: Vec2,
    faction: shared::Faction,
) -> Result<bool> {
    let visual_json = serde_json::to_string(visual)?;
    let allocated_zero: Vec<i32> = vec![0; shared::STAT_COUNT];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let base = shared::base_player_stats();
    // Insert character row. ON CONFLICT(name) DO NOTHING + check rows_affected
    // pra detectar nome duplicado.
    let res = sqlx::query(
        "INSERT INTO characters
         (name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points,
          skill_points_earned, skill_points_spent,
          account_id, visual_json, starting_weapon, faction, updated)
         VALUES ($1, $2, $3, $4, $5, 0, 0, 0, 0, $6, 1, 0, $7, $8, $9, $10, $11)
         ON CONFLICT(name) DO NOTHING",
    )
    .bind(name)
    .bind(spawn.x)
    .bind(spawn.y)
    .bind(base.hp_max)
    .bind(base.hp_max)
    .bind(&allocated_zero)
    .bind(account_id)
    .bind(&visual_json)
    .bind(starting_weapon as i16)
    .bind(faction.as_db_str())
    .bind(now)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(false);
    }
    // Arma inicial = 0 → personagem nasce SEM arma (o tutorial entrega a T1).
    // Só popula inventário/equip se um id de arma válido foi passado (compat).
    if starting_weapon != 0 {
        // So' equipada. Ia tambem uma COPIA pra bolsa (slot 0), e com o
        // Aprimorar isso virava duas pecas iguais de graca. A instancia
        // (Comum, Tier I) entra no primeiro login (`craft::instancia_inicial`).
        // Equipa a arma na slot weapon (mainhand).
        sqlx::query(
            "INSERT INTO equipment (character_name, slot, item_id)
             VALUES ($1, 'weapon', $2)
             ON CONFLICT (character_name, slot) DO UPDATE SET item_id = EXCLUDED.item_id",
        )
        .bind(name)
        .bind(starting_weapon as i32)
        .execute(pool)
        .await?;
    }
    Ok(true)
}

async fn load_proficiencies(pool: &PgPool, char_name: &str) -> Result<[u64; shared::PROF_COUNT]> {
    let mut arr = [0u64; shared::PROF_COUNT];
    let rows = sqlx::query_as::<_, (i32, i64)>(
        "SELECT prof_kind, xp FROM proficiencies WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (kind, xp) in rows {
        if kind < 0 {
            continue;
        }
        let idx = kind as usize;
        if idx < arr.len() {
            arr[idx] = xp.max(0) as u64;
        }
    }
    Ok(arr)
}

async fn load_vault(pool: &PgPool, char_name: &str) -> Result<Vec<shared::InventorySlot>> {
    let mut slots = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
    let rows = sqlx::query_as::<_, (i32, i32, i32, Option<String>)>(
        "SELECT slot, item_id, qty, instance_data FROM vault WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (slot, item_id, qty, inst_json) in rows {
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS {
            continue;
        }
        if qty <= 0 {
            continue;
        }
        slots[slot as usize] = shared::InventorySlot {
            item_id: item_id as u16,
            qty: qty as u32,
            instance: inst_json.and_then(|s| serde_json::from_str(&s).ok()),
        };
    }
    Ok(slots)
}

async fn load_equipment(pool: &PgPool, char_name: &str) -> Result<shared::Equipment> {
    let rows = sqlx::query_as::<_, (String, i32, Option<String>)>(
        "SELECT slot, item_id, instance_data FROM equipment WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    let mut eq = shared::Equipment::default();
    for (slot, item_id, inst_json) in rows {
        let iid = item_id as u16;
        let inst: Option<shared::items::ItemInstance> =
            inst_json.and_then(|s| serde_json::from_str(&s).ok());
        if let Some(s) = shared::EquipSlot::de_db_str(slot.as_str()) {
            eq.set(s, Some(iid), inst);
        }
    }
    Ok(eq)
}

async fn load_inventory(pool: &PgPool, char_name: &str) -> Result<Vec<shared::InventorySlot>> {
    let mut slots = vec![shared::InventorySlot::default(); shared::INVENTORY_SLOTS];
    let rows = sqlx::query_as::<_, (i32, i32, i32, Option<String>)>(
        "SELECT slot, item_id, qty, instance_data FROM inventory WHERE character_name = $1",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    for (slot, item_id, qty, inst_json) in rows {
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS {
            continue;
        }
        if qty <= 0 {
            continue;
        }
        slots[slot as usize] = shared::InventorySlot {
            item_id: item_id as u16,
            qty: qty as u32,
            instance: inst_json.and_then(|s| serde_json::from_str(&s).ok()),
        };
    }
    Ok(slots)
}

/// Mensagem enviada pela thread do mundo pro writer task.
#[derive(Debug)]
pub struct SaveBatch {
    pub rows: Vec<CharacterRow>,
    /// Registros do mercado desses personagens, gravados na MESMA transacao
    /// (docs/MERCADO.md): o item sai da bolsa e entra na saida juntos.
    pub mercado: Vec<crate::mercado::Registro>,
}

/// Log fire-and-forget de um drop (usado pro relatório no admin). Erro só
/// vira `tracing::warn!` — não queremos derrubar o tick do mundo se o DB
/// estiver lento.
pub fn log_drop(
    pool: PgPool,
    enemy_kind: u16,
    item_id: u16,
    qty: u32,
    rarity: u8,
    item_level: u16,
    refinement: u8,
) {
    tokio::spawn(async move {
        let r = sqlx::query(
            "INSERT INTO item_drops_log \
              (enemy_kind, item_id, qty, rarity, item_level, refinement) \
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(enemy_kind as i32)
        .bind(item_id as i32)
        .bind(qty as i32)
        .bind(rarity as i16)
        .bind(item_level as i32)
        .bind(refinement as i16)
        .execute(&pool)
        .await;
        if let Err(e) = r {
            tracing::warn!("drop log failed: {e:?}");
        }
    });
}

/// Spawn de task background que consome SaveBatch e escreve no DB.
/// Consome o pool — se precisar acessar DB em outros lugares, clone antes.
pub fn spawn_writer(pool: PgPool) -> mpsc::UnboundedSender<SaveBatch> {
    let (tx, mut rx) = mpsc::unbounded_channel::<SaveBatch>();
    tokio::spawn(async move {
        // Save que falhou vai INTEIRO no proximo: as linhas dos personagens E
        // os registros do mercado. So' o registro sem a linha gravaria a
        // saida no mercado com o item ainda na bolsa do banco — duplicado.
        let mut atrasado: Option<SaveBatch> = None;
        while let Some(batch) = rx.recv().await {
            let batch = match atrasado.take() {
                Some(velho) => juntar_atrasado(velho, batch),
                None => batch,
            };
            let t0 = std::time::Instant::now();
            match write_batch(&pool, &batch).await {
                Ok(()) => {
                    crate::telemetria::conta("save", "ok", 1);
                    crate::telemetria::conta("save_ms", "", t0.elapsed().as_millis() as i64);
                    crate::telemetria::conta("save_linhas", "", batch.rows.len() as i64);
                    crate::telemetria::conta("save_mercado", "", batch.mercado.len() as i64);
                    if !batch.mercado.is_empty() {
                        crate::mercado::acordar_relay();
                    }
                }
                Err(e) => {
                    crate::telemetria::conta("save", "falha", 1);
                    crate::telemetria::conta(
                        "save_atrasado_mercado",
                        "",
                        batch.mercado.len() as i64,
                    );
                    tracing::warn!("persist write failed: {e:?}");
                    atrasado = Some(batch);
                }
            }
        }
        tracing::debug!("persist writer task exiting");
    });
    tx
}

/// Um save que falhou junto do seguinte. A linha e' uma FOTO inteira do
/// personagem: a mais nova vence a velha do mesmo nome (ja' contem o que a
/// velha tinha, inclusive o item que saiu pro mercado). Personagem que so'
/// estava no velho vai com a foto velha — ela e' a que casa com os registros
/// dele. Registros: os velhos antes dos novos, todos (o banco ignora repetido
/// por id).
fn juntar_atrasado(velho: SaveBatch, novo: SaveBatch) -> SaveBatch {
    let rows = juntar_fotos(velho.rows, novo.rows, |r| r.name.as_str());
    let mut mercado = velho.mercado;
    mercado.extend(novo.mercado);
    SaveBatch { rows, mercado }
}

/// As fotos velhas cujo nome nao aparece nas novas, depois todas as novas.
fn juntar_fotos<T>(velhas: Vec<T>, novas: Vec<T>, nome: impl Fn(&T) -> &str) -> Vec<T> {
    let mut v: Vec<T> = velhas
        .into_iter()
        .filter(|a| !novas.iter().any(|b| nome(b) == nome(a)))
        .collect();
    v.extend(novas);
    v
}

#[cfg(test)]
mod testes_save_atrasado {
    use super::juntar_fotos;

    #[test]
    fn foto_nova_vence_e_quem_so_estava_no_velho_nao_se_perde() {
        let velhas = vec![("ana", 1), ("bia", 1)];
        let novas = vec![("bia", 2), ("caio", 2)];
        let j = juntar_fotos(velhas, novas, |r| r.0);
        assert_eq!(j, vec![("ana", 1), ("bia", 2), ("caio", 2)]);
    }

    #[test]
    fn sem_novas_fica_tudo_do_velho() {
        assert_eq!(
            juntar_fotos(vec![("ana", 1)], Vec::new(), |r| r.0),
            vec![("ana", 1)]
        );
    }
}

async fn write_batch(pool: &PgPool, batch: &SaveBatch) -> Result<()> {
    if batch.rows.is_empty() && batch.mercado.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for row in &batch.rows {
        let allocated_vec: Vec<i32> = row.allocated_points.iter().map(|&v| v as i32).collect();
        let (boat_kind, boat_x, boat_y, boat_dir): (
            Option<i16>,
            Option<f32>,
            Option<f32>,
            Option<i16>,
        ) = match row.boat {
            Some(b) => (
                Some(b.kind as i16),
                Some(b.pos.x),
                Some(b.pos.y),
                Some(b.dir as i16),
            ),
            None => (None, None, None, None),
        };
        let (boat_yaw, boat_sail_pos, boat_sail_angle, boat_anchor): (
            Option<f32>,
            Option<i16>,
            Option<f32>,
            Option<bool>,
        ) = match row.boat {
            Some(b) => (
                Some(b.yaw),
                Some(b.sail_position as i16),
                Some(b.sail_angle),
                Some(b.anchor_dropped),
            ),
            None => (None, None, None, None),
        };
        let (mounted_local_x, mounted_local_y): (Option<f32>, Option<f32>) = match row.mounted_local
        {
            Some(p) => (Some(p.x), Some(p.y)),
            None => (None, None),
        };
        // visual_json: persiste o VisualConfig em vigor (wardrobe mid-game).
        // None = mantem o que ja existe no banco (mas atualizamos sempre que
        // session.visual estiver setado, o que e o caso pra players logados).
        let visual_json: Option<String> = row
            .visual
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, fame, aura,
                                     unspent_points, allocated_points,
                                     skill_points_earned, skill_points_spent,
                                     boat_kind, boat_x, boat_y, boat_dir,
                                     gold, visual_json, updated,
                                     boat_yaw, boat_sail_pos, boat_sail_angle,
                                     boat_anchor_dropped, mounted_local_x, mounted_local_y,
                                     mp, stamina, zona, xp_bonus_ate,
                                     mortes_json, recuperacoes_dia, recuperacoes_usadas,
                                     fortuna_ate, sorte_ate, barra_json, preferencias_json, dungeon_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37)
             ON CONFLICT(name) DO UPDATE SET
               x = EXCLUDED.x,
               y = EXCLUDED.y,
               hp = EXCLUDED.hp,
               max_hp = EXCLUDED.max_hp,
               xp = EXCLUDED.xp,
               fame = EXCLUDED.fame,
               aura = EXCLUDED.aura,
               unspent_points = EXCLUDED.unspent_points,
               allocated_points = EXCLUDED.allocated_points,
               skill_points_earned = EXCLUDED.skill_points_earned,
               skill_points_spent = EXCLUDED.skill_points_spent,
               boat_kind = EXCLUDED.boat_kind,
               boat_x = EXCLUDED.boat_x,
               boat_y = EXCLUDED.boat_y,
               boat_dir = EXCLUDED.boat_dir,
               gold = EXCLUDED.gold,
               visual_json = COALESCE(EXCLUDED.visual_json, characters.visual_json),
               updated = EXCLUDED.updated,
               boat_yaw = EXCLUDED.boat_yaw,
               boat_sail_pos = EXCLUDED.boat_sail_pos,
               boat_sail_angle = EXCLUDED.boat_sail_angle,
               boat_anchor_dropped = EXCLUDED.boat_anchor_dropped,
               mounted_local_x = EXCLUDED.mounted_local_x,
               mounted_local_y = EXCLUDED.mounted_local_y,
               mp = EXCLUDED.mp,
               stamina = EXCLUDED.stamina,
               zona = COALESCE(EXCLUDED.zona, characters.zona),
               xp_bonus_ate = EXCLUDED.xp_bonus_ate,
               mortes_json = EXCLUDED.mortes_json,
               recuperacoes_dia = EXCLUDED.recuperacoes_dia,
               recuperacoes_usadas = EXCLUDED.recuperacoes_usadas,
               fortuna_ate = EXCLUDED.fortuna_ate,
               sorte_ate = EXCLUDED.sorte_ate,
               barra_json = EXCLUDED.barra_json,
               preferencias_json = EXCLUDED.preferencias_json,
               dungeon_json = EXCLUDED.dungeon_json",
        )
        .bind(&row.name)
        .bind(row.pos.x)
        .bind(row.pos.y)
        .bind(row.hp.current)
        .bind(row.hp.max)
        .bind(row.xp as i64)
        .bind(row.fame as i64)
        .bind(row.aura as i64)
        .bind(row.unspent_points as i32)
        .bind(&allocated_vec)
        .bind(row.skill_points_earned as i32)
        .bind(row.skill_points_spent as i32)
        .bind(boat_kind)
        .bind(boat_x)
        .bind(boat_y)
        .bind(boat_dir)
        .bind(row.gold as i64)
        .bind(&visual_json)
        .bind(now)
        .bind(boat_yaw)
        .bind(boat_sail_pos)
        .bind(boat_sail_angle)
        .bind(boat_anchor)
        .bind(mounted_local_x)
        .bind(mounted_local_y)
        .bind(row.mp)
        .bind(row.stamina)
        .bind(&row.zona)
        .bind(row.xp_bonus_ate)
        .bind(&row.mortes_json)
        .bind(row.recuperacoes_dia)
        .bind(row.recuperacoes_usadas)
        .bind(row.fortuna_ate)
        .bind(row.sorte_ate)
        .bind(&row.barra_json)
        .bind(&row.preferencias_json)
        .bind(&row.dungeon_json)
        .execute(&mut *tx)
        .await?;
        // A conta vai junto: bau aberto num personagem e a 1ª vitoria semanal
        // da conta nunca se separam.
        if let (Some(conta), false) = (row.account_id, row.conta_dungeon_json.is_empty()) {
            sqlx::query(
                "INSERT INTO dungeon_contas (account_id, dados_json, updated) VALUES ($1, $2, NOW())
                 ON CONFLICT (account_id) DO UPDATE SET dados_json = EXCLUDED.dados_json, updated = NOW()",
            )
            .bind(conta)
            .bind(&row.conta_dungeon_json)
            .execute(&mut *tx)
            .await?;
        }
        // Presenca: o premio entrou na bolsa acima; o resgate vira `aplicado`
        // na mesma transacao (docs/CALENDARIO.md). Queda antes daqui deixa a
        // linha pendente e o proximo login entrega de novo, uma vez.
        crate::presenca::marcar_aplicados(&mut tx, &row.presenca_aplicados).await?;
        crate::correio_admin::marcar(&mut tx, &row.name, &row.correio_recibos).await?;

        // Inventario: delete-all + insert-rows pra ser simples. O FK cascade
        // ja garante que deletar a linha do character limpa a inventory.
        sqlx::query("DELETE FROM inventory WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.inventory.iter().enumerate() {
            if slot.qty == 0 {
                continue;
            }
            let inst_json = slot.instance.and_then(|i| serde_json::to_string(&i).ok());
            sqlx::query(
                "INSERT INTO inventory (character_name, slot, item_id, qty, instance_data)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(slot.item_id as i32)
            .bind(slot.qty as i32)
            .bind(inst_json)
            .execute(&mut *tx)
            .await?;
        }

        // Equipment: mesmo padrao delete-all + insert.
        sqlx::query("DELETE FROM equipment WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for slot in shared::EquipSlot::TODOS {
            let (slot_name, item_opt, inst_opt) = (
                slot.as_db_str(),
                row.equipment.get(slot),
                row.equipment.get_inst(slot),
            );
            if let Some(iid) = item_opt {
                let inst_json = inst_opt.and_then(|i| serde_json::to_string(&i).ok());
                sqlx::query(
                    "INSERT INTO equipment (character_name, slot, item_id, instance_data) VALUES ($1, $2, $3, $4)",
                )
                .bind(&row.name)
                .bind(slot_name)
                .bind(iid as i32)
                .bind(inst_json)
                .execute(&mut *tx)
                .await?;
            }
        }

        // Cofre: mesmo padrao. Ele era CARREGADO e nunca gravado — o que se
        // guardava sumia no primeiro reinicio do servidor.
        sqlx::query("DELETE FROM vault WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.vault.iter().enumerate() {
            if slot.qty == 0 {
                continue;
            }
            let inst_json = slot.instance.and_then(|i| serde_json::to_string(&i).ok());
            sqlx::query(
                "INSERT INTO vault (character_name, slot, item_id, qty, instance_data)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(slot.item_id as i32)
            .bind(slot.qty as i32)
            .bind(inst_json)
            .execute(&mut *tx)
            .await?;
        }

        // Proficiencias: upsert por prof_kind.
        for (i, xp) in row.proficiencies.iter().enumerate() {
            if *xp == 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO proficiencies (character_name, prof_kind, xp)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(character_name, prof_kind) DO UPDATE SET xp = EXCLUDED.xp",
            )
            .bind(&row.name)
            .bind(i as i32)
            .bind(*xp as i64)
            .execute(&mut *tx)
            .await?;
        }

        // Player skills: delete-all + insert-rows. Cascade do FK em
        // Skill aprendida nao existe mais: a arma na mao da' as tres.
    }
    crate::mercado::gravar_registros(&mut tx, &batch.mercado).await?;
    tx.commit().await?;
    // Quests + pontos de facção (fora da tx; reconcilia character_quests).
    for row in &batch.rows {
        let _ = crate::quests::save_char(pool, &row.name, &row.quests, row.faction_points).await;
    }
    Ok(())
}
