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
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS characters (
            name     TEXT PRIMARY KEY,
            x        REAL NOT NULL,
            y        REAL NOT NULL,
            hp       INTEGER NOT NULL,
            max_hp   INTEGER NOT NULL,
            updated  BIGINT NOT NULL
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proficiencies (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            prof_kind      INTEGER NOT NULL,
            xp             BIGINT  NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, prof_kind)
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS inventory (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS equipment (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           TEXT NOT NULL,
            item_id        INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vault (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           INTEGER NOT NULL,
            item_id        INTEGER NOT NULL,
            qty            INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy_version (
            id          SMALLINT PRIMARY KEY DEFAULT 1,
            version     BIGINT NOT NULL DEFAULT 1,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CHECK (id = 1)
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    ).execute(pool).await?;
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
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    ).execute(pool).await?;
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
    ).execute(pool).await?;
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
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    ).execute(pool).await?;
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
    sqlx::query("SELECT pg_advisory_lock(728431)").execute(&mut *trava).await?;
    let r = init_schema_travado(&pool).await;
    let _ = sqlx::query("SELECT pg_advisory_unlock(728431)").execute(&mut *trava).await;
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
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS unspent_points INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS allocated_points INTEGER[] NOT NULL DEFAULT '{0,0,0,0,0,0}'")
        .execute(pool)
        .await?;

    // Boat state — quando player desconecta montado, salvamos o tipo do
    // barco + pos + direcao. Re-spawn no login. NULL = nao tava montado.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_kind SMALLINT NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_x REAL NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_y REAL NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_dir SMALLINT NULL")
        .execute(pool).await?;
    // Boat 2.5D: estado completo (yaw float, sail/anchor) + posicao do
    // player no deck local. Tudo NULL pra rows legacy (load fall-back).
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_yaw REAL NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_sail_pos SMALLINT NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_sail_angle REAL NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS boat_anchor_dropped BOOLEAN NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS mounted_local_x REAL NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS mounted_local_y REAL NULL")
        .execute(pool).await?;
    // Character creation: account_id liga char a conta (1:1, UNIQUE).
    // visual_json armazena VisualConfig serializado (skin/race/outfit/hair/color).
    // starting_weapon = item_id escolhido na criacao (informativo; weapon ja
    // ta em inventory+equipment do save).
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS account_id BIGINT NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS visual_json TEXT NULL")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS starting_weapon SMALLINT NULL")
        .execute(pool).await?;
    // Backfill account_id pra chars antigos (linka pelo username = char name).
    sqlx::query(
        "UPDATE characters c SET account_id = a.id
         FROM accounts a
         WHERE c.account_id IS NULL AND a.username = c.name"
    ).execute(pool).await?;
    // Indice nao-unico em account_id pra lookup rapido de chars por conta.
    // (Multi-char per account: removida constraint UNIQUE de versao anterior.)
    sqlx::query("DROP INDEX IF EXISTS idx_characters_account_unique")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_characters_account ON characters(account_id)")
        .execute(pool).await?;
    // Migration M6: design atual tem 6 stats (FOR/DES/INT/VIT/SPD/RES). Linhas
    // antigas com 5 elementos ganham um 0 no slot RES, preservando pontos ja
    // alocados. Idempotente — arrays de 6 nao sao tocados.
    sqlx::query(
        "UPDATE characters
         SET allocated_points = allocated_points || ARRAY[0]::INTEGER[]
         WHERE array_length(allocated_points, 1) = 5"
    ).execute(pool).await?;

    // Migration M7: escudo (item_id=7) sai do slot 'armor' e vai pra 'offhand'.
    // Idempotente: se ja moveu, UPDATE nao acha mais nada.
    sqlx::query(
        "UPDATE equipment SET slot = 'offhand'
         WHERE slot = 'armor' AND item_id = 7"
    ).execute(pool).await?;

    // Migration M8: deixa escudo comprável no shop. So aplica se o DB ja
    // tinha shield com buy_price=NULL (preserva tweaks manuais que o user
    // fez via SQL).
    sqlx::query(
        "UPDATE items SET buy_price = 50, shop_order = 9
         WHERE id = 7 AND buy_price IS NULL AND shop_order IS NULL"
    ).execute(pool).await?;

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
         ON CONFLICT DO NOTHING"
    ).execute(pool).await?;



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
         )"
    ).execute(pool).await?;

    // Migration M11: gold vira moeda (não-item). Coluna `characters.gold` +
    // backfill somando todo item_id=1 de inventory + vault, depois apaga as
    // rows. Idempotente: se rodar de novo, sum() vira 0 (nada pra somar).
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS gold BIGINT NOT NULL DEFAULT 0")
        .execute(pool).await?;
    sqlx::query(
        "UPDATE characters c SET gold = c.gold + COALESCE((
            SELECT SUM(qty)::BIGINT FROM inventory i
            WHERE i.character_name = c.name AND i.item_id = 1
        ), 0) + COALESCE((
            SELECT SUM(qty)::BIGINT FROM vault v
            WHERE v.character_name = c.name AND v.item_id = 1
        ), 0)"
    ).execute(pool).await?;
    sqlx::query("DELETE FROM inventory WHERE item_id = 1").execute(pool).await?;
    sqlx::query("DELETE FROM vault     WHERE item_id = 1").execute(pool).await?;

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
    sqlx::query(
        "ALTER TABLE inventory ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(pool).await?;

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
        "ALTER TABLE equipment ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(pool).await?;

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
    sqlx::query(
        "ALTER TABLE vault ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(pool).await?;

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
    ).execute(pool).await?;
    sqlx::query("INSERT INTO economy_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    ).execute(pool).await?;
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
    ] {
        sqlx::query(&format!("ALTER TABLE items {col}")).execute(pool).await?;
    }
    // Override de item_level no drop por enemy_kind (era hardcoded em
    // world.rs::spawn_loot_drops). NULL = usa items.item_level como fallback.
    // Phase 5 enemy refactor: build "playerizado" — colunas opcionais pra
    // weapon/offhand/armor + level. NULL = usa defaults hardcoded em
    // enemy_builds.rs. Schema soft (nao quebra se NULL); admin pode editar
    // via SQL ate ter UI dedicada.
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_level INTEGER")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_weapon SMALLINT")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_offhand SMALLINT")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_armor SMALLINT")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS build_alloc_points INTEGER[]")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds DROP COLUMN IF EXISTS build_learned_skills")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS loot_item_level INTEGER")
        .execute(pool).await?;

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
    ).execute(pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_loot_drops_kind ON loot_drops(enemy_kind)")
        .execute(pool).await?;

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
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_ts   ON item_drops_log(ts DESC)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_item ON item_drops_log(item_id)")
        .execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_kind ON item_drops_log(enemy_kind)")
        .execute(pool).await?;

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
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_farm_node_drops_kt \
                 ON farm_node_drops(kind, tier)")
        .execute(pool).await?;

    // Vendor shops — cada vendor tem um shop_id que aponta pra uma lista
    // curada de itens. Permite "espadeiro" que só vende espadas, "alquimista"
    // que só vende poções, etc., independente da categoria genérica.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    ).execute(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    ).execute(pool).await?;

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
            .execute(pool).await;
    }
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS faction TEXT NOT NULL DEFAULT 'peacemain'"
    ).execute(pool).await?;
    // Quando o personagem concluiu o tutorial pela ultima vez (NULL = nunca).
    // Usado pra UI ("ja fez tutorial") e pro fluxo de re-treino. Nao gateia
    // nada de forma rigida — tutorial e' repetivel.
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS last_tutorial_completed TIMESTAMPTZ NULL"
    ).execute(pool).await?;

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
        id: i32, name: &'static str, sell: i32, buy: Option<i32>, ord: Option<i32>, stack: i32,
        slot: Option<&'static str>, lvl: i32, ic: i32, ir: i32,
        hp: (i32,i32), mp: (i32,i32), atk: (i32,i32),
        def: (i32,i32), dex: (i32,i32), wis: (i32,i32),
    }
    let seed: &[S] = &[
        // Consumíveis / materiais — sem slot/range
        S{ id: item_id::GOLD as i32,           name:"Gold",            sell:0,   buy:None,         ord:None,    stack:9999, slot:None, lvl:1, ic:15, ir:9,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::HEALTH_POTION as i32,  name:"HP Potion",       sell:5,   buy:Some(10),     ord:Some(0), stack:20,   slot:None, lvl:1, ic:3,  ir:17,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MANA_POTION as i32,    name:"MP Potion",       sell:7,   buy:Some(15),     ord:Some(1), stack:20,   slot:None, lvl:1, ic:10, ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GREATER_HEAL as i32,   name:"HP Potion+",      sell:20,  buy:Some(40),     ord:Some(2), stack:20,   slot:None, lvl:1, ic:11, ir:17,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GREATER_MANA as i32,   name:"MP Potion+",      sell:25,  buy:Some(50),     ord:Some(3), stack:20,   slot:None, lvl:1, ic:9,  ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::STAMINA_POTION as i32, name:"Stamina Potion",  sell:10,  buy:Some(20),     ord:Some(4), stack:20,   slot:None, lvl:1, ic:8,  ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GEM as i32,            name:"Gem",             sell:50,  buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:15, ir:10,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::IRON_INGOT as i32,     name:"Iron Bar",        sell:30,  buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:10, ir:13,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::DRAGON_SCALE as i32,   name:"Dragon Scale",    sell:200, buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:12, ir:12,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        // Armas
        S{ id: item_id::SWORD as i32,          name:"Sword",           sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:5,  ic:1,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(8,16),  def:(0,0),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::DAGGER as i32,         name:"Dagger",          sell:40,  buy:Some(80),     ord:Some(5), stack:1,    slot:Some("Weapon"), lvl:5,  ic:0,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(5,12),  def:(0,0),  dex:(6,14), wis:(0,0) },
        S{ id: item_id::GREAT_SWORD as i32,    name:"Greatsword",      sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:10, ic:7,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(20,36), def:(0,0),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BOW as i32,            name:"Bow",             sell:75,  buy:Some(150),    ord:Some(7), stack:1,    slot:Some("Weapon"), lvl:8,  ic:13, ir:93,  hp:(0,0),    mp:(0,0),     atk:(10,20), def:(0,0),  dex:(8,16), wis:(0,0) },
        S{ id: item_id::STAFF as i32,          name:"Staff",           sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:6,  ic:3,  ir:93,  hp:(0,0),    mp:(20,60),   atk:(14,26), def:(0,0),  dex:(0,0),  wis:(3,8) },
        S{ id: item_id::WAND as i32,           name:"Wand",            sell:60,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:7,  ic:1,  ir:93,  hp:(0,0),    mp:(50,110),  atk:(4,10),  def:(0,0),  dex:(0,0),  wis:(5,12) },
        S{ id: item_id::SCIMITAR as i32,       name:"Scimitar",        sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:6,  ic:3,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(6,14),  def:(0,0),  dex:(4,10), wis:(0,0) },
        S{ id: item_id::AXE as i32,         name:"Axe",                sell:90,  buy:Some(180),    ord:Some(11),stack:1,    slot:Some("Weapon"), lvl:12, ic:5,  ir:90,  hp:(15,35),  mp:(0,0),     atk:(16,32), def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::SPEAR as i32,          name:"Spear",           sell:65,  buy:Some(130),    ord:Some(10),stack:1,    slot:Some("Weapon"), lvl:9,  ic:8,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(12,22), def:(0,0),  dex:(3,9),  wis:(0,0) },
        S{ id: item_id::CROSSBOW as i32,       name:"Crossbow",        sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:10, ic:15, ir:93,  hp:(0,0),    mp:(0,0),     atk:(14,24), def:(0,0),  dex:(6,12), wis:(0,0) },
        // Fase F — armas tier 2 (gate de char_lvl + prof_lvl). Item lvl 10 marca o tier.
        S{ id: item_id::ENHANCED_SWORD as i32, name:"Polished Blade",  sell:200, buy:Some(800),    ord:Some(20),stack:1,    slot:Some("Weapon"), lvl:10, ic:2,  ir:90,  hp:(8,15),   mp:(0,0),     atk:(18,22), def:(1,2),  dex:(2,4),  wis:(0,0) },
        // Fase F — armas tier 3 (char_lvl 20, sword prof 10). Item lvl 20.
        S{ id: item_id::VETERAN_SWORD as i32,  name:"Veteran's Blade", sell:600, buy:Some(2400), ord:Some(21),stack:1,    slot:Some("Weapon"), lvl:20, ic:4,  ir:90,  hp:(15,30),  mp:(0,0),     atk:(28,34), def:(2,4),  dex:(3,6),  wis:(0,0) },
        // Fase F — armas tier 2 das outras 5 profs (char_lvl 10, prof respectiva 5).
        S{ id: item_id::ENHANCED_BOW as i32,   name:"Reinforced Bow",  sell:200, buy:Some(800),    ord:Some(22),stack:1,    slot:Some("Weapon"), lvl:10, ic:14, ir:93,  hp:(0,0),    mp:(0,0),     atk:(22,30), def:(0,0),  dex:(18,30),wis:(0,0) },
        S{ id: item_id::ENHANCED_STAFF as i32, name:"Enchanted Staff", sell:200, buy:Some(800),    ord:Some(23),stack:1,    slot:Some("Weapon"), lvl:10, ic:4,  ir:93,  hp:(0,0),    mp:(50,110),  atk:(28,40), def:(0,0),  dex:(0,0),  wis:(6,14) },
        S{ id: item_id::ENHANCED_WAND as i32,  name:"Enchanted Wand",  sell:200, buy:Some(800),    ord:Some(24),stack:1,    slot:Some("Weapon"), lvl:10, ic:2,  ir:93,  hp:(0,0),    mp:(100,180), atk:(8,16),  def:(0,0),  dex:(0,0),  wis:(10,20) },
        S{ id: item_id::ENHANCED_AXE as i32,   name:"Forged Axe",      sell:200, buy:Some(800),    ord:Some(25),stack:1,    slot:Some("Weapon"), lvl:10, ic:6,  ir:90,  hp:(25,50),  mp:(0,0),     atk:(32,46), def:(4,9),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::ENHANCED_SPEAR as i32, name:"Reinforced Spear",sell:200, buy:Some(800),    ord:Some(26),stack:1,    slot:Some("Weapon"), lvl:10, ic:9,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(24,34), def:(0,0),  dex:(8,16), wis:(0,0) },
        // Armaduras / escudos
        S{ id: item_id::ARMOR as i32,          name:"Armor",           sell:25,  buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:5,  ic:0,  ir:120, hp:(25,60),  mp:(0,0),     atk:(0,0),   def:(3,8),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::SHIELD as i32,         name:"Shield",          sell:25,  buy:Some(50),     ord:Some(9), stack:1,    slot:Some("Offhand"),lvl:5,  ic:6,  ir:132, hp:(50,100), mp:(0,0),     atk:(0,0),   def:(5,12), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::LEATHER_ARMOR as i32,  name:"Leather Armor",   sell:60,  buy:Some(120),    ord:Some(6), stack:1,    slot:Some("Armor"),  lvl:7,  ic:2,  ir:120, hp:(15,35),  mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(3,9),  wis:(0,0) },
        S{ id: item_id::PLATE_ARMOR as i32,    name:"Plate Armor",     sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:12, ic:12, ir:120, hp:(80,160), mp:(0,0),     atk:(0,0),   def:(8,16), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::ROBE as i32,           name:"Tunic",           sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:7,  ic:0,  ir:123, hp:(5,15),   mp:(30,90),   atk:(0,0),   def:(1,4),  dex:(0,0),  wis:(4,12) },
        S{ id: item_id::HEAVY_SHIELD as i32,   name:"Heavy Shield",    sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Offhand"),lvl:12, ic:8,  ir:132, hp:(70,140), mp:(0,0),     atk:(0,0),   def:(10,20),dex:(0,0),  wis:(0,0) },
        // Acessórios
        S{ id: item_id::RING as i32,           name:"Ring",            sell:20,  buy:None,         ord:None,    stack:1,    slot:Some("Ring"),    lvl:5,  ic:12, ir:128, hp:(0,0),    mp:(0,0),     atk:(0,0),   def:(0,0),  dex:(2,8),  wis:(1,5) },
        S{ id: item_id::AMULET as i32,         name:"Amulet",          sell:90,  buy:Some(180),    ord:Some(8), stack:1,    slot:Some("Necklace"),lvl:7,  ic:0,  ir:129, hp:(8,25),   mp:(15,45),   atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(4,12) },
        S{ id: item_id::LUCKY_RING as i32,     name:"Lucky Ring",      sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Ring"),    lvl:8,  ic:13, ir:128, hp:(5,18),   mp:(10,30),   atk:(1,4),   def:(0,0),  dex:(3,9),  wis:(1,4) },
        S{ id: item_id::PENDANT as i32,        name:"Pendant",         sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:9,  ic:2,  ir:129, hp:(15,35),  mp:(20,50),   atk:(0,0),   def:(0,2),  dex:(0,0),  wis:(2,6) },
        S{ id: item_id::CHARM as i32,          name:"Mystic Amulet",   sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:10, ic:4,  ir:129, hp:(0,0),    mp:(5,20),    atk:(2,6),   def:(0,0),  dex:(2,6),  wis:(2,6) },
        // Phase E — slots novos
        S{ id: item_id::HELM_LEATHER as i32,   name:"Leather Cap",     sell:40,  buy:None,         ord:None,    stack:1,    slot:Some("Helm"),   lvl:6,  ic:0,  ir:116, hp:(8,22),   mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(2,6),  wis:(0,0) },
        S{ id: item_id::HELM_PLATE as i32,     name:"Plate Helm",      sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Helm"),   lvl:11, ic:4,  ir:116, hp:(30,60),  mp:(0,0),     atk:(0,0),   def:(4,10), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::LEGS_LEATHER as i32,   name:"Leather Pants",   sell:45,  buy:None,         ord:None,    stack:1,    slot:Some("Legs"),   lvl:6,  ic:0,  ir:124, hp:(12,28),  mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(3,7),  wis:(0,0) },
        S{ id: item_id::LEGS_PLATE as i32,     name:"Plate Pants",     sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Legs"),   lvl:11, ic:4,  ir:124, hp:(40,80),  mp:(0,0),     atk:(0,0),   def:(6,12), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BOOTS_LEATHER as i32,  name:"Leather Boots",   sell:35,  buy:None,         ord:None,    stack:1,    slot:Some("Boots"),  lvl:5,  ic:0,  ir:126, hp:(5,15),   mp:(0,0),     atk:(0,0),   def:(0,3),  dex:(4,8),  wis:(0,0) },
        S{ id: item_id::BOOTS_PLATE as i32,    name:"Plate Boots",     sell:70,  buy:None,         ord:None,    stack:1,    slot:Some("Boots"),  lvl:10, ic:4,  ir:126, hp:(20,40),  mp:(0,0),     atk:(0,0),   def:(3,7),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::GLOVES_LEATHER as i32, name:"Leather Gloves",  sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Gloves"), lvl:5,  ic:0,  ir:122, hp:(0,0),    mp:(0,0),     atk:(1,5),   def:(0,2),  dex:(3,7),  wis:(0,0) },
        S{ id: item_id::GLOVES_PLATE as i32,   name:"Gauntlets",       sell:65,  buy:None,         ord:None,    stack:1,    slot:Some("Gloves"), lvl:10, ic:4,  ir:122, hp:(12,28),  mp:(0,0),     atk:(3,8),   def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BELT_BASIC as i32,     name:"Belt",            sell:35,  buy:None,         ord:None,    stack:1,    slot:Some("Belt"),   lvl:5,  ic:0,  ir:130, hp:(15,35),  mp:(0,0),     atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BELT_MAGIC as i32,     name:"Magic Sash",      sell:75,  buy:None,         ord:None,    stack:1,    slot:Some("Belt"),   lvl:9,  ic:4,  ir:130, hp:(0,0),    mp:(20,50),   atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(2,6) },
        S{ id: item_id::CAPE_BASIC as i32,     name:"Cape",            sell:40,  buy:None,         ord:None,    stack:1,    slot:Some("Cape"),   lvl:6,  ic:0,  ir:134, hp:(12,28),  mp:(0,0),     atk:(0,0),   def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::CAPE_MAGIC as i32,     name:"Magic Cloak",     sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Cape"),   lvl:10, ic:4,  ir:134, hp:(0,0),    mp:(25,60),   atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(3,9) },
        S{ id: item_id::NECKLACE_BASIC as i32, name:"Necklace",        sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:7,  ic:6,  ir:129, hp:(12,28),  mp:(5,15),    atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(2,5) },
        S{ id: item_id::NECKLACE_MAGIC as i32, name:"Magic Necklace",  sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:11, ic:8,  ir:129, hp:(0,0),    mp:(25,55),   atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(4,10) },
        // Embarcacao — usavel na margem (consumida ao usar; volta no dismount).
        // Sem equip slot. Stack 1 (item unico). Icone re-aproveitado de barril
        // ate ter art proprio.
        S{ id: item_id::BOAT_LYLIAN_LEUTARD as i32, name:"Lylian Leutard", sell:0, buy:Some(500), ord:Some(50), stack:1, slot:None, lvl:1, ic:0, ir:138, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },

        // === Materiais de coleta e craft. Nunca estiveram na tabela: a coleta
        // entregava um item sem nome e sem `stack_max`, que caia em stack de 1 e
        // enchia a bolsa com dezenas de linhas de uma madeira cada. Achado com os
        // bots coletando na ilha.
        S{ id: item_id::WOOD_T1 as i32,        name:"Madeira T1",   sell:2,    buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::WOOD_T2 as i32,        name:"Madeira T2",   sell:6,    buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::WOOD_T3 as i32,        name:"Madeira T3",   sell:18,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::WOOD_T4 as i32,        name:"Madeira T4",   sell:54,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::LEATHER_T1 as i32,     name:"Couro T1",     sell:3,    buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::LEATHER_T2 as i32,     name:"Couro T2",     sell:9,    buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::LEATHER_T3 as i32,     name:"Couro T3",     sell:27,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::LEATHER_T4 as i32,     name:"Couro T4",     sell:81,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MINERAL_T1 as i32,     name:"Mineral T1",   sell:4,    buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MINERAL_T2 as i32,     name:"Mineral T2",   sell:12,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MINERAL_T3 as i32,     name:"Mineral T3",   sell:36,   buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MINERAL_T4 as i32,     name:"Mineral T4",   sell:108,  buy:None, ord:None, stack:999, slot:None, lvl:1, ic:-1, ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },

        // Peixes (drop da pesca) — stackáveis, sem slot. icon_path setado
        // explicitamente abaixo pros sprites de Fish/ (ic/ir são sentinela -1
        // pra NÃO virar Items/r###_c## no backfill de icon_path).
        S{ id: item_id::FISH_ANCHOVY as i32,      name:"Anchova",                sell:8,   buy:None,       ord:None,     stack:99,slot:None,         lvl:1,  ic:-1,ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::FISH_CLOWNFISH as i32,    name:"Peixe-palhaço",          sell:18,  buy:None,       ord:None,     stack:99,slot:None,         lvl:1,  ic:-1,ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::FISH_SURGEONFISH as i32,  name:"Peixe-cirurgião",        sell:35,  buy:None,       ord:None,     stack:99,slot:None,         lvl:1,  ic:-1,ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::FISH_PUFFERFISH as i32,   name:"Baiacu",                 sell:60,  buy:None,       ord:None,     stack:99,slot:None,         lvl:1,  ic:-1,ir:-1, hp:(0,0), mp:(0,0), atk:(0,0), def:(0,0), dex:(0,0), wis:(0,0) },
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
             WHERE items.icon_col = 0 AND items.icon_row = 0"
        )
        .bind(s.id).bind(s.name).bind(s.sell).bind(s.buy).bind(s.ord).bind(s.stack)
        .bind(s.slot).bind(s.lvl).bind(s.ic).bind(s.ir)
        .bind(s.hp.0).bind(s.hp.1).bind(s.mp.0).bind(s.mp.1)
        .bind(s.atk.0).bind(s.atk.1).bind(s.def.0).bind(s.def.1)
        .bind(s.dex.0).bind(s.dex.1).bind(s.wis.0).bind(s.wis.1)
        .execute(pool).await?;

        // Force-update name pra refletir traduções PT->EN. ON CONFLICT do
        // INSERT acima não atualiza name (preserva edits do admin), então
        // garantimos aqui que o seed sobrescreve o nome em DBs existentes.
        sqlx::query("UPDATE items SET name = $1 WHERE id = $2")
            .bind(s.name).bind(s.id).execute(pool).await?;
    }
    // Backfill icon_path apontando pros PNGs extraídos em
    // MMORPG/Assets/_Project/Resources/Items/r{row}_c{col}.png — naming
    // gerado direto a partir do (icon_col, icon_row) já populados.
    // Só seta se ainda for NULL (admin pode editar sem ser sobrescrito).
    sqlx::query(
        "UPDATE items \
         SET icon_path = 'Items/r' || lpad(icon_row::text, 3, '0') || \
                          '_c' || lpad(icon_col::text, 2, '0') \
         WHERE icon_path IS NULL AND icon_col >= 0 AND icon_row >= 0"
    ).execute(pool).await?;

    // Peixes: icon vem dos sprites Fish/<Nome> (RemoteContent), não do
    // spritesheet de Items. Só seta se NULL (admin pode sobrescrever).
    for (id, addr) in [
        (item_id::FISH_ANCHOVY,     "Fish/Anchovy"),
        (item_id::FISH_CLOWNFISH,   "Fish/Clownfish"),
        (item_id::FISH_SURGEONFISH, "Fish/Surgeonfish"),
        (item_id::FISH_PUFFERFISH,  "Fish/Pufferfish"),
    ] {
        sqlx::query("UPDATE items SET icon_path = $1 WHERE id = $2 AND icon_path IS NULL")
            .bind(addr).bind(id as i32).execute(pool).await?;
    }

    // Enemy kinds — espelha o array hardcoded antigo. Tuple muito grande;
    // usa struct local pra clareza.
    struct E { kind:i32,name:&'static str,hp:i32,sp:f32,dmg:i32,cd:f32,det:f32,rng:f32,kite:Option<f32>,proj:i32,xp:i64,def:i32,sz:f32,t:[f32;4] }
    let kinds: &[E] = &[
        E{ kind:0, name:"Grunt",     hp:50,  sp:2.0, dmg:10, cd:2.0, det:9.0,  rng:1.8,  kite:None,        proj:1, xp:30,  def:0,  sz:1.0,  t:[1.0,1.0,1.0,1.0] },
        E{ kind:1, name:"Tank",      hp:120, sp:1.3, dmg:18, cd:2.8, det:8.0,  rng:1.8,  kite:None,        proj:1, xp:75,  def:8,  sz:1.3,  t:[1.0,0.35,0.25,1.0] },
        E{ kind:2, name:"Ranger",    hp:35,  sp:2.4, dmg:12, cd:1.5, det:13.0, rng:9.0,  kite:Some(5.0),   proj:1, xp:50,  def:0,  sz:0.85, t:[0.3,0.9,1.0,1.0] },
        E{ kind:3, name:"Ninja",     hp:40,  sp:4.2, dmg:15, cd:1.0, det:11.0, rng:1.8,  kite:None,        proj:1, xp:55,  def:2,  sz:0.75, t:[0.75,0.2,1.0,1.0] },
        E{ kind:4, name:"Mago",      hp:45,  sp:1.4, dmg:22, cd:2.2, det:15.0, rng:12.0, kite:Some(8.0),   proj:1, xp:70,  def:1,  sz:1.0,  t:[0.4,0.4,1.0,1.0] },
        E{ kind:5, name:"Berserker", hp:200, sp:1.5, dmg:28, cd:3.0, det:8.0,  rng:1.8,  kite:None,        proj:1, xp:110, def:4,  sz:1.5,  t:[1.0,0.5,0.1,1.0] },
        E{ kind:6, name:"Arqueiro",  hp:45,  sp:2.8, dmg:14, cd:1.6, det:13.0, rng:9.0,  kite:Some(7.0),   proj:1, xp:60,  def:1,  sz:1.0,  t:[0.2,0.9,0.3,1.0] },
        E{ kind:7, name:"Boss",      hp:700, sp:1.6, dmg:40, cd:2.8, det:18.0, rng:13.0, kite:Some(10.0),  proj:5, xp:600, def:20, sz:2.2,  t:[1.0,0.85,0.15,1.0] },
    ];
    for e in kinds {
        sqlx::query(
            "INSERT INTO enemy_kinds (kind, name, hp_max, speed, attack_damage, attack_cooldown, \
             detect_range, attack_range, kite_dist, proj_count, xp_reward, defense, size_scale, \
             tint_r, tint_g, tint_b, tint_a) VALUES \
             ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT (kind) DO NOTHING"
        )
        .bind(e.kind).bind(e.name).bind(e.hp).bind(e.sp).bind(e.dmg).bind(e.cd)
        .bind(e.det).bind(e.rng).bind(e.kite).bind(e.proj).bind(e.xp).bind(e.def).bind(e.sz)
        .bind(e.t[0]).bind(e.t[1]).bind(e.t[2]).bind(e.t[3])
        .execute(pool).await?;
    }
    // Backfill do loot_item_level — antes hardcoded em world.rs.
    // Só seta se NULL (admin pode editar via web admin sem ser sobrescrito).
    let item_levels: &[(i32, i32)] = &[
        (0, 10),  // Grunt
        (1, 15),  // Tank
        (2, 10),  // Ranger
        (3, 25),  // Ninja
        (4, 20),  // Mago
        (5, 30),  // Berserker
        (6, 10),  // Arqueiro
        (7, 50),  // Boss
    ];
    for (kind, lvl) in item_levels {
        sqlx::query("UPDATE enemy_kinds SET loot_item_level = $2 WHERE kind = $1 AND loot_item_level IS NULL")
            .bind(kind).bind(lvl).execute(pool).await?;
    }

    // Loot — só seeda se totalmente vazio (rebalanceios manuais não são
    // sobrescritos).
    let loot_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM loot_drops")
        .fetch_one(pool).await?;
    if loot_count == 0 {
        // (enemy_kind, item_id, qty_min, qty_max, chance)
        let drops: &[(i32, u16, i32, i32, f32)] = &[
            // Boss (7) — alto valor + raridades
            (7, item_id::GOLD,           200, 500, 1.0),
            (7, item_id::DRAGON_SCALE,   1,   4,   1.0),
            (7, item_id::GREATER_HEAL,   2,   5,   1.0),
            (7, item_id::GREATER_MANA,   2,   2,   1.0),
            (7, item_id::GREAT_SWORD,    1,   1,   0.55),
            (7, item_id::WAND,           1,   1,   0.55),
            (7, item_id::PLATE_ARMOR,    1,   1,   0.50),
            (7, item_id::ROBE,           1,   1,   0.55),
            (7, item_id::LUCKY_RING,     1,   1,   0.45),
            // Berserker (5) — armadura
            (5, item_id::GOLD,           25,  60,  1.0),
            (5, item_id::PLATE_ARMOR,    1,   1,   0.22),
            (5, item_id::ARMOR,          1,   1,   0.23),
            (5, item_id::GREATER_HEAL,   1,   1,   0.30),
            (5, item_id::IRON_INGOT,     1,   3,   0.20),
            // Mago (4) — staff/wand + mana
            (4, item_id::GOLD,           15,  35,  1.0),
            (4, item_id::WAND,           1,   1,   0.22),
            (4, item_id::STAFF,          1,   1,   0.23),
            (4, item_id::MANA_POTION,    1,   3,   0.35),
            (4, item_id::ROBE,           1,   1,   0.18),
            (4, item_id::GEM,            1,   1,   0.12),
            // Tank (1)
            (1, item_id::GOLD,           15,  40,  1.0),
            (1, item_id::SHIELD,         1,   1,   0.18),
            (1, item_id::ARMOR,          1,   1,   0.25),
            (1, item_id::SWORD,          1,   1,   0.10),
            (1, item_id::HEALTH_POTION,  1,   3,   0.30),
            (1, item_id::IRON_INGOT,     1,   1,   0.15),
            // Ranger/Arqueiro (2 e 6)
            (2, item_id::GOLD,           10,  28,  1.0),
            (2, item_id::BOW,            1,   1,   0.20),
            (2, item_id::RING,           1,   1,   0.13),
            (2, item_id::AMULET,         1,   1,   0.12),
            (2, item_id::STAMINA_POTION, 1,   1,   0.35),
            (6, item_id::GOLD,           10,  28,  1.0),
            (6, item_id::BOW,            1,   1,   0.20),
            (6, item_id::RING,           1,   1,   0.13),
            (6, item_id::AMULET,         1,   1,   0.12),
            (6, item_id::STAMINA_POTION, 1,   1,   0.35),
            // Ninja (3) — leve + dagger
            (3, item_id::GOLD,           8,   22,  1.0),
            (3, item_id::DAGGER,         1,   1,   0.25),
            (3, item_id::LEATHER_ARMOR,  1,   1,   0.22),
            (3, item_id::MANA_POTION,    1,   1,   0.35),
            (3, item_id::LUCKY_RING,     1,   1,   0.10),
            // Grunt (0)
            (0, item_id::GOLD,           4,   14,  1.0),
            (0, item_id::HEALTH_POTION,  1,   1,   0.25),
            (0, item_id::MANA_POTION,    1,   1,   0.15),
            (0, item_id::IRON_INGOT,     1,   1,   0.08),
        ];
        for (kind, item, qmin, qmax, chance) in drops {
            sqlx::query(
                "INSERT INTO loot_drops (enemy_kind, item_id, qty_min, qty_max, chance) \
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(kind).bind(*item as i32).bind(qmin).bind(qmax).bind(chance)
            .execute(pool).await?;
        }
        tracing::info!("economy seed: {} loot drops inseridos", drops.len());
    }

    // Seed aditivo — itens novos (ids 24-30) só são inseridos se ainda não
    // existirem na tabela. Permite expandir o pool de drops sem resetar DB.
    let new_drops: &[(i32, u16, i32, i32, f32)] = &[
        // Scimitar (24) — light melee, ninja/ranger
        (3, item_id::SCIMITAR,      1, 1, 0.18),
        (2, item_id::SCIMITAR,      1, 1, 0.10),
        (6, item_id::SCIMITAR,      1, 1, 0.10),
        // Hammer (25) — heavy weapon
        (5, item_id::AXE,        1, 1, 0.18),
        (1, item_id::AXE,        1, 1, 0.12),
        (7, item_id::AXE,        1, 1, 0.40),
        // Spear (26) — pole
        (1, item_id::SPEAR,         1, 1, 0.16),
        (5, item_id::SPEAR,         1, 1, 0.14),
        // Crossbow (27) — ranged
        (2, item_id::CROSSBOW,      1, 1, 0.15),
        (6, item_id::CROSSBOW,      1, 1, 0.18),
        (7, item_id::CROSSBOW,      1, 1, 0.45),
        // Heavy Shield (28) — defense
        (1, item_id::HEAVY_SHIELD,  1, 1, 0.16),
        (5, item_id::HEAVY_SHIELD,  1, 1, 0.10),
        (7, item_id::HEAVY_SHIELD,  1, 1, 0.40),
        // Pendant (29) — joia
        (4, item_id::PENDANT,       1, 1, 0.10),
        (7, item_id::PENDANT,       1, 1, 0.35),
        (2, item_id::PENDANT,       1, 1, 0.08),
        (6, item_id::PENDANT,       1, 1, 0.08),
        // Charm (30) — joia
        (3, item_id::CHARM,         1, 1, 0.12),
        (4, item_id::CHARM,         1, 1, 0.10),
        (2, item_id::CHARM,         1, 1, 0.08),
        (6, item_id::CHARM,         1, 1, 0.08),
        // === Fase E — slots novos ===
        // Helm leather/plate (31, 32)
        (3, item_id::HELM_LEATHER,  1, 1, 0.14),
        (2, item_id::HELM_LEATHER,  1, 1, 0.12),
        (6, item_id::HELM_LEATHER,  1, 1, 0.12),
        (1, item_id::HELM_PLATE,    1, 1, 0.16),
        (5, item_id::HELM_PLATE,    1, 1, 0.14),
        (7, item_id::HELM_PLATE,    1, 1, 0.40),
        // Boots leather/plate (35, 36)
        (3, item_id::BOOTS_LEATHER, 1, 1, 0.14),
        (2, item_id::BOOTS_LEATHER, 1, 1, 0.12),
        (6, item_id::BOOTS_LEATHER, 1, 1, 0.12),
        (1, item_id::BOOTS_PLATE,   1, 1, 0.14),
        (5, item_id::BOOTS_PLATE,   1, 1, 0.12),
        (7, item_id::BOOTS_PLATE,   1, 1, 0.35),
        // Gloves leather/plate (37, 38)
        (3, item_id::GLOVES_LEATHER,1, 1, 0.14),
        (2, item_id::GLOVES_LEATHER,1, 1, 0.12),
        (6, item_id::GLOVES_LEATHER,1, 1, 0.12),
        (1, item_id::GLOVES_PLATE,  1, 1, 0.14),
        (5, item_id::GLOVES_PLATE,  1, 1, 0.12),
        (7, item_id::GLOVES_PLATE,  1, 1, 0.35),
        // Cape basic/magic (41, 42)
        (1, item_id::CAPE_BASIC,    1, 1, 0.10),
        (5, item_id::CAPE_BASIC,    1, 1, 0.10),
        (3, item_id::CAPE_BASIC,    1, 1, 0.10),
        (4, item_id::CAPE_MAGIC,    1, 1, 0.12),
        (6, item_id::CAPE_MAGIC,    1, 1, 0.10),
        (7, item_id::CAPE_MAGIC,    1, 1, 0.30),
        // Necklace basic/magic (43, 44)
        (2, item_id::NECKLACE_BASIC,1, 1, 0.10),
        (6, item_id::NECKLACE_BASIC,1, 1, 0.10),
        (1, item_id::NECKLACE_BASIC,1, 1, 0.10),
        (4, item_id::NECKLACE_MAGIC,1, 1, 0.12),
        (3, item_id::NECKLACE_MAGIC,1, 1, 0.10),
        (7, item_id::NECKLACE_MAGIC,1, 1, 0.35),
        // === Fase F — armas tier 2 ===
        // Lâmina Polida (45) — drop em inimigos lvl 15-25 (sistema novo: kind=level).
        // Chance baixa pra ser raro. Tank/Berserker (kinds 1, 5) também dropam
        // pq são melee de tier médio no sistema legacy.
        (15, item_id::ENHANCED_SWORD, 1, 1, 0.04),
        (18, item_id::ENHANCED_SWORD, 1, 1, 0.05),
        (20, item_id::ENHANCED_SWORD, 1, 1, 0.06),
        (22, item_id::ENHANCED_SWORD, 1, 1, 0.06),
        (25, item_id::ENHANCED_SWORD, 1, 1, 0.08),
        (1,  item_id::ENHANCED_SWORD, 1, 1, 0.04),
        (5,  item_id::ENHANCED_SWORD, 1, 1, 0.04),
        // === Fase F — armas tier 3 ===
        // Lâmina do Veterano (46) — drop em inimigos lvl 20-30. Mais raro que tier 2.
        // Boss (kind 7 legacy) também dropa pq é o end-game current.
        (20, item_id::VETERAN_SWORD,  1, 1, 0.02),
        (23, item_id::VETERAN_SWORD,  1, 1, 0.03),
        (25, item_id::VETERAN_SWORD,  1, 1, 0.04),
        (27, item_id::VETERAN_SWORD,  1, 1, 0.05),
        (30, item_id::VETERAN_SWORD,  1, 1, 0.06),
        (7,  item_id::VETERAN_SWORD,  1, 1, 0.15),
        // === Fase F — armas tier 2 outras profs (47..51) ===
        // Distribuição: cada arma dropa em mobs lvl 15-25 com chance 0.04-0.06,
        // + um kind legacy temático (Ranger pra Bow, Mago pra Staff/Wand,
        // Berserker pra Axe, Tank pra Spear).
        // Arco Reforçado (47) — Ranger/Arqueiro
        (15, item_id::ENHANCED_BOW,    1, 1, 0.04),
        (18, item_id::ENHANCED_BOW,    1, 1, 0.05),
        (22, item_id::ENHANCED_BOW,    1, 1, 0.06),
        (2,  item_id::ENHANCED_BOW,    1, 1, 0.04),
        (6,  item_id::ENHANCED_BOW,    1, 1, 0.04),
        // Cajado Encantado (48) — Mago
        (15, item_id::ENHANCED_STAFF,  1, 1, 0.04),
        (18, item_id::ENHANCED_STAFF,  1, 1, 0.05),
        (22, item_id::ENHANCED_STAFF,  1, 1, 0.06),
        (4,  item_id::ENHANCED_STAFF,  1, 1, 0.06),
        // Varinha Encantada (49) — Mago
        (15, item_id::ENHANCED_WAND,   1, 1, 0.04),
        (18, item_id::ENHANCED_WAND,   1, 1, 0.05),
        (22, item_id::ENHANCED_WAND,   1, 1, 0.06),
        (4,  item_id::ENHANCED_WAND,   1, 1, 0.06),
        // Machado Forjado (50) — Berserker/Tank
        (15, item_id::ENHANCED_AXE,    1, 1, 0.04),
        (18, item_id::ENHANCED_AXE,    1, 1, 0.05),
        (22, item_id::ENHANCED_AXE,    1, 1, 0.06),
        (5,  item_id::ENHANCED_AXE,    1, 1, 0.05),
        (1,  item_id::ENHANCED_AXE,    1, 1, 0.04),
        // Lança Reforçada (51) — Tank/Berserker
        (15, item_id::ENHANCED_SPEAR,  1, 1, 0.04),
        (18, item_id::ENHANCED_SPEAR,  1, 1, 0.05),
        (22, item_id::ENHANCED_SPEAR,  1, 1, 0.06),
        (1,  item_id::ENHANCED_SPEAR,  1, 1, 0.05),
        (5,  item_id::ENHANCED_SPEAR,  1, 1, 0.04),
    ];
    let mut inserted = 0usize;
    for (kind, item, qmin, qmax, chance) in new_drops {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loot_drops WHERE enemy_kind = $1 AND item_id = $2"
        )
        .bind(kind).bind(*item as i32)
        .fetch_one(pool).await?;
        if exists == 0 {
            // Guard de FK: parte dos `new_drops` referencia kinds do sistema
            // level-based (15, 18, 20, ...) que nao estao no seed base de
            // `enemy_kinds`. Em DB novo o INSERT direto viola a FK e derruba o
            // boot; aqui a linha simplesmente e pulada.
            let n = sqlx::query(
                "INSERT INTO loot_drops (enemy_kind, item_id, qty_min, qty_max, chance) \
                 SELECT $1, $2, $3, $4, $5 \
                 WHERE EXISTS (SELECT 1 FROM enemy_kinds WHERE kind = $1)"
            )
            .bind(kind).bind(*item as i32).bind(qmin).bind(qmax).bind(chance)
            .execute(pool).await?;
            inserted += n.rows_affected() as usize;
        }
    }
    if inserted > 0 {
        tracing::info!("economy seed: {} loot drops aditivos (itens novos)", inserted);
    }

    // Seed dos shops dos vendors. shop_id=1 fica como generalista (legacy
    // do shop antigo). Demais são especializados.
    let shop_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendor_shops")
        .fetch_one(pool).await?;
    if shop_count == 0 {
        let shops: &[(i32, &str, &[u16])] = &[
            (1, "Merchant", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION, item_id::DAGGER,
                item_id::LEATHER_ARMOR, item_id::BOW, item_id::AMULET, item_id::SHIELD,
            ]),
            (2, "Swordsmith", &[
                item_id::SWORD, item_id::DAGGER, item_id::GREAT_SWORD,
            ]),
            (3, "Alchemist", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION,
            ]),
            (4, "Blacksmith", &[
                item_id::ARMOR, item_id::SHIELD, item_id::LEATHER_ARMOR,
                item_id::PLATE_ARMOR,
            ]),
            (5, "Mage", &[
                item_id::WAND, item_id::STAFF, item_id::ROBE, item_id::AMULET,
                item_id::LUCKY_RING, item_id::RING,
            ]),
            (6, "Archer", &[
                item_id::BOW, item_id::STAMINA_POTION,
            ]),
        ];
        for (sid, name, items) in shops {
            sqlx::query(
                "INSERT INTO vendor_shops (shop_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"
            ).bind(sid).bind(*name).execute(pool).await?;
            for (i, &item) in items.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) \
                     VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
                )
                .bind(sid).bind(item as i32).bind(i as i32)
                .execute(pool).await?;
            }
        }
        tracing::info!("economy seed: {} vendor shops inseridos", shops.len());
    }

    // Adiciona itens novos aos vendors existentes (idempotente via ON CONFLICT).
    let new_shop_items: &[(i32, u16)] = &[
        (2, item_id::SCIMITAR), (2, item_id::SPEAR), (2, item_id::AXE),
        (4, item_id::HEAVY_SHIELD),
        (5, item_id::PENDANT), (5, item_id::CHARM),
        (6, item_id::CROSSBOW),
        // Mercador (shop 1) tambem vende Axe e Spear — eh o unico vendor
        // NPC que sempre tem no mundo, garante que o player consiga comprar.
        (1, item_id::AXE), (1, item_id::SPEAR),
        // Fase F — Lâmina Polida no Espadeiro (shop 2). Buy_price 800g vem do
        // seed do item; gate de char_lvl + prof_lvl bloqueia equip mesmo após compra.
        (2, item_id::ENHANCED_SWORD),
        // Fase F — Lâmina do Veterano no Espadeiro (2400g, char_lvl 20, prof 10).
        (2, item_id::VETERAN_SWORD),
        // Fase F — armas tier 2 das outras profs (800g cada).
        (6, item_id::ENHANCED_BOW),     // Arqueiro
        (5, item_id::ENHANCED_STAFF),   // Mística
        (5, item_id::ENHANCED_WAND),    // Mística
        (2, item_id::ENHANCED_AXE),     // Espadeiro (também vende machados)
        (2, item_id::ENHANCED_SPEAR),   // Espadeiro
        // Mercador (1) — fallback acessível, vende TUDO tier 2/3. Garante que
        // mesmo no mapa onde só Klaus existe, o player consiga comprar todas.
        (1, item_id::ENHANCED_SWORD),
        (1, item_id::VETERAN_SWORD),
        (1, item_id::ENHANCED_BOW),
        (1, item_id::ENHANCED_STAFF),
        (1, item_id::ENHANCED_WAND),
        (1, item_id::ENHANCED_AXE),
        (1, item_id::ENHANCED_SPEAR),
        // Recursos T1 vendaveis no Mercador — facilita early game (wood/leather/mineral
        // basicos). Buy price = sell_price * 3 setado em UPDATE separado abaixo.
        (1, item_id::WOOD_T1),
        (1, item_id::LEATHER_T1),
        (1, item_id::MINERAL_T1),
    ];
    for (sid, item) in new_shop_items {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vendor_shop_items WHERE shop_id = $1 AND item_id = $2"
        )
        .bind(sid).bind(*item as i32)
        .fetch_one(pool).await?;
        if exists == 0 {
            let next_order: i32 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM vendor_shop_items WHERE shop_id = $1"
            )
            .bind(sid).fetch_one(pool).await?;
            sqlx::query(
                "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
            )
            .bind(sid).bind(*item as i32).bind(next_order)
            .execute(pool).await?;
        }
    }

    // Recursos T1 — buy_price = sell_price * 3 (custa 3x o preco de venda).
    // Idempotente: so seta se ainda for NULL/0 — admin pode editar via web sem
    // ser sobrescrito.
    let t1_resources: &[u16] = &[
        item_id::WOOD_T1, item_id::LEATHER_T1, item_id::MINERAL_T1,
    ];
    for &iid in t1_resources {
        sqlx::query(
            "UPDATE items SET buy_price = sell_price * 3 \
             WHERE id = $1 \
               AND sell_price > 0 \
               AND (buy_price IS NULL OR buy_price = 0)"
        )
        .bind(iid as i32)
        .execute(pool).await?;
    }

    // Farm node drops — seed só se vazio. Replica o comportamento legado:
    // 1 row de material principal por (kind, tier) com qty escalando, +
    // 1 row de gold com chance proporcional ao tier.
    let farm_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM farm_node_drops")
        .fetch_one(pool).await?;
    if farm_count == 0 {
        // (kind, tier, item_id, qty_min, qty_max, chance)
        // qty_base = 2 + tier; qty_extra = +0..2 → range [base, base+2].
        let farm_drops: &[(&str, i32, u16, i32, i32, f32)] = &[
            // Tree → Madeira
            ("Tree",   1, item_id::WOOD_T1,    3, 5, 1.0),
            ("Tree",   2, item_id::WOOD_T2,    4, 6, 1.0),
            ("Tree",   3, item_id::WOOD_T3,    5, 7, 1.0),
            ("Tree",   4, item_id::WOOD_T4,    6, 8, 1.0),
            // Rock → Mineral
            ("Rock",   1, item_id::MINERAL_T1, 3, 5, 1.0),
            ("Rock",   2, item_id::MINERAL_T2, 4, 6, 1.0),
            ("Rock",   3, item_id::MINERAL_T3, 5, 7, 1.0),
            ("Rock",   4, item_id::MINERAL_T4, 6, 8, 1.0),
            // Flower → Couro (default legado; admin troca pra herbal/etc.)
            ("Flower", 1, item_id::LEATHER_T1, 3, 5, 1.0),
            ("Flower", 2, item_id::LEATHER_T2, 4, 6, 1.0),
            ("Flower", 3, item_id::LEATHER_T3, 5, 7, 1.0),
            ("Flower", 4, item_id::LEATHER_T4, 6, 8, 1.0),
            // Gold side-drop por tier (chance = 0.10 * tier; qty = 1 + tier*2)
            ("Tree",   1, item_id::GOLD,       3, 3, 0.10),
            ("Tree",   2, item_id::GOLD,       5, 5, 0.20),
            ("Tree",   3, item_id::GOLD,       7, 7, 0.30),
            ("Tree",   4, item_id::GOLD,       9, 9, 0.40),
            ("Rock",   1, item_id::GOLD,       3, 3, 0.10),
            ("Rock",   2, item_id::GOLD,       5, 5, 0.20),
            ("Rock",   3, item_id::GOLD,       7, 7, 0.30),
            ("Rock",   4, item_id::GOLD,       9, 9, 0.40),
            ("Flower", 1, item_id::GOLD,       3, 3, 0.10),
            ("Flower", 2, item_id::GOLD,       5, 5, 0.20),
            ("Flower", 3, item_id::GOLD,       7, 7, 0.30),
            ("Flower", 4, item_id::GOLD,       9, 9, 0.40),
        ];
        for (kind, tier, item, qmin, qmax, chance) in farm_drops {
            sqlx::query(
                "INSERT INTO farm_node_drops (kind, tier, item_id, qty_min, qty_max, chance) \
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(*kind).bind(tier).bind(*item as i32).bind(qmin).bind(qmax).bind(chance)
            .execute(pool).await?;
        }
        tracing::info!("economy seed: {} farm node drops inseridos", farm_drops.len());
    }

    Ok(())
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    let rows = sqlx::query_as::<_,
        (String, f32, f32, i32, i32, i64, i64, i64, i32, Vec<i32>, i32, i32,
         Option<i16>, Option<f32>, Option<f32>, Option<i16>)>(
        "SELECT name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points, \
                skill_points_earned, skill_points_spent, \
                boat_kind, boat_x, boat_y, boat_dir FROM characters",
    )
    .fetch_all(pool)
    .await?;
    // Query separada pra account_id + visual_json + gold (tuple FromRow limit 16).
    let extras: Vec<(String, Option<i64>, Option<String>, i64)> = sqlx::query_as(
        "SELECT name, account_id, visual_json, gold FROM characters"
    ).fetch_all(pool).await?;
    let extras_map: HashMap<String, (Option<i64>, Option<String>, u64)> =
        extras.into_iter().map(|(n, a, v, g)| (n, (a, v, g.max(0) as u64))).collect();
    // Faction — query separada (TEXT). Parse tolerante; default Peacemain.
    let faction_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, faction FROM characters"
    ).fetch_all(pool).await?;
    let faction_map: HashMap<String, shared::Faction> = faction_rows.into_iter()
        .map(|(n, f)| (n, shared::Faction::from_str_lenient(&f).unwrap_or_default()))
        .collect();
    // Tutorial concluído (epoch). NULL = nunca → login no mundo redireciona pro tutorial.
    let tut_rows: Vec<(String, Option<i64>)> = sqlx::query_as(
        "SELECT name, EXTRACT(EPOCH FROM last_tutorial_completed)::BIGINT FROM characters"
    ).fetch_all(pool).await?;
    let tut_map: HashMap<String, Option<i64>> = tut_rows.into_iter().collect();
    // Boat 2.5D extras: yaw/sail/anchor + mounted_local. Tudo opcional pra
    // compat com rows legacy (sao NULL quando antigos).
    type BoatExtras = (Option<f32>, Option<i16>, Option<f32>, Option<bool>, Option<f32>, Option<f32>);
    let boat_extras: Vec<(String, Option<f32>, Option<i16>, Option<f32>, Option<bool>, Option<f32>, Option<f32>)> = sqlx::query_as(
        "SELECT name, boat_yaw, boat_sail_pos, boat_sail_angle, boat_anchor_dropped, \
                mounted_local_x, mounted_local_y FROM characters"
    ).fetch_all(pool).await?;
    let boat_extras_map: HashMap<String, BoatExtras> = boat_extras.into_iter()
        .map(|(n, y, sp, sa, a, lx, ly)| (n, (y, sp, sa, a, lx, ly)))
        .collect();

    let mut out = HashMap::with_capacity(rows.len());
    for (name, x, y, hp, max_hp, xp, fame, aura, unspent, allocated_vec,
         sp_earned, sp_spent, boat_kind, boat_x, boat_y, boat_dir) in rows
    {
        let (account_id, visual_json, gold) = extras_map.get(&name).cloned().unwrap_or((None, None, 0));
        let faction = faction_map.get(&name).copied().unwrap_or_default();
        let inv = load_inventory(pool, &name).await?;
        let equip = load_equipment(pool, &name).await?;
        let vault = load_vault(pool, &name).await?;
        let profs = load_proficiencies(pool, &name).await?;
        let (quests, faction_points) = crate::quests::load_char(pool, &name).await.unwrap_or_default();
        let mut allocated = [0u32; shared::STAT_COUNT];
        for (i, v) in allocated_vec.into_iter().enumerate().take(shared::STAT_COUNT) {
            allocated[i] = v.max(0) as u32;
        }
        // Boat extras (Boat 2.5D — yaw/sail/anchor + mounted_local).
        let extras = boat_extras_map.get(&name).copied();
        let (b_yaw, b_sail_pos, b_sail_angle, b_anchor, ml_x, ml_y) = extras
            .unwrap_or((None, None, None, None, None, None));
        let boat = match (boat_kind, boat_x, boat_y, boat_dir) {
            (Some(k), Some(bx), Some(by), Some(d)) => {
                let dir = d.max(0) as u8;
                // Fallback: derive yaw de dir se nao tem boat_yaw salvo.
                let yaw = b_yaw.unwrap_or((dir as f32) * std::f32::consts::FRAC_PI_4);
                Some(PersistedBoat {
                    kind: k.max(0) as u16,
                    pos: Vec2::new(bx, by),
                    dir, yaw,
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
        out.insert(
            name.clone(),
            CharacterRow {
                name,
                pos: Vec2::new(x, y),
                hp: Health { current: hp, max: max_hp },
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
         ON CONFLICT(name) DO NOTHING"
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
        // Inventario[0] = arma escolhida (qty 1).
        sqlx::query(
            "INSERT INTO inventory (character_name, slot, item_id, qty)
             VALUES ($1, 0, $2, 1)
             ON CONFLICT (character_name, slot) DO NOTHING"
        )
        .bind(name)
        .bind(starting_weapon as i32)
        .execute(pool)
        .await?;
        // Equipa a arma na slot weapon (mainhand).
        sqlx::query(
            "INSERT INTO equipment (character_name, slot, item_id)
             VALUES ($1, 'weapon', $2)
             ON CONFLICT (character_name, slot) DO UPDATE SET item_id = EXCLUDED.item_id"
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
        if kind < 0 { continue; }
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
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS { continue; }
        if qty <= 0 { continue; }
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
        match slot.as_str() {
            "weapon"   => { eq.weapon   = Some(iid); eq.weapon_inst   = inst; }
            "armor"    => { eq.armor    = Some(iid); eq.armor_inst    = inst; }
            "ring"     => { eq.ring     = Some(iid); eq.ring_inst     = inst; }
            "offhand"  => { eq.offhand  = Some(iid); eq.offhand_inst  = inst; }
            "helm"     => { eq.helm     = Some(iid); eq.helm_inst     = inst; }
            "legs"     => { eq.legs     = Some(iid); eq.legs_inst     = inst; }
            "boots"    => { eq.boots    = Some(iid); eq.boots_inst    = inst; }
            "gloves"   => { eq.gloves   = Some(iid); eq.gloves_inst   = inst; }
            "belt"     => { eq.belt     = Some(iid); eq.belt_inst     = inst; }
            "cape"     => { eq.cape     = Some(iid); eq.cape_inst     = inst; }
            "necklace"     => { eq.necklace     = Some(iid); eq.necklace_inst     = inst; }
            _ => {}
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
        if slot < 0 || (slot as usize) >= shared::INVENTORY_SLOTS { continue; }
        if qty <= 0 { continue; }
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
             VALUES ($1,$2,$3,$4,$5,$6)"
        )
        .bind(enemy_kind as i32).bind(item_id as i32).bind(qty as i32)
        .bind(rarity as i16).bind(item_level as i32).bind(refinement as i16)
        .execute(&pool).await;
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
        while let Some(batch) = rx.recv().await {
            if let Err(e) = write_batch(&pool, &batch).await {
                tracing::warn!("persist write failed: {e:?}");
            }
        }
        tracing::debug!("persist writer task exiting");
    });
    tx
}

async fn write_batch(pool: &PgPool, batch: &SaveBatch) -> Result<()> {
    if batch.rows.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    for row in &batch.rows {
        let allocated_vec: Vec<i32> = row.allocated_points.iter().map(|&v| v as i32).collect();
        let (boat_kind, boat_x, boat_y, boat_dir): (Option<i16>, Option<f32>, Option<f32>, Option<i16>) =
            match row.boat {
                Some(b) => (Some(b.kind as i16), Some(b.pos.x), Some(b.pos.y), Some(b.dir as i16)),
                None    => (None, None, None, None),
            };
        let (boat_yaw, boat_sail_pos, boat_sail_angle, boat_anchor): (Option<f32>, Option<i16>, Option<f32>, Option<bool>) =
            match row.boat {
                Some(b) => (Some(b.yaw), Some(b.sail_position as i16), Some(b.sail_angle), Some(b.anchor_dropped)),
                None    => (None, None, None, None),
            };
        let (mounted_local_x, mounted_local_y): (Option<f32>, Option<f32>) =
            match row.mounted_local {
                Some(p) => (Some(p.x), Some(p.y)),
                None    => (None, None),
            };
        // visual_json: persiste o VisualConfig em vigor (wardrobe mid-game).
        // None = mantem o que ja existe no banco (mas atualizamos sempre que
        // session.visual estiver setado, o que e o caso pra players logados).
        let visual_json: Option<String> = row.visual.as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, fame, aura,
                                     unspent_points, allocated_points,
                                     skill_points_earned, skill_points_spent,
                                     boat_kind, boat_x, boat_y, boat_dir,
                                     gold, visual_json, updated,
                                     boat_yaw, boat_sail_pos, boat_sail_angle,
                                     boat_anchor_dropped, mounted_local_x, mounted_local_y)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)
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
               mounted_local_y = EXCLUDED.mounted_local_y",
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
        .execute(&mut *tx)
        .await?;

        // Inventario: delete-all + insert-rows pra ser simples. O FK cascade
        // ja garante que deletar a linha do character limpa a inventory.
        sqlx::query("DELETE FROM inventory WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.inventory.iter().enumerate() {
            if slot.qty == 0 { continue; }
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
        for (slot_name, item_opt, inst_opt) in [
            ("weapon",   row.equipment.weapon,   row.equipment.weapon_inst),
            ("armor",    row.equipment.armor,    row.equipment.armor_inst),
            ("ring",     row.equipment.ring,     row.equipment.ring_inst),
            ("offhand",  row.equipment.offhand,  row.equipment.offhand_inst),
            ("helm",     row.equipment.helm,     row.equipment.helm_inst),
            ("legs",     row.equipment.legs,     row.equipment.legs_inst),
            ("boots",    row.equipment.boots,    row.equipment.boots_inst),
            ("gloves",   row.equipment.gloves,   row.equipment.gloves_inst),
            ("belt",     row.equipment.belt,     row.equipment.belt_inst),
            ("cape",     row.equipment.cape,     row.equipment.cape_inst),
            ("necklace",     row.equipment.necklace,     row.equipment.necklace_inst),
        ] {
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

        // Proficiencias: upsert por prof_kind.
        for (i, xp) in row.proficiencies.iter().enumerate() {
            if *xp == 0 { continue; }
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
    tx.commit().await?;
    // Quests + pontos de facção (fora da tx; reconcilia character_quests).
    for row in &batch.rows {
        let _ = crate::quests::save_char(pool, &row.name, &row.quests, row.faction_points).await;
    }
    Ok(())
}
