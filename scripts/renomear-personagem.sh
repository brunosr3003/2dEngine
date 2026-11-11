#!/usr/bin/env bash
# Renames a character in PROD in one transaction per database (copy row with
# the new name, point every table at it, delete the old one; FKs have no ON
# UPDATE CASCADE). Run ONLY with the tempest-prod-* units STOPPED: each game
# process keeps the character list in memory (F1CARP came back as a ghost on
# 11/11/2026 when this ran with the servers up).
#   ssh mmo-vps 'bash -s' < scripts/renomear-personagem.sh VELHO NOVO
set -euo pipefail
VELHO="$1"; NOVO="$2"
[[ "$VELHO" =~ ^[A-Za-z0-9_]+$ && "$NOVO" =~ ^[A-Za-z0-9_]+$ ]] || { echo "nomes invalidos"; exit 1; }
if systemctl list-units "tempest-prod-*" --no-legend --plain | grep -q running; then
  echo "pare as unidades tempest-prod-* antes"; exit 1
fi
B=/opt/tempest-prod/backups/renomeia-$VELHO-$(date -u +%Y%m%d%H%M); mkdir -p "$B"
docker exec tempest-prod-pg pg_dump -U solar -Fc tempest_sa01 > "$B/tempest_sa01.dump"
docker exec tempest-prod-pg pg_dump -U solar -Fc tempest_central > "$B/tempest_central.dump"
docker exec -i tempest-prod-pg psql -U solar -d tempest_sa01 <<SQL
\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE novo AS SELECT * FROM characters WHERE name = '$VELHO';
DO \$\$ BEGIN IF (SELECT count(*) FROM novo) <> 1 THEN RAISE EXCEPTION 'personagem nao encontrado'; END IF;
  IF EXISTS (SELECT 1 FROM characters WHERE lower(name) = lower('$NOVO')) THEN RAISE EXCEPTION 'nome novo ja existe'; END IF; END \$\$;
UPDATE novo SET name = '$NOVO';
INSERT INTO characters SELECT * FROM novo;
UPDATE character_quests SET char_name = '$NOVO' WHERE char_name = '$VELHO';
UPDATE equipment SET character_name = '$NOVO' WHERE character_name = '$VELHO';
UPDATE inventory SET character_name = '$NOVO' WHERE character_name = '$VELHO';
UPDATE player_skills SET character_name = '$NOVO' WHERE character_name = '$VELHO';
UPDATE proficiencies SET character_name = '$NOVO' WHERE character_name = '$VELHO';
UPDATE vault SET character_name = '$NOVO' WHERE character_name = '$VELHO';
UPDATE social_amigos SET de = '$NOVO' WHERE de = '$VELHO';
UPDATE social_amigos SET para = '$NOVO' WHERE para = '$VELHO';
UPDATE social_cartas SET de = '$NOVO' WHERE de = '$VELHO';
UPDATE social_cartas SET para = '$NOVO' WHERE para = '$VELHO';
UPDATE social_clas SET lider = '$NOVO' WHERE lider = '$VELHO';
UPDATE social_convites SET nome = '$NOVO' WHERE nome = '$VELHO';
UPDATE social_convites SET de = '$NOVO' WHERE de = '$VELHO';
UPDATE social_envios SET nome = '$NOVO' WHERE nome = '$VELHO';
UPDATE social_membros SET nome = '$NOVO' WHERE nome = '$VELHO';
UPDATE social_oficiais SET para = '$NOVO' WHERE para = '$VELHO';
UPDATE social_campanhas SET destino = '$NOVO' WHERE destino = '$VELHO';
UPDATE hud_layouts SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_cartas_aplicadas SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_saida SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE presenca_resgates SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE telemetria_personagem SET personagem = '$NOVO' WHERE personagem = '$VELHO';
DELETE FROM characters WHERE name = '$VELHO';
SELECT name, xp, account_id FROM characters WHERE name IN ('$VELHO', '$NOVO') ORDER BY name;
SELECT 'inv', count(*) FROM inventory WHERE character_name = '$NOVO' UNION ALL SELECT 'equip', count(*) FROM equipment WHERE character_name = '$NOVO';
COMMIT;
SQL
docker exec -i tempest-prod-pg psql -U solar -d tempest_central <<SQL
\set ON_ERROR_STOP on
BEGIN;
UPDATE loja_entregas SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_anuncios SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_cartas SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_vagas SET personagem = '$NOVO' WHERE personagem = '$VELHO';
UPDATE mercado_vendas SET vendedor = '$NOVO' WHERE vendedor = '$VELHO';
UPDATE mercado_vendas SET comprador = '$NOVO' WHERE comprador = '$VELHO';
COMMIT;
SQL
echo "backup em $B"
