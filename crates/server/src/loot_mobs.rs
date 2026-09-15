//! Base de loot dos oito tipos: cobre, materiais e poucas pocoes.
//! Cada entrada rola separadamente; equipamento nunca faz parte do pool.
use shared::item_id::*;

// (tipo, item, minimo, maximo, chance)
pub const BASE: &[(i32,u16,i32,i32,f32)] = &[
    (0,COPPER,4,14,1.0),
    (0,HEALTH_POTION,1,1,0.08), (0,MANA_POTION,1,1,0.04),
    (1,COPPER,15,40,1.0), (1,STEEL,1,3,0.25), (1,HEALTH_POTION,1,1,0.12),
    (2,COPPER,10,28,1.0), (2,STEEL,1,3,0.25), (2,MOON_SHADOW_STONE,1,2,0.12), (2,STAMINA_POTION,1,1,0.08),
    (3,COPPER,8,22,1.0), (3,QUINTESSENCE,1,2,0.20), (3,MANA_POTION,1,1,0.08),
    (4,COPPER,15,35,1.0), (4,DARK_HEART_STONE,1,2,0.20), (4,ANIMA_STONE,1,2,0.20), (4,MANA_POTION,1,1,0.12),
    (5,COPPER,25,60,1.0), (5,PLATINUM,1,3,0.25), (5,EXORCISM_BAUBLE,1,2,0.15), (5,GREATER_HEAL,1,1,0.10),
    (6,COPPER,10,28,1.0), (6,ILLUMINATING_FRAGMENT,1,2,0.20), (6,STEEL,1,2,0.20), (6,STAMINA_POTION,1,1,0.08),
    (7,COPPER,200,500,1.0), (7,DARKSTEEL,5,12,0.50), (7,GLITTERING_POWDER,1,1,0.05),
    (7,GREATER_HEAL,1,2,0.35), (7,GREATER_MANA,1,1,0.25),
];

/// Os caranguejos de praia (kinds 8 e 9): cobre e pocao; o rei rende um pouco
/// de aco. Migrados a parte, com marcador proprio — o
/// `loot_mobs_recursos_v1` ja' rodou nos bancos que existem.
///
/// Nenhum mob da' CHAVE (Escama, Garra, Chifre, Couro): so' chefe e
/// dungeon/raid (`shared::chaves`, migracao `chaves_so_de_chefe_v1`).
pub const BASE_PRAIA: &[(i32,u16,i32,i32,f32)] = &[
    (8,COPPER,3,10,1.0), (8,HEALTH_POTION,1,1,0.08),
    (9,COPPER,12,30,1.0), (9,STEEL,1,2,0.15), (9,HEALTH_POTION,1,1,0.12),
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
    // Caranguejos: uma vez, so' se o kind ja' existe (o seed de `enemy_kinds`
    // roda antes). Nao apaga nada: kind 8 e 9 nao tinham loot.
    let praia=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('loot_caranguejos_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if praia {
        for &(kind,item,min,max,chance) in BASE_PRAIA {
            sqlx::query("INSERT INTO loot_drops(enemy_kind,item_id,qty_min,qty_max,chance) SELECT $1,$2,$3,$4,$5 WHERE EXISTS(SELECT 1 FROM enemy_kinds WHERE kind=$1)")
                .bind(kind).bind(item as i32).bind(min).bind(max).bind(chance).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1").execute(&mut *tx).await?;
        tracing::info!("Loot dos caranguejos semeado");
    }
    // Chaves de craft (Escama, Garra, Chifre, Couro) so' de chefe e
    // dungeon/raid: saem de todo mob e de toda pedra, em toda cor. Banco novo
    // ja' nasce sem (BASE e `linhas_da_pedra` nao tem chave).
    let chaves=sqlx::query("INSERT INTO economy_migrations(name) VALUES ('chaves_so_de_chefe_v1') ON CONFLICT DO NOTHING")
        .execute(&mut *tx).await?.rows_affected()>0;
    if chaves {
        let ids:Vec<i32>=shared::item_id::todas_as_chaves().into_iter().map(|i|i as i32).collect();
        let mobs=sqlx::query("DELETE FROM loot_drops WHERE item_id = ANY($1)").bind(&ids).execute(&mut *tx).await?.rows_affected();
        let tem_pedra:bool=sqlx::query_scalar("SELECT to_regclass('farm_node_drops') IS NOT NULL").fetch_one(&mut *tx).await?;
        let pedras=if tem_pedra {
            sqlx::query("DELETE FROM farm_node_drops WHERE item_id = ANY($1)").bind(&ids).execute(&mut *tx).await?.rows_affected()
        } else { 0 };
        sqlx::query("UPDATE economy_version SET version=version+1 WHERE id=1").execute(&mut *tx).await?;
        tracing::info!("Chaves de craft tiradas de {mobs} linhas de mob e {pedras} de coleta: agora so' chefe");
    }
    tx.commit().await?;
    if nova {tracing::info!("Loot dos mobs atualizado: cobre, materiais e pocoes; sem equipamentos");}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nenhum_mob_da_chave() {
        let chaves=todas_as_chaves();
        for e in BASE.iter().chain(BASE_PRAIA) {
            assert!(!chaves.contains(&e.1),"kind {} da' chave {}",e.0,e.1);
        }
    }
    #[test]
    fn caranguejos_tem_cobre_e_nenhum_equipamento() {
        for kind in [8, 9] {
            let entries:Vec<_>=BASE_PRAIA.iter().filter(|e|e.0==kind).collect();
            assert_eq!(entries.iter().filter(|e|e.1==COPPER && e.4==1.0).count(),1);
            for e in entries {
                assert!(shared::equip_slot_of(e.1).is_none());
                assert!(e.2>0 && e.3>=e.2 && e.4>0.0 && e.4<=1.0);
            }
        }
        // o rei rende mais que o pequeno
        let cobre=|k:i32| BASE_PRAIA.iter().find(|e|e.0==k && e.1==COPPER).unwrap().3;
        assert!(cobre(9)>cobre(8));
    }
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
