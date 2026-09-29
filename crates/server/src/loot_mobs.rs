//! Base loot for the eight types: copper, materials and few potions.
//! Each entry rolls separately; equipment is never part of the pool.
use shared::item_id::*;

// (tipo, item, minimo, maximo, chance)
pub const BASE: &[(i32, u16, i32, i32, f32)] = &[
    (0, COPPER, 4, 14, 1.0),
    (0, HEALTH_POTION, 1, 1, 0.08),
    (0, MANA_POTION, 1, 1, 0.04),
    (1, COPPER, 15, 40, 1.0),
    (1, STEEL, 1, 3, 0.25),
    (1, HEALTH_POTION, 1, 1, 0.12),
    (2, COPPER, 10, 28, 1.0),
    (2, STEEL, 1, 3, 0.25),
    (2, MOON_SHADOW_STONE, 1, 2, 0.12),
    (2, STAMINA_POTION, 1, 1, 0.08),
    (3, COPPER, 8, 22, 1.0),
    (3, QUINTESSENCE, 1, 2, 0.20),
    (3, MANA_POTION, 1, 1, 0.08),
    (4, COPPER, 15, 35, 1.0),
    (4, DARK_HEART_STONE, 1, 2, 0.20),
    (4, ANIMA_STONE, 1, 2, 0.20),
    (4, MANA_POTION, 1, 1, 0.12),
    (5, COPPER, 25, 60, 1.0),
    (5, PLATINUM, 1, 3, 0.25),
    (5, EXORCISM_BAUBLE, 1, 2, 0.15),
    (5, GREATER_HEAL, 1, 1, 0.10),
    (6, COPPER, 10, 28, 1.0),
    (6, ILLUMINATING_FRAGMENT, 1, 2, 0.20),
    (6, STEEL, 1, 2, 0.20),
    (6, STAMINA_POTION, 1, 1, 0.08),
    (7, COPPER, 200, 500, 1.0),
    (7, DARKSTEEL, 5, 12, 0.50),
    (7, GLITTERING_POWDER, 1, 1, 0.05),
    (7, GREATER_HEAL, 1, 2, 0.35),
    (7, GREATER_MANA, 1, 1, 0.25),
];

/// The beach crabs (kinds 8 and 9): copper and a potion; the king yields a
/// little steel. Migrated separately, with their own marker — the
/// `loot_mobs_recursos_v1` has already run on the databases that exist.
///
/// No mob gives a KEY (Scale, Claw, Horn, Hide): only bosses and
/// dungeon/raid (`shared::chaves`, migration `chaves_so_de_chefe_v1`).
pub const BASE_PRAIA: &[(i32, u16, i32, i32, f32)] = &[
    (8, COPPER, 3, 10, 1.0),
    (8, HEALTH_POTION, 1, 1, 0.08),
    (9, COPPER, 12, 30, 1.0),
    (9, STEEL, 1, 2, 0.15),
    (9, HEALTH_POTION, 1, 1, 0.12),
];

/// THE BESTIARY OF THE OTHER ISLANDS (kinds 10-15, `economy::kinds_do_bioma`).
///
/// Each island drops the material its tier asks for, and the copper rises
/// with the tier. None gives a KEY — keys are boss and dungeon only.
pub const BASE_ILHAS: &[(i32, u16, i32, i32, f32)] = &[
    // Geleira
    (10, COPPER, 30, 70, 1.0),
    (10, PLATINUM, 1, 3, 0.22),
    (10, GREATER_HEAL, 1, 1, 0.10),
    (11, COPPER, 28, 65, 1.0),
    (11, STEEL, 2, 5, 0.28),
    (11, ILLUMINATING_FRAGMENT, 1, 2, 0.18),
    (12, COPPER, 22, 55, 1.0),
    (12, QUINTESSENCE, 1, 3, 0.24),
    (12, GREATER_MANA, 1, 1, 0.10),
    // Ermo
    (13, COPPER, 26, 60, 1.0),
    (13, STEEL, 2, 4, 0.26),
    (13, EXORCISM_BAUBLE, 1, 2, 0.16),
    (14, COPPER, 70, 160, 1.0),
    (14, DARKSTEEL, 2, 6, 0.30),
    (14, ANIMA_STONE, 1, 3, 0.22),
    (14, GREATER_HEAL, 1, 2, 0.20),
    // Planalto
    (15, COPPER, 90, 200, 1.0),
    (15, DARKSTEEL, 3, 8, 0.34),
    (15, PLATINUM, 2, 5, 0.26),
    (15, GLITTERING_POWDER, 1, 1, 0.04),
];

/// Migrates only the mobs' economy, once, inside a transaction.
pub async fn migrar(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS economy_migrations (name TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())")
        .execute(&mut *tx).await?;
    let nova=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('loot_mobs_recursos_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if nova {
        sqlx::query("DELETE FROM loot_drops WHERE enemy_kind BETWEEN 0 AND 7 OR item_id BETWEEN 400 AND 414 OR item_id IN (SELECT id FROM items WHERE equip_slot IS NOT NULL AND equip_slot <> '')")
            .execute(&mut *tx).await?;
        for &(kind, item, min, max, chance) in BASE {
            sqlx::query("INSERT INTO loot_drops(enemy_kind,item_id,qty_min,qty_max,chance) SELECT $1,$2,$3,$4,$5 WHERE EXISTS(SELECT 1 FROM enemy_kinds WHERE kind=$1)")
                .bind(kind).bind(item as i32).bind(min).bind(max).bind(chance).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1")
            .execute(&mut *tx)
            .await?;
    }
    // Crabs: once, only if the kind already exists (the `enemy_kinds` seed runs
    // first). Deletes nothing: kinds 8 and 9 had no loot.
    let praia=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('loot_caranguejos_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if praia {
        for &(kind, item, min, max, chance) in BASE_PRAIA {
            sqlx::query("INSERT INTO loot_drops(enemy_kind,item_id,qty_min,qty_max,chance) SELECT $1,$2,$3,$4,$5 WHERE EXISTS(SELECT 1 FROM enemy_kinds WHERE kind=$1)")
                .bind(kind).bind(item as i32).bind(min).bind(max).bind(chance).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1")
            .execute(&mut *tx)
            .await?;
        tracing::info!("Crab loot seeded");
    }
    // The bestiary of the other islands: loot for kinds 10-15, and the cleanup
    // of the SEA kinds (20-24), orphaned when the Open Sea was undone. They no
    // longer spawn (the draw is by biome now), but they still showed up in
    // "where to get" and in the economy sums.
    let ilhas = sqlx::query(
        "INSERT INTO economy_migrations(name) VALUES ('bestiario_por_ilha_v1') ON CONFLICT DO NOTHING",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if ilhas {
        for &(kind, item, min, max, chance) in BASE_ILHAS {
            sqlx::query("INSERT INTO loot_drops(enemy_kind,item_id,qty_min,qty_max,chance) SELECT $1,$2,$3,$4,$5 WHERE EXISTS(SELECT 1 FROM enemy_kinds WHERE kind=$1)")
                .bind(kind).bind(item as i32).bind(min).bind(max).bind(chance).execute(&mut *tx).await?;
        }
        let orfaos = sqlx::query("DELETE FROM loot_drops WHERE enemy_kind BETWEEN 20 AND 24")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let kinds = sqlx::query("DELETE FROM enemy_kinds WHERE kind BETWEEN 20 AND 24")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1")
            .execute(&mut *tx)
            .await?;
        tracing::info!(
            "Per-island bestiary seeded; {kinds} sea kinds and {orfaos} loot rows removed"
        );
    }
    // Craft keys (Scale, Claw, Horn, Hide) from bosses and dungeon/raid only:
    // they come off every mob and every stone, in every color. A new database
    // is born without them (BASE and `linhas_da_pedra` have no keys).
    let chaves=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('chaves_so_de_chefe_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if chaves {
        let ids: Vec<i32> = shared::item_id::todas_as_chaves()
            .into_iter()
            .map(|i| i as i32)
            .collect();
        let mobs = sqlx::query("DELETE FROM loot_drops WHERE item_id = ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let tem_pedra: bool =
            sqlx::query_scalar("SELECT to_regclass('farm_node_drops') IS NOT NULL")
                .fetch_one(&mut *tx)
                .await?;
        let pedras = if tem_pedra {
            sqlx::query("DELETE FROM farm_node_drops WHERE item_id = ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await?
                .rows_affected()
        } else {
            0
        };
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1")
            .execute(&mut *tx)
            .await?;
        tracing::info!(
            "Crafting keys removed from {mobs} mob rows and {pedras} gathering rows: bosses only now"
        );
    }
    tx.commit().await?;
    if nova {
        tracing::info!("Mob loot updated: copper, materials and potions; no gear");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nenhum_mob_da_chave() {
        let chaves = todas_as_chaves();
        for e in BASE.iter().chain(BASE_PRAIA) {
            assert!(!chaves.contains(&e.1), "kind {} da' chave {}", e.0, e.1);
        }
    }
    #[test]
    fn caranguejos_tem_cobre_e_nenhum_equipamento() {
        for kind in [8, 9] {
            let entries: Vec<_> = BASE_PRAIA.iter().filter(|e| e.0 == kind).collect();
            assert_eq!(
                entries
                    .iter()
                    .filter(|e| e.1 == COPPER && e.4 == 1.0)
                    .count(),
                1
            );
            for e in entries {
                assert!(shared::equip_slot_of(e.1).is_none());
                assert!(e.2 > 0 && e.3 >= e.2 && e.4 > 0.0 && e.4 <= 1.0);
            }
        }
        // the king yields more than the small one
        let cobre = |k: i32| {
            BASE_PRAIA
                .iter()
                .find(|e| e.0 == k && e.1 == COPPER)
                .unwrap()
                .3
        };
        assert!(cobre(9) > cobre(8));
    }
    #[test]
    fn todos_tem_cobre_poucas_pocoes_e_nenhum_equipamento() {
        for kind in 0..=7 {
            let entries: Vec<_> = BASE.iter().filter(|e| e.0 == kind).collect();
            assert_eq!(
                entries
                    .iter()
                    .filter(|e| e.1 == COPPER && e.4 == 1.0)
                    .count(),
                1
            );
            assert!(entries.iter().any(|e| matches!(
                e.1,
                HEALTH_POTION | MANA_POTION | STAMINA_POTION | GREATER_HEAL | GREATER_MANA
            )));
            for e in entries {
                assert!(shared::equip_slot_of(e.1).is_none());
                assert_ne!(e.1, GOLD);
                assert!(e.2 > 0 && e.3 >= e.2 && e.4 > 0.0 && e.4 <= 1.0);
                if matches!(
                    e.1,
                    HEALTH_POTION | MANA_POTION | STAMINA_POTION | GREATER_HEAL | GREATER_MANA
                ) {
                    assert!(e.4 <= if kind == 7 { 0.35 } else { 0.12 });
                }
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// EVERY creature in the bestiary has loot.
    ///
    /// A creature with no row in `loot_drops` dies and drops nothing — not even
    /// copper. It is not an error and does not appear in the log: the player just
    /// thinks the island is poor. The test walks the real bestiary, and not a
    /// list written here.
    #[test]
    fn nenhum_bicho_do_bestiario_cai_sem_nada() {
        use shared::terreno::ARQUIPELAGO;
        let tem = |k: u16| {
            BASE.iter()
                .chain(BASE_PRAIA)
                .chain(BASE_ILHAS)
                .any(|(kk, ..)| *kk as u16 == k)
        };
        for d in ARQUIPELAGO.iter() {
            let b = crate::economy::kinds_do_bioma(d.bioma);
            let p = crate::economy::kinds_de_praia_do_bioma(d.bioma);
            for k in b.iter().chain(p) {
                assert!(
                    tem(*k),
                    "{}: o kind {k} ({}) nao tem loot nenhum",
                    d.nome,
                    crate::economy::kind_inicial(*k).map_or("?", |x| x.name)
                );
            }
        }
    }

    /// And every creature drops COPPER, with chance 1. Copper is the day-to-day
    /// currency (docs/ECONOMIA.md): a creature that sometimes pays nothing makes
    /// the whole island look broken.
    #[test]
    fn todo_bicho_paga_cobre_sempre() {
        for tabela in [BASE, BASE_PRAIA, BASE_ILHAS] {
            let kinds: std::collections::BTreeSet<i32> =
                tabela.iter().map(|(k, ..)| *k).collect();
            for k in kinds {
                let cobre = tabela
                    .iter()
                    .find(|(kk, item, ..)| *kk == k && *item == COPPER);
                let (.., chance) = cobre.unwrap_or_else(|| panic!("kind {k} sem cobre"));
                assert_eq!(*chance, 1.0, "kind {k}: cobre com chance {chance}");
            }
        }
    }

    /// No mob gives a KEY — bosses and dungeons only (`shared::chaves`). A key on
    /// a common mob brings the whole craft economy down.
    #[test]
    fn nenhum_mob_da_chave() {
        let chaves = shared::item_id::todas_as_chaves();
        for tabela in [BASE, BASE_PRAIA, BASE_ILHAS] {
            for (k, item, ..) in tabela {
                assert!(
                    !chaves.contains(item),
                    "kind {k} larga a chave {item}"
                );
            }
        }
    }
}
