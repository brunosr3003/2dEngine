#!/usr/bin/env bash
# Cria as contas que o loadbot usa. Todas com a MESMA senha do usuario de dev,
# porque o hash argon2 e' copiado — gerar N hashes levaria mais tempo que o
# teste inteiro.
#
#   ./scripts/loadbot-accounts.sh 200 [banco]
set -eu
N="${1:-50}"
DB="${2:-mmo_dev}"
HASH=$(docker exec mmo-pg psql -U solar -d "$DB" -Atc \
  "select password_hash from accounts where username='bruno' limit 1")
[ -n "$HASH" ] || { echo "conta 'bruno' nao existe em $DB"; exit 1; }

SQL="INSERT INTO accounts (username,email,password_hash) SELECT v.u, v.u||'@load', '$HASH' FROM (VALUES"
for i in $(seq 0 $((N-1))); do
  SQL="$SQL ('bot$i'),"
done
SQL="${SQL%,}) AS v(u) WHERE NOT EXISTS (SELECT 1 FROM accounts a WHERE a.username = v.u);"
docker exec mmo-pg psql -U solar -d "$DB" -c "$SQL"
