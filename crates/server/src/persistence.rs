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

#[derive(Debug, Clone)]
pub struct CharacterRow {
    pub name: String,
    pub pos: Vec2,
    pub hp: Health,
    pub xp: u64,
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
    /// Skills aprendidas + rank atual + slot equipado (None pra passivas
    /// ou ativas não-equipadas).
    pub learned_skills: Vec<shared::LearnedSkill>,
}

/// Abre o pool Postgres, garante schema criado.
pub async fn open_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await?;

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
    .execute(&pool)
    .await?;

    sqlx::query(
        "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS class TEXT NOT NULL DEFAULT 'warrior'",
    )
    .execute(&pool)
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
    .execute(&pool)
    .await?;

    // Migracao inline: colunas de progressao. IF NOT EXISTS para idempotencia.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS xp BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS fame BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS aura BIGINT NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    // Pontos de atributo: unspent counter + array de 6 alocados.
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS unspent_points INTEGER NOT NULL DEFAULT 0")
        .execute(&pool)
        .await?;
    sqlx::query("ALTER TABLE characters ADD COLUMN IF NOT EXISTS allocated_points INTEGER[] NOT NULL DEFAULT '{0,0,0,0,0,0}'")
        .execute(&pool)
        .await?;
    // Migration M6: design atual tem 6 stats (FOR/DES/INT/VIT/SPD/RES). Linhas
    // antigas com 5 elementos ganham um 0 no slot RES, preservando pontos ja
    // alocados. Idempotente — arrays de 6 nao sao tocados.
    sqlx::query(
        "UPDATE characters
         SET allocated_points = allocated_points || ARRAY[0]::INTEGER[]
         WHERE array_length(allocated_points, 1) = 5"
    ).execute(&pool).await?;

    // Migration M7: escudo (item_id=7) sai do slot 'armor' e vai pra 'offhand'.
    // Idempotente: se ja moveu, UPDATE nao acha mais nada.
    sqlx::query(
        "UPDATE equipment SET slot = 'offhand'
         WHERE slot = 'armor' AND item_id = 7"
    ).execute(&pool).await?;

    // Migration M8: deixa escudo comprável no shop. So aplica se o DB ja
    // tinha shield com buy_price=NULL (preserva tweaks manuais que o user
    // fez via SQL).
    sqlx::query(
        "UPDATE items SET buy_price = 50, shop_order = 9
         WHERE id = 7 AND buy_price IS NULL AND shop_order IS NULL"
    ).execute(&pool).await?;

    // Migration M9: garante que o Mercador (shop_id=1, vendor Klaus no mapa)
    // venda escudo. ON CONFLICT DO NOTHING pra ser idempotente em DBs onde
    // ja foi adicionado.
    sqlx::query(
        "INSERT INTO vendor_shop_items (shop_id, item_id, sort_order)
         VALUES (1, 7, 9) ON CONFLICT DO NOTHING"
    ).execute(&pool).await?;

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
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS proficiencies (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            prof_kind      INTEGER NOT NULL,
            xp             BIGINT  NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, prof_kind)
        )",
    )
    .execute(&pool)
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
    .execute(&pool)
    .await?;
    // Migration Fase A: instance_data armazena ItemInstance serializada
    // como JSON. NULL pra stackáveis e itens legacy.
    sqlx::query(
        "ALTER TABLE inventory ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS equipment (
            character_name TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            slot           TEXT NOT NULL,
            item_id        INTEGER NOT NULL,
            PRIMARY KEY (character_name, slot)
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "ALTER TABLE equipment ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(&pool).await?;

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
    .execute(&pool)
    .await?;
    sqlx::query(
        "ALTER TABLE vault ADD COLUMN IF NOT EXISTS instance_data TEXT NULL"
    ).execute(&pool).await?;

    // ── Economy tables ──────────────────────────────────────────────────────
    // Bumpa `economy_version.version` em qualquer ferramenta SQL pra forçar
    // hot-reload no servidor.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS economy_version (
            id          SMALLINT PRIMARY KEY DEFAULT 1,
            version     BIGINT NOT NULL DEFAULT 1,
            updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            CHECK (id = 1)
        )",
    ).execute(&pool).await?;
    sqlx::query("INSERT INTO economy_version (id, version) VALUES (1, 1) ON CONFLICT DO NOTHING")
        .execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS items (
            id          INTEGER PRIMARY KEY,
            name        TEXT    NOT NULL,
            sell_price  INTEGER NOT NULL DEFAULT 0,
            buy_price   INTEGER,
            shop_order  INTEGER,
            stack_max   INTEGER NOT NULL DEFAULT 1
        )",
    ).execute(&pool).await?;
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
        sqlx::query(&format!("ALTER TABLE items {col}")).execute(&pool).await?;
    }
    // Override de item_level no drop por enemy_kind (era hardcoded em
    // world.rs::spawn_loot_drops). NULL = usa items.item_level como fallback.
    sqlx::query("ALTER TABLE enemy_kinds ADD COLUMN IF NOT EXISTS loot_item_level INTEGER")
        .execute(&pool).await?;

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
    ).execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS loot_drops (
            id          SERIAL PRIMARY KEY,
            enemy_kind  INTEGER NOT NULL REFERENCES enemy_kinds(kind) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            qty_min     INTEGER NOT NULL,
            qty_max     INTEGER NOT NULL,
            chance      REAL    NOT NULL DEFAULT 1.0
        )",
    ).execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_loot_drops_kind ON loot_drops(enemy_kind)")
        .execute(&pool).await?;

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
    ).execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_ts   ON item_drops_log(ts DESC)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_item ON item_drops_log(item_id)")
        .execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_drops_log_kind ON item_drops_log(enemy_kind)")
        .execute(&pool).await?;

    // Vendor shops — cada vendor tem um shop_id que aponta pra uma lista
    // curada de itens. Permite "espadeiro" que só vende espadas, "alquimista"
    // que só vende poções, etc., independente da categoria genérica.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shops (
            shop_id  INTEGER PRIMARY KEY,
            name     TEXT NOT NULL
        )",
    ).execute(&pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vendor_shop_items (
            shop_id     INTEGER NOT NULL REFERENCES vendor_shops(shop_id) ON DELETE CASCADE,
            item_id     INTEGER NOT NULL,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (shop_id, item_id)
        )",
    ).execute(&pool).await?;

    // ── Skills (Phase 1 / M11) ──────────────────────────────────────────────
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS skills (
            id              INTEGER PRIMARY KEY,
            name            TEXT NOT NULL,
            description     TEXT NOT NULL DEFAULT '',
            prof            TEXT NOT NULL,
            tier            SMALLINT NOT NULL,
            is_passive      BOOLEAN NOT NULL,
            path            TEXT,
            unlock_char_lvl SMALLINT NOT NULL DEFAULT 1,
            unlock_prof_lvl SMALLINT NOT NULL DEFAULT 1,
            usable_with     TEXT[],
            cost_mp         INTEGER NOT NULL DEFAULT 0,
            cost_stamina    INTEGER NOT NULL DEFAULT 0,
            cooldown_s      REAL    NOT NULL DEFAULT 0.0,
            cast_time_s     REAL    NOT NULL DEFAULT 0.0,
            target_type     TEXT    NOT NULL DEFAULT 'none',
            range_tiles     REAL    NOT NULL DEFAULT 0.0,
            radius_tiles    REAL    NOT NULL DEFAULT 0.0,
            base_damage     INTEGER NOT NULL DEFAULT 0,
            base_heal       INTEGER NOT NULL DEFAULT 0,
            scaling_atk     REAL    NOT NULL DEFAULT 0.0,
            scaling_wis     REAL    NOT NULL DEFAULT 0.0,
            scaling_dex     REAL    NOT NULL DEFAULT 0.0,
            per_rank_dmg_pct  REAL  NOT NULL DEFAULT 0.0,
            per_rank_cd_pct   REAL  NOT NULL DEFAULT 0.0,
            per_rank_cost_pct REAL  NOT NULL DEFAULT 0.0,
            rank5_payload   JSONB,
            rank10_payload  JSONB,
            effect_payload  JSONB,
            icon_path       TEXT,
            vfx_id          TEXT,
            active          BOOLEAN NOT NULL DEFAULT TRUE
        )",
    ).execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_skills_prof ON skills(prof)")
        .execute(&pool).await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS player_skills (
            character_name  TEXT NOT NULL REFERENCES characters(name) ON DELETE CASCADE,
            skill_id        INTEGER NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
            rank            SMALLINT NOT NULL DEFAULT 1,
            equipped_slot   SMALLINT,
            PRIMARY KEY (character_name, skill_id)
        )",
    ).execute(&pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_pskills_char ON player_skills(character_name)")
        .execute(&pool).await?;
    // Slot 0..5 único por player (impede 2 skills no mesmo slot).
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_pskills_slot
         ON player_skills(character_name, equipped_slot)
         WHERE equipped_slot IS NOT NULL"
    ).execute(&pool).await?;

    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS skill_points_earned INTEGER NOT NULL DEFAULT 0"
    ).execute(&pool).await?;
    sqlx::query(
        "ALTER TABLE characters ADD COLUMN IF NOT EXISTS skill_points_spent INTEGER NOT NULL DEFAULT 0"
    ).execute(&pool).await?;

    // Migration M11: backfill SP retroativo pra players existentes.
    //
    // Cada char ganha SP = level atual. A primeira tentativa usou
    // `1 + sqrt(xp/100)` (fórmula do comentário em level_of_xp), mas a
    // IMPLEMENTAÇÃO Rust é stair-step cumulativo. Diverge em L>5 e dava
    // SP=100 com só ~1M xp. Usa função PL/pgSQL idêntica ao Rust agora.
    sqlx::query(
        "CREATE OR REPLACE FUNCTION compute_char_level(p_xp bigint)
         RETURNS int AS $$
         DECLARE
            lvl int := 1;
            need bigint := 100;
            remaining bigint := GREATEST(p_xp, 0);
         BEGIN
            WHILE remaining >= need AND lvl < 100 LOOP
                remaining := remaining - need;
                lvl := lvl + 1;
                need := (lvl::bigint) * (lvl::bigint) * 100;
            END LOOP;
            RETURN lvl;
         END;
         $$ LANGUAGE plpgsql IMMUTABLE"
    ).execute(&pool).await?;

    // M11d: corrige o backfill anterior. Re-sync SP_earned pra char_level
    // sempre que ainda não tem skills aprendidas (estado clean = safe pra
    // sobrescrever). Players que aprenderam skills mantêm o SP atual pra
    // não invalidar gastos.
    sqlx::query(
        "UPDATE characters c SET skill_points_earned = compute_char_level(c.xp)
         WHERE NOT EXISTS (
             SELECT 1 FROM player_skills ps WHERE ps.character_name = c.name
         )
         AND c.skill_points_earned != compute_char_level(c.xp)"
    ).execute(&pool).await?;

    // Migration M11b: rename HAMMER → AXE no DB. Idempotente:
    // só atualiza se ainda tem o nome antigo "Martelo de Guerra".
    sqlx::query(
        "UPDATE items SET name = 'Machado'
         WHERE id = 25 AND name = 'Martelo de Guerra'"
    ).execute(&pool).await?;

    // Migration M11c: desativa SCIMITAR (24) e CROSSBOW (27) — removidos
    // do design Phase 1. Items continuam no DB pra players que tinham no
    // inventário (vendem/guardam), mas não dropam mais nem podem equipar.
    sqlx::query(
        "UPDATE items SET active = FALSE WHERE id IN (24, 27) AND active = TRUE"
    ).execute(&pool).await?;

    seed_economy_if_needed(&pool).await?;
    seed_skills_if_needed(&pool).await?;

    Ok(pool)
}

/// Seed inicial das 64 skills (8 profs × 8 skills).
///
/// Idempotência: `ON CONFLICT (id) DO UPDATE` só sobrescreve quando a
/// `description` é vazia (sentinel "ainda não editada via admin"). Após o
/// admin tocar a skill, futuras boots não a alteram.
///
/// IDs reservados 1001..1064 (range distinto dos items 1..99).
async fn seed_skills_if_needed(pool: &PgPool) -> Result<()> {
    use shared::SKILL_TIER_UNLOCKS;
    let (t1c, t1p) = (SKILL_TIER_UNLOCKS[0].0 as i16, SKILL_TIER_UNLOCKS[0].1 as i16);
    let (t2c, t2p) = (SKILL_TIER_UNLOCKS[1].0 as i16, SKILL_TIER_UNLOCKS[1].1 as i16);
    let (t3c, t3p) = (SKILL_TIER_UNLOCKS[2].0 as i16, SKILL_TIER_UNLOCKS[2].1 as i16);
    let (t4c, t4p) = (SKILL_TIER_UNLOCKS[3].0 as i16, SKILL_TIER_UNLOCKS[3].1 as i16);

    // Categorias de cross-weapon (passa como Vec<&str>; None = universal/All)
    let melee:   Vec<&str> = vec!["Sword", "Axe", "Spear", "Dagger", "Unarmed"];
    let h1_melee: Vec<&str> = vec!["Sword", "Dagger", "Spear"];
    let heavy:   Vec<&str> = vec!["Axe", "Sword"]; // GS está em Sword prof
    let caster:  Vec<&str> = vec!["Wand", "Staff"];
    let ranged:  Vec<&str> = vec!["Bow"];
    let stealth: Vec<&str> = vec!["Dagger"];

    struct S<'a> {
        id: i32,
        name: &'a str,
        prof: &'a str,
        tier: i16,
        is_passive: bool,
        path: &'a str,
        unlock_char: i16,
        unlock_prof: i16,
        usable_with: Option<Vec<&'a str>>, // None = ALL
        cost_mp: i32,
        cost_st: i32,
        cd: f32,
        cast: f32,
        target: &'a str,
        range_t: f32,
        radius: f32,
        base_dmg: i32,
        base_heal: i32,
        scal_atk: f32,
        scal_wis: f32,
        scal_dex: f32,
        rank_dmg: f32, // % por rank (0.10 = +10%)
        rank_cd: f32,
        rank_cost: f32,
        desc: &'a str,
    }

    // Atalhos pra cross-weapon arrays.
    let none: Option<Vec<&str>> = None;
    let melee_v   = Some(melee.clone());
    let h1_v      = Some(h1_melee.clone());
    let heavy_v   = Some(heavy.clone());
    let caster_v  = Some(caster.clone());
    let ranged_v  = Some(ranged.clone());
    let stealth_v = Some(stealth.clone());
    let unarmed_v: Option<Vec<&str>> = Some(vec!["Unarmed"]);
    let spear_v: Option<Vec<&str>>   = Some(vec!["Spear"]);
    let axe_v: Option<Vec<&str>>     = Some(vec!["Axe", "Sword"]); // crusher = HEAVY
    let dagger_sword_v: Option<Vec<&str>> = Some(vec!["Dagger", "Sword"]);

    let seed: Vec<S> = vec![
        // ── SWORD (1001..1008) — Duelist (D) + Tank (T) ──────────────────────
        S{ id:1001, name:"Riposte",          prof:"Sword", tier:1, is_passive:false, path:"duelist",  unlock_char:t1c, unlock_prof:t1p,
            usable_with: h1_v.clone(), cost_mp:0, cost_st:10, cd:0.0, cast:0.0, target:"self",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0,
            scal_atk:1.5, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Após parry com sucesso: contra-ataque livre +50% dmg, sem cooldown."
        },
        S{ id:1002, name:"Combat Stance",    prof:"Sword", tier:1, is_passive:true,  path:"duelist",  unlock_char:t1c, unlock_prof:t1p,
            usable_with: dagger_sword_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.01, rank_cd:0.0, rank_cost:0.0,
            desc:"+1% atk speed por rank (Sword/Dagger). r5: +5% crit. r10: parry window +50ms."
        },
        S{ id:1003, name:"Shield Bash",      prof:"Sword", tier:2, is_passive:false, path:"tank",     unlock_char:t2c, unlock_prof:t2p,
            usable_with: Some(vec!["Sword"]), cost_mp:0, cost_st:30, cd:8.0, cast:0.0, target:"cone",
            range_t:1.6, radius:0.0, base_dmg:8, base_heal:0,
            scal_atk:0.5, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Stuna alvo 1.2s + dano. Requer escudo equipado."
        },
        S{ id:1004, name:"Bulwark",          prof:"Sword", tier:2, is_passive:true,  path:"tank",     unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.005, rank_cd:0.0, rank_cost:0.0,
            desc:"+0.5% block reduction por rank. r5: -5% block stam cost. r10: buff de 10s após bloquear."
        },
        S{ id:1005, name:"Sword Dance",      prof:"Sword", tier:3, is_passive:false, path:"duelist",  unlock_char:t3c, unlock_prof:t3p,
            usable_with: h1_v.clone(), cost_mp:0, cost_st:40, cd:14.0, cast:0.0, target:"cone",
            range_t:2.0, radius:0.0, base_dmg:14, base_heal:0,
            scal_atk:1.2, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"Combo locked 3-hit; último guaranteed crit."
        },
        S{ id:1006, name:"Taunt",            prof:"Sword", tier:3, is_passive:false, path:"tank",     unlock_char:t3c, unlock_prof:t3p,
            usable_with: melee_v.clone(), cost_mp:0, cost_st:25, cd:10.0, cast:0.0, target:"self",
            range_t:4.0, radius:4.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Aggro forçado em raio 4 por 4s. Mobs perto te atacam preferencialmente."
        },
        S{ id:1007, name:"Master's Counter", prof:"Sword", tier:4, is_passive:false, path:"duelist",  unlock_char:t4c, unlock_prof:t4p,
            usable_with: h1_v.clone(), cost_mp:0, cost_st:50, cd:30.0, cast:0.0, target:"self",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0,
            scal_atk:1.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Stance 2s: parries automático + contra-ataque. Período de superioridade."
        },
        S{ id:1008, name:"Iron Will",        prof:"Sword", tier:4, is_passive:true,  path:"tank",     unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.03, rank_cd:0.0, rank_cost:0.0,
            desc:"Abaixo de 30% HP: -30% dmg recebido. r5: +1%/s regen. r10: imune stagger."
        },

        // ── AXE (1009..1016) — Crusher (Cr) + Berserker (Br) ─────────────────
        S{ id:1009, name:"Cleave",           prof:"Axe",   tier:1, is_passive:false, path:"crusher",  unlock_char:t1c, unlock_prof:t1p,
            usable_with: heavy_v.clone(), cost_mp:0, cost_st:25, cd:6.0, cast:0.0, target:"cone",
            range_t:2.0, radius:0.0, base_dmg:12, base_heal:0,
            scal_atk:1.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Cone 120° na frente. Atinge múltiplos alvos com 100% atk."
        },
        S{ id:1010, name:"Heavy Hands",      prof:"Axe",   tier:1, is_passive:true,  path:"crusher",  unlock_char:t1c, unlock_prof:t1p,
            usable_with: heavy_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.01, rank_cd:0.0, rank_cost:0.0,
            desc:"+1% atk dmg por rank com Axe/GS. r5: +5% knockback. r10: ignora 50% def."
        },
        S{ id:1011, name:"Bloodthirst",      prof:"Axe",   tier:2, is_passive:false, path:"berserker",unlock_char:t2c, unlock_prof:t2p,
            usable_with: melee_v.clone(), cost_mp:0, cost_st:35, cd:18.0, cast:0.0, target:"self",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Buff 4s: +20% atk speed + 10% lifesteal. Ataques curam você."
        },
        S{ id:1012, name:"Frenzy",           prof:"Axe",   tier:2, is_passive:true,  path:"berserker",unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.005, rank_cd:0.0, rank_cost:0.0,
            desc:"Abaixo de 50% HP: +0.5% atk speed por rank. r5: +crit. r10: +mov speed."
        },
        S{ id:1013, name:"Whirlwind",        prof:"Axe",   tier:3, is_passive:false, path:"crusher",  unlock_char:t3c, unlock_prof:t3p,
            usable_with: heavy_v.clone(), cost_mp:0, cost_st:50, cd:18.0, cast:0.0, target:"aoe_circle",
            range_t:0.0, radius:2.5, base_dmg:8, base_heal:0,
            scal_atk:0.7, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"Spin 360° raio 2.5; 4 hits ao longo de 1.5s."
        },
        S{ id:1014, name:"Decapitate",       prof:"Axe",   tier:3, is_passive:false, path:"berserker",unlock_char:t3c, unlock_prof:t3p,
            usable_with: melee_v.clone(), cost_mp:0, cost_st:40, cd:25.0, cast:0.0, target:"cone",
            range_t:1.8, radius:0.0, base_dmg:25, base_heal:0,
            scal_atk:2.5, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Alvo abaixo de 20% HP: instakill. Senão 250% atk."
        },
        S{ id:1015, name:"Earthshatter",     prof:"Axe",   tier:4, is_passive:false, path:"crusher",  unlock_char:t4c, unlock_prof:t4p,
            usable_with: heavy_v.clone(), cost_mp:0, cost_st:60, cd:35.0, cast:0.6, target:"aoe_circle",
            range_t:0.0, radius:5.0, base_dmg:30, base_heal:0,
            scal_atk:1.5, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Slam ground; AoE radius 5 com stun 2s."
        },
        S{ id:1016, name:"Unstoppable",      prof:"Axe",   tier:4, is_passive:true,  path:"berserker",unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.01, rank_cost:0.0,
            desc:"Imune a stagger acima de 75% HP. r5: aplica também a parry. r10: -10% global cd."
        },

        // ── SPEAR (1017..1024) — Reach (Re) + Charger (Ch) ───────────────────
        S{ id:1017, name:"Lunge",            prof:"Spear", tier:1, is_passive:false, path:"charger",  unlock_char:t1c, unlock_prof:t1p,
            usable_with: spear_v.clone(), cost_mp:0, cost_st:20, cd:7.0, cast:0.0, target:"line",
            range_t:4.0, radius:0.0, base_dmg:10, base_heal:0,
            scal_atk:1.2, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"Dash 4 tiles + thrust com 120% atk."
        },
        S{ id:1018, name:"Long Reach",       prof:"Spear", tier:1, is_passive:true,  path:"reach",    unlock_char:t1c, unlock_prof:t1p,
            usable_with: spear_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.0, rank_cost:0.0,
            desc:"+0.05/rank melee range com spear. r5: -10% atk speed penalty. r10: cone +10°."
        },
        S{ id:1019, name:"Sweep",            prof:"Spear", tier:2, is_passive:false, path:"reach",    unlock_char:t2c, unlock_prof:t2p,
            usable_with: Some(vec!["Spear","Axe","Sword"]), cost_mp:0, cost_st:25, cd:10.0, cast:0.0, target:"cone",
            range_t:2.0, radius:0.0, base_dmg:8, base_heal:0,
            scal_atk:0.8, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"Arco 180°; knockback 1 tile."
        },
        S{ id:1020, name:"Phalanx",          prof:"Spear", tier:2, is_passive:true,  path:"reach",    unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.005, rank_cd:0.0, rank_cost:0.0,
            desc:"Sem mover por 1s: +0.5%/rank block reduction. r5: aplica a parry. r10: reflete 20% dmg."
        },
        S{ id:1021, name:"Impale",           prof:"Spear", tier:3, is_passive:false, path:"reach",    unlock_char:t3c, unlock_prof:t3p,
            usable_with: spear_v.clone(), cost_mp:0, cost_st:30, cd:14.0, cast:0.0, target:"line",
            range_t:2.5, radius:0.0, base_dmg:18, base_heal:0,
            scal_atk:2.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"200% atk single-target + bleed 6s."
        },
        S{ id:1022, name:"Charge",           prof:"Spear", tier:3, is_passive:false, path:"charger",  unlock_char:t3c, unlock_prof:t3p,
            usable_with: spear_v.clone(), cost_mp:0, cost_st:35, cd:18.0, cast:0.0, target:"line",
            range_t:6.0, radius:0.0, base_dmg:14, base_heal:0,
            scal_atk:1.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.05, rank_cost:0.0,
            desc:"Sprint 6 tiles + dmg + stun 1s no impacto."
        },
        S{ id:1023, name:"Dragon Tail",      prof:"Spear", tier:4, is_passive:false, path:"charger",  unlock_char:t4c, unlock_prof:t4p,
            usable_with: spear_v.clone(), cost_mp:0, cost_st:60, cd:35.0, cast:0.0, target:"aoe_circle",
            range_t:10.0, radius:3.0, base_dmg:25, base_heal:0,
            scal_atk:1.5, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Pole-vault: pula 10 tiles, AoE landing radius 3."
        },
        S{ id:1024, name:"Spearman's Resolve",prof:"Spear",tier:4, is_passive:true,  path:"reach",    unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.0, rank_cost:0.0,
            desc:"First hit em alvo: +5%/rank dmg. r5: +crit. r10: aplica bleed."
        },

        // ── DAGGER (1025..1032) — Assassin (As) + Venom (V) ──────────────────
        S{ id:1025, name:"Backstab",         prof:"Dagger",tier:1, is_passive:false, path:"assassin", unlock_char:t1c, unlock_prof:t1p,
            usable_with: stealth_v.clone(), cost_mp:0, cost_st:20, cd:5.0, cast:0.0, target:"cone",
            range_t:1.4, radius:0.0, base_dmg:10, base_heal:0,
            scal_atk:2.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Pelas costas: +200% dmg. Senão dmg base."
        },
        S{ id:1026, name:"Sharp Edge",       prof:"Dagger",tier:1, is_passive:true,  path:"assassin", unlock_char:t1c, unlock_prof:t1p,
            usable_with: dagger_sword_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.003, rank_cd:0.0, rank_cost:0.0,
            desc:"+0.3%/rank crit chance. r5: +10% crit dmg. r10: bleed em crit."
        },
        S{ id:1027, name:"Vanish",           prof:"Dagger",tier:2, is_passive:false, path:"assassin", unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:30, cd:25.0, cast:0.0, target:"self",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Invisível 4s + 50% mov speed. Quebra ao atacar com bonus crit."
        },
        S{ id:1028, name:"Toxic Coating",    prof:"Dagger",tier:2, is_passive:true,  path:"venom",    unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Auto-attacks aplicam poison +1/rank dmg/s 4s. r5: stack até 3x. r10: contagia em morte."
        },
        S{ id:1029, name:"Poison Strike",    prof:"Dagger",tier:3, is_passive:false, path:"venom",    unlock_char:t3c, unlock_prof:t3p,
            usable_with: melee_v.clone(), cost_mp:0, cost_st:30, cd:12.0, cast:0.0, target:"cone",
            range_t:1.5, radius:0.0, base_dmg:12, base_heal:0,
            scal_atk:1.5, scal_wis:0.0, scal_dex:0.3, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"150% atk + heavy poison stack 6s."
        },
        S{ id:1030, name:"Shadowstep",       prof:"Dagger",tier:3, is_passive:false, path:"assassin", unlock_char:t3c, unlock_prof:t3p,
            usable_with: stealth_v.clone(), cost_mp:0, cost_st:20, cd:15.0, cast:0.0, target:"line",
            range_t:6.0, radius:0.0, base_dmg:0, base_heal:0,
            scal_atk:0.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Teleport atrás do alvo (range 6). Próximo hit guaranteed crit."
        },
        S{ id:1031, name:"Death Mark",       prof:"Dagger",tier:4, is_passive:false, path:"venom",    unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:0, cost_st:30, cd:25.0, cast:0.0, target:"line",
            range_t:8.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.0, rank_cost:0.0,
            desc:"Marca alvo: +30% dmg recebido (todas fontes) por 8s."
        },
        S{ id:1032, name:"Killer Instinct",  prof:"Dagger",tier:4, is_passive:true,  path:"assassin", unlock_char:t4c, unlock_prof:t4p,
            usable_with: dagger_sword_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Crit reseta cd Backstab. r5: kill +30% mov 5s. r10: kill +1 SP por dia."
        },

        // ── BOW (1033..1040) — Sniper (S) + Trapper (Tr) ─────────────────────
        S{ id:1033, name:"Power Shot",       prof:"Bow",   tier:1, is_passive:false, path:"sniper",   unlock_char:t1c, unlock_prof:t1p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:30, cd:6.0, cast:0.6, target:"projectile",
            range_t:14.0, radius:0.0, base_dmg:14, base_heal:0,
            scal_atk:2.0, scal_wis:0.0, scal_dex:0.5, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Charge 0.6s + tiro forte 200% atk single-target."
        },
        S{ id:1034, name:"Eagle Eye",        prof:"Bow",   tier:1, is_passive:true,  path:"sniper",   unlock_char:t1c, unlock_prof:t1p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.0, rank_cost:0.0,
            desc:"+5%/rank range com bow. r5: +crit em alvos longe. r10: ignora 50% def em headshot."
        },
        S{ id:1035, name:"Caltrops",         prof:"Bow",   tier:2, is_passive:false, path:"trapper",  unlock_char:t2c, unlock_prof:t2p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:25, cd:12.0, cast:0.0, target:"aoe_circle",
            range_t:5.0, radius:2.0, base_dmg:4, base_heal:0,
            scal_atk:0.3, scal_wis:0.0, scal_dex:0.4, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Drop trap radius 2; slow 50% + dmg ao pisar (10s duração)."
        },
        S{ id:1036, name:"Quick Draw",       prof:"Bow",   tier:2, is_passive:true,  path:"sniper",   unlock_char:t2c, unlock_prof:t2p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.0, rank_cost:0.0,
            desc:"Sem mover 1s: 1ª flecha guaranteed crit. r5: +25% dmg. r10: pierce 1 alvo."
        },
        S{ id:1037, name:"Multishot",        prof:"Bow",   tier:3, is_passive:false, path:"sniper",   unlock_char:t3c, unlock_prof:t3p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:50, cd:10.0, cast:0.0, target:"projectile",
            range_t:14.0, radius:0.0, base_dmg:6, base_heal:0,
            scal_atk:0.7, scal_wis:0.0, scal_dex:0.4, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"5 flechas com spread 30° (substitui SECONDARY antigo)."
        },
        S{ id:1038, name:"Smoke Bomb",       prof:"Bow",   tier:3, is_passive:false, path:"trapper",  unlock_char:t3c, unlock_prof:t3p,
            usable_with: none.clone(), cost_mp:0, cost_st:30, cd:18.0, cast:0.0, target:"aoe_circle",
            range_t:5.0, radius:3.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.05, rank_cost:0.0,
            desc:"Radius 3; blind 4s (alvos perdem aim/atk)."
        },
        S{ id:1039, name:"Rain of Arrows",   prof:"Bow",   tier:4, is_passive:false, path:"sniper",   unlock_char:t4c, unlock_prof:t4p,
            usable_with: ranged_v.clone(), cost_mp:0, cost_st:80, cd:30.0, cast:0.5, target:"aoe_circle",
            range_t:10.0, radius:4.0, base_dmg:6, base_heal:0,
            scal_atk:0.4, scal_wis:0.0, scal_dex:0.3, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"AoE radius 4; 8 flechas ao longo de 3s."
        },
        S{ id:1040, name:"Hunter's Mark",    prof:"Bow",   tier:4, is_passive:true,  path:"trapper",  unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.0, rank_cost:0.0,
            desc:"First hit marca alvo: +20% dmg de todas fontes 6s. r5: marca aparece no minimap. r10: kill estende 4s."
        },

        // ── WAND (1041..1048) — Pyro (Py) + Frost (Fr) ───────────────────────
        S{ id:1041, name:"Fireball",         prof:"Wand",  tier:1, is_passive:false, path:"pyro",     unlock_char:t1c, unlock_prof:t1p,
            usable_with: caster_v.clone(), cost_mp:30, cost_st:0, cd:4.0, cast:0.3, target:"projectile",
            range_t:10.0, radius:1.5, base_dmg:18, base_heal:0,
            scal_atk:0.0, scal_wis:0.8, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Projétil AoE radius 1.5; 80% wis dano."
        },
        S{ id:1042, name:"Mana Pool",        prof:"Wand",  tier:1, is_passive:true,  path:"pyro",     unlock_char:t1c, unlock_prof:t1p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.0, rank_cost:0.0,
            desc:"+5/rank mp_max. r5: +0.5 mp regen/s. r10: -5% spell costs."
        },
        S{ id:1043, name:"Frost Bolt",       prof:"Wand",  tier:2, is_passive:false, path:"frost",    unlock_char:t2c, unlock_prof:t2p,
            usable_with: caster_v.clone(), cost_mp:35, cost_st:0, cd:8.0, cast:0.3, target:"projectile",
            range_t:10.0, radius:0.0, base_dmg:14, base_heal:0,
            scal_atk:0.0, scal_wis:0.7, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Projétil + slow 50% por 3s."
        },
        S{ id:1044, name:"Ignite",           prof:"Wand",  tier:2, is_passive:true,  path:"pyro",     unlock_char:t2c, unlock_prof:t2p,
            usable_with: caster_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Spells de fogo aplicam burn 3s. r5: stack 2x. r10: spread radius 1 ao matar."
        },
        S{ id:1045, name:"Meteor",           prof:"Wand",  tier:3, is_passive:false, path:"pyro",     unlock_char:t3c, unlock_prof:t3p,
            usable_with: caster_v.clone(), cost_mp:80, cost_st:0, cd:25.0, cast:1.2, target:"aoe_circle",
            range_t:8.0, radius:4.0, base_dmg:40, base_heal:0,
            scal_atk:0.0, scal_wis:2.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Wind-up 1.2s; AoE radius 4 com stun 1s."
        },
        S{ id:1046, name:"Frost Nova",       prof:"Wand",  tier:3, is_passive:false, path:"frost",    unlock_char:t3c, unlock_prof:t3p,
            usable_with: caster_v.clone(), cost_mp:50, cost_st:0, cd:15.0, cast:0.0, target:"aoe_circle",
            range_t:0.0, radius:3.0, base_dmg:20, base_heal:0,
            scal_atk:0.0, scal_wis:1.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Self-radius 3; freeze 1s + 100% wis dmg."
        },
        S{ id:1047, name:"Inferno",          prof:"Wand",  tier:4, is_passive:false, path:"pyro",     unlock_char:t4c, unlock_prof:t4p,
            usable_with: caster_v.clone(), cost_mp:150, cost_st:0, cd:45.0, cast:0.5, target:"aoe_circle",
            range_t:6.0, radius:6.0, base_dmg:10, base_heal:0,
            scal_atk:0.0, scal_wis:0.5, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Sustained AoE radius 6, 5s; tick de 50% wis dmg/s."
        },
        S{ id:1048, name:"Elemental Mastery",prof:"Wand",  tier:4, is_passive:true,  path:"frost",    unlock_char:t4c, unlock_prof:t4p,
            usable_with: caster_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.03, rank_cd:0.0, rank_cost:0.0,
            desc:"+3%/rank dmg em alvos slowed/burned. r5: stack 2x. r10: dispel barato."
        },

        // ── STAFF (1049..1056) — Restoration (Re) + Storm (St) ───────────────
        S{ id:1049, name:"Lesser Heal",      prof:"Staff", tier:1, is_passive:false, path:"restoration",unlock_char:t1c, unlock_prof:t1p,
            usable_with: none.clone(), cost_mp:25, cost_st:0, cd:6.0, cast:0.4, target:"self",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:30,
            scal_atk:0.0, scal_wis:0.5, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Self heal ~25% HP. Universal — qualquer arma pode usar."
        },
        S{ id:1050, name:"Mana Conduit",     prof:"Staff", tier:1, is_passive:true,  path:"restoration",unlock_char:t1c, unlock_prof:t1p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.0, rank_cost:0.01,
            desc:"-1%/rank mp cost. r5: -CD. r10: cura crit dá overheal shield."
        },
        S{ id:1051, name:"Lightning Bolt",   prof:"Staff", tier:2, is_passive:false, path:"storm",    unlock_char:t2c, unlock_prof:t2p,
            usable_with: caster_v.clone(), cost_mp:30, cost_st:0, cd:5.0, cast:0.0, target:"line",
            range_t:8.0, radius:0.0, base_dmg:25, base_heal:0,
            scal_atk:0.0, scal_wis:1.5, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Single-target instant 150% wis."
        },
        S{ id:1052, name:"Healing Touch",    prof:"Staff", tier:2, is_passive:true,  path:"restoration",unlock_char:t2c, unlock_prof:t2p,
            usable_with: none.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.01, rank_cd:0.0, rank_cost:0.0,
            desc:"Auto-attacks healam self 1%/rank dmg dealt. r5: aplica em ally próximo. r10: crit dobra."
        },
        S{ id:1053, name:"Group Heal",       prof:"Staff", tier:3, is_passive:false, path:"restoration",unlock_char:t3c, unlock_prof:t3p,
            usable_with: none.clone(), cost_mp:70, cost_st:0, cd:18.0, cast:0.5, target:"aoe_circle",
            range_t:5.0, radius:5.0, base_dmg:0, base_heal:50,
            scal_atk:0.0, scal_wis:0.8, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Radius 5; +30% HP self+allies."
        },
        S{ id:1054, name:"Chain Lightning",  prof:"Staff", tier:3, is_passive:false, path:"storm",    unlock_char:t3c, unlock_prof:t3p,
            usable_with: caster_v.clone(), cost_mp:60, cost_st:0, cd:12.0, cast:0.3, target:"projectile",
            range_t:8.0, radius:0.0, base_dmg:20, base_heal:0,
            scal_atk:0.0, scal_wis:1.2, scal_dex:0.0, rank_dmg:0.08, rank_cd:0.0, rank_cost:0.0,
            desc:"Bounce em 4 alvos; dmg falloff 25% por bounce."
        },
        S{ id:1055, name:"Resurrection",     prof:"Staff", tier:4, is_passive:false, path:"restoration",unlock_char:t4c, unlock_prof:t4p,
            usable_with: none.clone(), cost_mp:200, cost_st:0, cd:120.0, cast:2.0, target:"line",
            range_t:5.0, radius:0.0, base_dmg:0, base_heal:0,
            scal_atk:0.0, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.0, rank_cd:0.0, rank_cost:0.0,
            desc:"Revive ally downed a 50% HP."
        },
        S{ id:1056, name:"Storm Caller",     prof:"Staff", tier:4, is_passive:true,  path:"storm",    unlock_char:t4c, unlock_prof:t4p,
            usable_with: caster_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.05, rank_cd:0.05, rank_cost:0.0,
            desc:"Em combate: auto-cast lightning a cada 4s. r5: chain bounces. r10: crit."
        },

        // ── UNARMED (1057..1064) — Combo (Co) + Defender (Df) ────────────────
        S{ id:1057, name:"Combo Strike",     prof:"Unarmed",tier:1, is_passive:false, path:"combo",   unlock_char:t1c, unlock_prof:t1p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:18, cd:5.0, cast:0.0, target:"cone",
            range_t:1.5, radius:0.0, base_dmg:6, base_heal:0,
            scal_atk:0.4, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"5-hit barrage; último hit knockback."
        },
        S{ id:1058, name:"Hardened Fists",   prof:"Unarmed",tier:1, is_passive:true,  path:"combo",   unlock_char:t1c, unlock_prof:t1p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.02, rank_cd:0.0, rank_cost:0.0,
            desc:"+2/rank atk dmg unarmed. r5: +crit. r10: hits ignoram 30% def."
        },
        S{ id:1059, name:"Knockout Punch",   prof:"Unarmed",tier:2, is_passive:false, path:"defender",unlock_char:t2c, unlock_prof:t2p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:25, cd:12.0, cast:0.0, target:"cone",
            range_t:1.4, radius:0.0, base_dmg:18, base_heal:0,
            scal_atk:1.5, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Heavy 1-hit + stun 1.5s."
        },
        S{ id:1060, name:"Iron Body",        prof:"Unarmed",tier:2, is_passive:true,  path:"defender",unlock_char:t2c, unlock_prof:t2p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.0, rank_cost:0.0,
            desc:"Sem arma: +1/rank def. r5: -dmg de DoT. r10: parry com mãos."
        },
        S{ id:1061, name:"Hurricane Kick",   prof:"Unarmed",tier:3, is_passive:false, path:"combo",   unlock_char:t3c, unlock_prof:t3p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:40, cd:14.0, cast:0.0, target:"aoe_circle",
            range_t:0.0, radius:1.8, base_dmg:8, base_heal:0,
            scal_atk:0.6, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"AoE 360 raio 1.8; 3 hits."
        },
        S{ id:1062, name:"Body Throw",       prof:"Unarmed",tier:3, is_passive:false, path:"defender",unlock_char:t3c, unlock_prof:t3p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:30, cd:16.0, cast:0.0, target:"cone",
            range_t:1.2, radius:0.0, base_dmg:10, base_heal:0,
            scal_atk:0.8, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Grab + throw 3 tiles back; stun 1s."
        },
        S{ id:1063, name:"Flying Kick",      prof:"Unarmed",tier:4, is_passive:false, path:"combo",   unlock_char:t4c, unlock_prof:t4p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:50, cd:20.0, cast:0.0, target:"line",
            range_t:5.0, radius:0.0, base_dmg:16, base_heal:0,
            scal_atk:1.4, scal_wis:0.0, scal_dex:0.0, rank_dmg:0.10, rank_cd:0.0, rank_cost:0.0,
            desc:"Dash 5 tiles + dmg + knockback."
        },
        S{ id:1064, name:"Master's Form",    prof:"Unarmed",tier:4, is_passive:true,  path:"defender",unlock_char:t4c, unlock_prof:t4p,
            usable_with: unarmed_v.clone(), cost_mp:0, cost_st:0, cd:0.0, cast:0.0, target:"none",
            range_t:0.0, radius:0.0, base_dmg:0, base_heal:0, scal_atk:0.0, scal_wis:0.0, scal_dex:0.0,
            rank_dmg:0.0, rank_cd:0.0, rank_cost:0.0,
            desc:"+parry window +block reduction unarmed. r5: parry recovers stam. r10: counter-stun automático."
        },
    ];

    let mut inserted = 0usize;
    for s in &seed {
        let usable: Option<Vec<String>> = s.usable_with.as_ref()
            .map(|v| v.iter().map(|x| x.to_string()).collect());
        sqlx::query(
            "INSERT INTO skills
              (id, name, description, prof, tier, is_passive, path,
               unlock_char_lvl, unlock_prof_lvl, usable_with,
               cost_mp, cost_stamina, cooldown_s, cast_time_s,
               target_type, range_tiles, radius_tiles,
               base_damage, base_heal, scaling_atk, scaling_wis, scaling_dex,
               per_rank_dmg_pct, per_rank_cd_pct, per_rank_cost_pct, active)
             VALUES
              ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
               $18,$19,$20,$21,$22,$23,$24,$25,TRUE)
             ON CONFLICT (id) DO UPDATE SET
               name        = EXCLUDED.name,
               description = EXCLUDED.description,
               prof        = EXCLUDED.prof,
               tier        = EXCLUDED.tier,
               is_passive  = EXCLUDED.is_passive,
               path        = EXCLUDED.path,
               unlock_char_lvl = EXCLUDED.unlock_char_lvl,
               unlock_prof_lvl = EXCLUDED.unlock_prof_lvl,
               usable_with = EXCLUDED.usable_with,
               cost_mp     = EXCLUDED.cost_mp,
               cost_stamina= EXCLUDED.cost_stamina,
               cooldown_s  = EXCLUDED.cooldown_s,
               cast_time_s = EXCLUDED.cast_time_s,
               target_type = EXCLUDED.target_type,
               range_tiles = EXCLUDED.range_tiles,
               radius_tiles= EXCLUDED.radius_tiles,
               base_damage = EXCLUDED.base_damage,
               base_heal   = EXCLUDED.base_heal,
               scaling_atk = EXCLUDED.scaling_atk,
               scaling_wis = EXCLUDED.scaling_wis,
               scaling_dex = EXCLUDED.scaling_dex,
               per_rank_dmg_pct  = EXCLUDED.per_rank_dmg_pct,
               per_rank_cd_pct   = EXCLUDED.per_rank_cd_pct,
               per_rank_cost_pct = EXCLUDED.per_rank_cost_pct
             WHERE skills.description = ''"
        )
        .bind(s.id).bind(s.name).bind(s.desc).bind(s.prof).bind(s.tier).bind(s.is_passive).bind(s.path)
        .bind(s.unlock_char).bind(s.unlock_prof).bind(usable)
        .bind(s.cost_mp).bind(s.cost_st).bind(s.cd).bind(s.cast)
        .bind(s.target).bind(s.range_t).bind(s.radius)
        .bind(s.base_dmg).bind(s.base_heal).bind(s.scal_atk).bind(s.scal_wis).bind(s.scal_dex)
        .bind(s.rank_dmg).bind(s.rank_cd).bind(s.rank_cost)
        .execute(pool).await?;
        inserted += 1;
    }
    tracing::info!("seed_skills_if_needed: {} skills inseridas/atualizadas", inserted);
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
        S{ id: item_id::GOLD as i32,           name:"Ouro",            sell:0,   buy:None,         ord:None,    stack:9999, slot:None, lvl:1, ic:15, ir:9,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::HEALTH_POTION as i32,  name:"Poção de Vida",   sell:5,   buy:Some(10),     ord:Some(0), stack:20,   slot:None, lvl:1, ic:3,  ir:17,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::MANA_POTION as i32,    name:"Poção de Mana",   sell:7,   buy:Some(15),     ord:Some(1), stack:20,   slot:None, lvl:1, ic:10, ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GREATER_HEAL as i32,   name:"Vida Maior",      sell:20,  buy:Some(40),     ord:Some(2), stack:20,   slot:None, lvl:1, ic:11, ir:17,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GREATER_MANA as i32,   name:"Mana Maior",      sell:25,  buy:Some(50),     ord:Some(3), stack:20,   slot:None, lvl:1, ic:9,  ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::STAMINA_POTION as i32, name:"Poção Stamina",   sell:10,  buy:Some(20),     ord:Some(4), stack:20,   slot:None, lvl:1, ic:8,  ir:7,   hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::GEM as i32,            name:"Gema",            sell:50,  buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:15, ir:10,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::IRON_INGOT as i32,     name:"Lingote de Ferro",sell:30,  buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:10, ir:13,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        S{ id: item_id::DRAGON_SCALE as i32,   name:"Escama de Dragão",sell:200, buy:None,         ord:None,    stack:99,   slot:None, lvl:1, ic:12, ir:12,  hp:(0,0),  mp:(0,0),    atk:(0,0),   def:(0,0), dex:(0,0), wis:(0,0) },
        // Armas
        S{ id: item_id::SWORD as i32,          name:"Espada",          sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:5,  ic:1,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(8,16),  def:(0,0),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::DAGGER as i32,         name:"Adaga",           sell:40,  buy:Some(80),     ord:Some(5), stack:1,    slot:Some("Weapon"), lvl:5,  ic:0,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(5,12),  def:(0,0),  dex:(6,14), wis:(0,0) },
        S{ id: item_id::GREAT_SWORD as i32,    name:"Espadão",         sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:10, ic:7,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(20,36), def:(0,0),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BOW as i32,            name:"Arco",            sell:75,  buy:Some(150),    ord:Some(7), stack:1,    slot:Some("Weapon"), lvl:8,  ic:13, ir:93,  hp:(0,0),    mp:(0,0),     atk:(10,20), def:(0,0),  dex:(8,16), wis:(0,0) },
        S{ id: item_id::STAFF as i32,          name:"Cajado",          sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:6,  ic:3,  ir:93,  hp:(0,0),    mp:(20,60),   atk:(14,26), def:(0,0),  dex:(0,0),  wis:(3,8) },
        S{ id: item_id::WAND as i32,           name:"Varinha",         sell:60,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:7,  ic:1,  ir:93,  hp:(0,0),    mp:(50,110),  atk:(4,10),  def:(0,0),  dex:(0,0),  wis:(5,12) },
        S{ id: item_id::SCIMITAR as i32,       name:"Cimitarra",       sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:6,  ic:3,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(6,14),  def:(0,0),  dex:(4,10), wis:(0,0) },
        S{ id: item_id::AXE as i32,         name:"Martelo de Guerra",sell:90, buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:12, ic:5,  ir:90,  hp:(15,35),  mp:(0,0),     atk:(16,32), def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::SPEAR as i32,          name:"Lança",           sell:65,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:9,  ic:8,  ir:90,  hp:(0,0),    mp:(0,0),     atk:(12,22), def:(0,0),  dex:(3,9),  wis:(0,0) },
        S{ id: item_id::CROSSBOW as i32,       name:"Besta",           sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Weapon"), lvl:10, ic:15, ir:93,  hp:(0,0),    mp:(0,0),     atk:(14,24), def:(0,0),  dex:(6,12), wis:(0,0) },
        // Armaduras / escudos
        S{ id: item_id::ARMOR as i32,          name:"Armadura",        sell:25,  buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:5,  ic:0,  ir:120, hp:(25,60),  mp:(0,0),     atk:(0,0),   def:(3,8),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::SHIELD as i32,         name:"Escudo",          sell:25,  buy:Some(50),     ord:Some(9), stack:1,    slot:Some("Offhand"),lvl:5,  ic:6,  ir:132, hp:(50,100), mp:(0,0),     atk:(0,0),   def:(5,12), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::LEATHER_ARMOR as i32,  name:"Couro",           sell:60,  buy:Some(120),    ord:Some(6), stack:1,    slot:Some("Armor"),  lvl:7,  ic:2,  ir:120, hp:(15,35),  mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(3,9),  wis:(0,0) },
        S{ id: item_id::PLATE_ARMOR as i32,    name:"Placa",           sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:12, ic:12, ir:120, hp:(80,160), mp:(0,0),     atk:(0,0),   def:(8,16), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::ROBE as i32,           name:"Manto",           sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Armor"),  lvl:7,  ic:0,  ir:123, hp:(5,15),   mp:(30,90),   atk:(0,0),   def:(1,4),  dex:(0,0),  wis:(4,12) },
        S{ id: item_id::HEAVY_SHIELD as i32,   name:"Escudo Pesado",   sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Offhand"),lvl:12, ic:8,  ir:132, hp:(70,140), mp:(0,0),     atk:(0,0),   def:(10,20),dex:(0,0),  wis:(0,0) },
        // Acessórios
        S{ id: item_id::RING as i32,           name:"Anel",            sell:20,  buy:None,         ord:None,    stack:1,    slot:Some("Ring"),    lvl:5,  ic:12, ir:128, hp:(0,0),    mp:(0,0),     atk:(0,0),   def:(0,0),  dex:(2,8),  wis:(1,5) },
        S{ id: item_id::AMULET as i32,         name:"Amuleto",         sell:90,  buy:Some(180),    ord:Some(8), stack:1,    slot:Some("Necklace"),lvl:7,  ic:0,  ir:129, hp:(8,25),   mp:(15,45),   atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(4,12) },
        S{ id: item_id::LUCKY_RING as i32,     name:"Anel da Sorte",   sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Ring"),    lvl:8,  ic:13, ir:128, hp:(5,18),   mp:(10,30),   atk:(1,4),   def:(0,0),  dex:(3,9),  wis:(1,4) },
        S{ id: item_id::PENDANT as i32,        name:"Pingente",        sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:9,  ic:2,  ir:129, hp:(15,35),  mp:(20,50),   atk:(0,0),   def:(0,2),  dex:(0,0),  wis:(2,6) },
        S{ id: item_id::CHARM as i32,          name:"Amuleto Místico", sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:10, ic:4,  ir:129, hp:(0,0),    mp:(5,20),    atk:(2,6),   def:(0,0),  dex:(2,6),  wis:(2,6) },
        // Phase E — slots novos
        S{ id: item_id::HELM_LEATHER as i32,   name:"Capacete de Couro",sell:40, buy:None,         ord:None,    stack:1,    slot:Some("Helm"),   lvl:6,  ic:0,  ir:116, hp:(8,22),   mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(2,6),  wis:(0,0) },
        S{ id: item_id::HELM_PLATE as i32,     name:"Elmo de Placa",   sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Helm"),   lvl:11, ic:4,  ir:116, hp:(30,60),  mp:(0,0),     atk:(0,0),   def:(4,10), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::LEGS_LEATHER as i32,   name:"Calças de Couro", sell:45,  buy:None,         ord:None,    stack:1,    slot:Some("Legs"),   lvl:6,  ic:0,  ir:124, hp:(12,28),  mp:(0,0),     atk:(0,0),   def:(1,5),  dex:(3,7),  wis:(0,0) },
        S{ id: item_id::LEGS_PLATE as i32,     name:"Calças de Placa", sell:90,  buy:None,         ord:None,    stack:1,    slot:Some("Legs"),   lvl:11, ic:4,  ir:124, hp:(40,80),  mp:(0,0),     atk:(0,0),   def:(6,12), dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BOOTS_LEATHER as i32,  name:"Botas de Couro",  sell:35,  buy:None,         ord:None,    stack:1,    slot:Some("Boots"),  lvl:5,  ic:0,  ir:126, hp:(5,15),   mp:(0,0),     atk:(0,0),   def:(0,3),  dex:(4,8),  wis:(0,0) },
        S{ id: item_id::BOOTS_PLATE as i32,    name:"Botas de Placa",  sell:70,  buy:None,         ord:None,    stack:1,    slot:Some("Boots"),  lvl:10, ic:4,  ir:126, hp:(20,40),  mp:(0,0),     atk:(0,0),   def:(3,7),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::GLOVES_LEATHER as i32, name:"Luvas de Couro",  sell:30,  buy:None,         ord:None,    stack:1,    slot:Some("Gloves"), lvl:5,  ic:0,  ir:122, hp:(0,0),    mp:(0,0),     atk:(1,5),   def:(0,2),  dex:(3,7),  wis:(0,0) },
        S{ id: item_id::GLOVES_PLATE as i32,   name:"Manoplas",        sell:65,  buy:None,         ord:None,    stack:1,    slot:Some("Gloves"), lvl:10, ic:4,  ir:122, hp:(12,28),  mp:(0,0),     atk:(3,8),   def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BELT_BASIC as i32,     name:"Cinto",           sell:35,  buy:None,         ord:None,    stack:1,    slot:Some("Belt"),   lvl:5,  ic:0,  ir:130, hp:(15,35),  mp:(0,0),     atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::BELT_MAGIC as i32,     name:"Faixa Mágica",    sell:75,  buy:None,         ord:None,    stack:1,    slot:Some("Belt"),   lvl:9,  ic:4,  ir:130, hp:(0,0),    mp:(20,50),   atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(2,6) },
        S{ id: item_id::CAPE_BASIC as i32,     name:"Capa",            sell:40,  buy:None,         ord:None,    stack:1,    slot:Some("Cape"),   lvl:6,  ic:0,  ir:134, hp:(12,28),  mp:(0,0),     atk:(0,0),   def:(2,6),  dex:(0,0),  wis:(0,0) },
        S{ id: item_id::CAPE_MAGIC as i32,     name:"Manto Mágico",    sell:80,  buy:None,         ord:None,    stack:1,    slot:Some("Cape"),   lvl:10, ic:4,  ir:134, hp:(0,0),    mp:(25,60),   atk:(0,0),   def:(0,3),  dex:(0,0),  wis:(3,9) },
        S{ id: item_id::NECKLACE_BASIC as i32, name:"Colar",           sell:50,  buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:7,  ic:6,  ir:129, hp:(12,28),  mp:(5,15),    atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(2,5) },
        S{ id: item_id::NECKLACE_MAGIC as i32, name:"Colar Mágico",    sell:100, buy:None,         ord:None,    stack:1,    slot:Some("Necklace"),lvl:11, ic:8,  ir:129, hp:(0,0),    mp:(25,55),   atk:(0,0),   def:(0,0),  dex:(0,0),  wis:(4,10) },
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
    ];
    let mut inserted = 0usize;
    for (kind, item, qmin, qmax, chance) in new_drops {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM loot_drops WHERE enemy_kind = $1 AND item_id = $2"
        )
        .bind(kind).bind(*item as i32)
        .fetch_one(pool).await?;
        if exists == 0 {
            sqlx::query(
                "INSERT INTO loot_drops (enemy_kind, item_id, qty_min, qty_max, chance) \
                 VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(kind).bind(*item as i32).bind(qmin).bind(qmax).bind(chance)
            .execute(pool).await?;
            inserted += 1;
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
            (1, "Mercador", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION, item_id::DAGGER,
                item_id::LEATHER_ARMOR, item_id::BOW, item_id::AMULET, item_id::SHIELD,
            ]),
            (2, "Espadeiro", &[
                item_id::SWORD, item_id::DAGGER, item_id::GREAT_SWORD,
            ]),
            (3, "Alquimista", &[
                item_id::HEALTH_POTION, item_id::MANA_POTION, item_id::GREATER_HEAL,
                item_id::GREATER_MANA, item_id::STAMINA_POTION,
            ]),
            (4, "Ferreiro", &[
                item_id::ARMOR, item_id::SHIELD, item_id::LEATHER_ARMOR,
                item_id::PLATE_ARMOR,
            ]),
            (5, "Mística", &[
                item_id::WAND, item_id::STAFF, item_id::ROBE, item_id::AMULET,
                item_id::LUCKY_RING, item_id::RING,
            ]),
            (6, "Arqueiro", &[
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

    Ok(())
}

pub async fn load_all(pool: &PgPool) -> Result<HashMap<String, CharacterRow>> {
    let rows = sqlx::query_as::<_, (String, f32, f32, i32, i32, i64, i64, i64, i32, Vec<i32>, i32, i32)>(
        "SELECT name, x, y, hp, max_hp, xp, fame, aura, unspent_points, allocated_points, \
                skill_points_earned, skill_points_spent FROM characters",
    )
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for (name, x, y, hp, max_hp, xp, fame, aura, unspent, allocated_vec, sp_earned, sp_spent) in rows {
        let inv = load_inventory(pool, &name).await?;
        let equip = load_equipment(pool, &name).await?;
        let vault = load_vault(pool, &name).await?;
        let profs = load_proficiencies(pool, &name).await?;
        let learned = load_learned_skills(pool, &name).await?;
        let mut allocated = [0u32; shared::STAT_COUNT];
        for (i, v) in allocated_vec.into_iter().enumerate().take(shared::STAT_COUNT) {
            allocated[i] = v.max(0) as u32;
        }
        out.insert(
            name.clone(),
            CharacterRow {
                name,
                pos: Vec2::new(x, y),
                hp: Health { current: hp, max: max_hp },
                xp: xp.max(0) as u64,
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
                learned_skills: learned,
            },
        );
    }
    Ok(out)
}

/// Carrega lista de skills aprendidas pelo personagem. Retorna Vec vazio
/// se não tem skills (player novo).
async fn load_learned_skills(pool: &PgPool, char_name: &str) -> Result<Vec<shared::LearnedSkill>> {
    let rows = sqlx::query_as::<_, (i32, i16, Option<i16>)>(
        "SELECT skill_id, rank, equipped_slot FROM player_skills \
         WHERE character_name = $1 ORDER BY skill_id",
    )
    .bind(char_name)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (sid, rank, slot) in rows {
        if sid < 0 || rank <= 0 { continue; }
        out.push(shared::LearnedSkill {
            skill_id: sid as u32,
            rank: rank.clamp(0, shared::MAX_SKILL_RANK as i16) as u8,
            equipped_slot: slot.and_then(|s| if (0..shared::SKILL_BAR_SLOTS as i16).contains(&s) {
                Some(s as u8)
            } else { None }),
        });
    }
    Ok(out)
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
            "necklace" => { eq.necklace = Some(iid); eq.necklace_inst = inst; }
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
        sqlx::query(
            "INSERT INTO characters (name, x, y, hp, max_hp, xp, fame, aura,
                                     unspent_points, allocated_points,
                                     skill_points_earned, skill_points_spent, updated)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
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
               updated = EXCLUDED.updated",
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
        .bind(now)
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
            ("necklace", row.equipment.necklace, row.equipment.necklace_inst),
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
        // characters limpa órfãos automaticamente quando deletam o char.
        sqlx::query("DELETE FROM player_skills WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for ls in &row.learned_skills {
            if ls.rank == 0 || ls.rank > shared::MAX_SKILL_RANK { continue; }
            sqlx::query(
                "INSERT INTO player_skills (character_name, skill_id, rank, equipped_slot)
                 VALUES ($1, $2, $3, $4)"
            )
            .bind(&row.name)
            .bind(ls.skill_id as i32)
            .bind(ls.rank as i16)
            .bind(ls.equipped_slot.map(|s| s as i16))
            .execute(&mut *tx)
            .await?;
        }

        // Vault: mesmo padrao do inventory.
        sqlx::query("DELETE FROM vault WHERE character_name = $1")
            .bind(&row.name)
            .execute(&mut *tx)
            .await?;
        for (i, slot) in row.vault.iter().enumerate() {
            if slot.qty == 0 { continue; }
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
    }
    tx.commit().await?;
    Ok(())
}
