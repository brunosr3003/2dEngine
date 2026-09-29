#!/usr/bin/env bash
# Read-only: lists the seeded names still in Portuguese in the live database.
#
# After nomes_em_ingles_v1 these should be gone. Any row that comes back is a
# name the migration did not match — almost certainly because the row was
# first seeded under an older name, and the items upsert never updates `name`
# (ON CONFLICT (id) DO UPDATE sets slots, icons and stat ranges, not the
# name). Those names reach an English player untranslated.
#
#   bash tools/nomes_pt_no_banco.sh
#
# It reads DATABASE_URL out of the prod env file without sourcing it — the
# file has a line zsh refuses to parse — and never prints the URL. There is
# no psql on this host, so it runs through the `mmo-pg` container, which has
# one.
set -euo pipefail
cd "$(dirname "$0")/.."

ENVFILE="${TEMPEST_ENV:-$HOME/tempest-prod/tempest.env}"
[ -f "$ENVFILE" ] || { echo "no env file at $ENVFILE" >&2; exit 1; }

URL="$(sed -n 's/^DATABASE_URL=//p' "$ENVFILE" | head -1 | sed 's/^["'\'']//; s/["'\'']$//')"
[ -n "$URL" ] || { echo "DATABASE_URL not found in $ENVFILE" >&2; exit 1; }

PT="name ~ '[[:alpha:]]*[ãõçáéíóúâêôÃÕÇÁÉÍÓÚÂÊÔ]' \
 OR name ILIKE '% de %' OR name ILIKE '% da %' OR name ILIKE '% do %' \
 OR name ILIKE '% dos %' OR name ILIKE '% das %'"

for t in items enemy_kinds vendor_shops; do
  printf '\n== %s ==\n' "$t"
  docker exec -i mmo-pg psql "$URL" -Atc "SELECT count(*) FROM $t WHERE $PT" \
    | sed 's/^/  ainda em portugues: /'
  docker exec -i mmo-pg psql "$URL" -Atc "SELECT id, name FROM $t WHERE $PT ORDER BY 1 LIMIT 60" 2>/dev/null \
    || docker exec -i mmo-pg psql "$URL" -Atc "SELECT kind, name FROM $t WHERE $PT ORDER BY 1 LIMIT 60"
done

printf '\n== the migration row ==\n'
docker exec -i mmo-pg psql "$URL" -Atc \
  "SELECT name, applied_at FROM economy_migrations WHERE name = 'nomes_em_ingles_v1'"
