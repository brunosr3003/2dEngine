//! Base de loot dos oito tipos: cobre, materiais e poucas pocoes.
//! Cada entrada rola separadamente; equipamento nunca faz parte do pool.
use shared::item_id::*;

// (tipo, item, minimo, maximo, chance)
pub const BASE: &[(i32,u16,i32,i32,f32)] = &[
    (0,COPPER,4,14,1.0), (0,HIDE,1,1,0.04), (0,CLAW,1,1,0.02),
    (0,HEALTH_POTION,1,1,0.08), (0,MANA_POTION,1,1,0.04),
    (1,COPPER,15,40,1.0), (1,STEEL,1,3,0.25), (1,HIDE,1,1,0.04), (1,HEALTH_POTION,1,1,0.12),
    (2,COPPER,10,28,1.0), (2,STEEL,1,3,0.25), (2,MOON_SHADOW_STONE,1,2,0.12), (2,STAMINA_POTION,1,1,0.08),
    (3,COPPER,8,22,1.0), (3,QUINTESSENCE,1,2,0.20), (3,CLAW,1,1,0.03), (3,MANA_POTION,1,1,0.08),
    (4,COPPER,15,35,1.0), (4,DARK_HEART_STONE,1,2,0.20), (4,ANIMA_STONE,1,2,0.20), (4,MANA_POTION,1,1,0.12),
    (5,COPPER,25,60,1.0), (5,PLATINUM,1,3,0.25), (5,EXORCISM_BAUBLE,1,2,0.15), (5,HORN,1,1,0.03), (5,GREATER_HEAL,1,1,0.10),
    (6,COPPER,10,28,1.0), (6,ILLUMINATING_FRAGMENT,1,2,0.20), (6,STEEL,1,2,0.20), (6,STAMINA_POTION,1,1,0.08),
    (7,COPPER,200,500,1.0), (7,DARKSTEEL,5,12,0.50), (7,GLITTERING_POWDER,1,1,0.05),
    (7,SCALE,1,1,0.05), (7,GREATER_HEAL,1,2,0.35), (7,GREATER_MANA,1,1,0.25),
];

/// Migra apenas a economia dos mobs, uma vez, dentro de uma transacao.
pub async fn migrar(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let mut tx=pool.begin().await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS economy_migrations (name TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())")
        .execute(&mut *tx).await?;
    let nova=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('loot_mobs_recursos_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if nova {
        sqlx::query("DELETE FROM loot_drops WHERE enemy_kind BETWEEN 0 AND 7 OR item_id BETWEEN 400 AND 414 OR item_id IN (SELECT id FROM items WHERE equip_slot IS NOT NULL AND equip_slot <> '')")
            .execute(&mut *tx).await?;
        for &(kind,item,min,max,chance) in BASE {
            sqlx::query("INSERT INTO loot_drops(enemy_kind,item_id,qty_min,qty_max,chance) SELECT $1,$2,$3,$4,$5 WHERE EXISTS(SELECT 1 FROM enemy_kinds WHERE kind=$1)")
                .bind(kind).bind(item as i32).bind(min).bind(max).bind(chance).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1").execute(&mut *tx).await?;
    }
    tx.commit().await?;
    if nova {tracing::info!("Loot dos mobs atualizado: cobre, materiais e pocoes; sem equipamentos");}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn todos_tem_cobre_poucas_pocoes_e_nenhum_equipamento() {
        for kind in 0..=7 {
            let entries:Vec<_>=BASE.iter().filter(|e|e.0==kind).collect();
            assert_eq!(entries.iter().filter(|e|e.1==COPPER && e.4==1.0).count(),1);
            assert!(entries.iter().any(|e|matches!(e.1,HEALTH_POTION|MANA_POTION|STAMINA_POTION|GREATER_HEAL|GREATER_MANA)));
            for e in entries {
                assert!(shared::equip_slot_of(e.1).is_none());assert_ne!(e.1,GOLD);
                assert!(e.2>0 && e.3>=e.2 && e.4>0.0 && e.4<=1.0);
                if matches!(e.1,HEALTH_POTION|MANA_POTION|STAMINA_POTION|GREATER_HEAL|GREATER_MANA) {
                    assert!(e.4<=if kind==7 {0.35} else {0.12});
                }
            }
        }
    }
}
