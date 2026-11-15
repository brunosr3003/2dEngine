#!/usr/bin/env bash
# Moves EVERY character back to the starting island (Bosque), at the spot a
# new character is born — for the world remap (docs/RESET.md, "Mundo novo",
# owner 15/11/2026: "voltar todo mundo pra ilha inicial e eles vão vendo onde
# aguentam upar"). Level, XP, story and items are untouched: a character
# whose story already reached another island can sail back from any Captain.
#
# Run it with the game STOPPED (all `tempest-*` units of that environment):
# a running process keeps its own copy of the characters and would write the
# old position back on its next save.
#
# Usage:
#   scripts/voltar-ilha-inicial.sh <postgres container> <realm database> [--sim]
#   e.g. scripts/voltar-ilha-inicial.sh tempest-teste-pg tempest_sa01 --sim
#   --sim prints what would change and changes nothing.
set -euo pipefail
pg="${1:?postgres container}"
db="${2:?realm database}"
sim="${3:-}"
psql() { docker exec -i "$pg" psql -U solar -d "$db" -v ON_ERROR_STOP=1 "$@"; }

# Where a new character is born: the newest character still on the Bosque
# with no XP (`persistence::criar_personagem` writes the island's spawn).
# Without one, the caller must pass X/Y in the environment.
if [[ -z "${X:-}" || -z "${Y:-}" ]]; then
    read -r X Y < <(psql -At -F' ' -c "SELECT x, y FROM characters
        WHERE zona = 'ilha_inicial' AND xp = 0 ORDER BY updated DESC LIMIT 1")
fi
[[ -n "${X:-}" && -n "${Y:-}" ]] || { echo "no fresh Bosque character to copy the spawn from; pass X=.. Y=.." >&2; exit 1; }
echo "spawn of the starting island: x=$X y=$Y"

psql -c "SELECT coalesce(zona, '(null)') AS zona, count(*) FROM characters GROUP BY 1 ORDER BY 2 DESC"
if [[ "$sim" == "--sim" ]]; then
    echo "--sim: nothing changed"
    exit 0
fi
psql <<SQL
BEGIN;
UPDATE characters
   SET zona = 'ilha_inicial', x = $X, y = $Y, magica_volta = '', zona_volta = NULL
 WHERE zona IS DISTINCT FROM 'ilha_inicial' OR x <> $X OR y <> $Y;
SELECT coalesce(zona, '(null)') AS zona, count(*) FROM characters GROUP BY 1;
COMMIT;
SQL
