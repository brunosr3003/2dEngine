#!/usr/bin/env bash
# Merge de servidores: move contas e personagens de um realm pra outro.
#
# Servidor tem banco proprio, entao juntar dois e' operacao de BANCO — a unica
# ponte que existe entre realms. Nao ha caminho por rede: jogador nunca
# atravessa, e isso e' de proposito.
#
#   ./scripts/merge-realms.sh mmo_sa02 mmo_sa01            # simulacao
#   ./scripts/merge-realms.sh mmo_sa02 mmo_sa01 --aplicar  # pra valer
#
# ORIGEM esvazia, DESTINO recebe. A origem NAO e' apagada: depois de conferir,
# o operador dropa o banco na mao.
#
# ── Colisao de nome ──────────────────────────────────────────────────────────
# `characters.name` e' chave primaria e `accounts.username`/`email` sao unicos.
# Dois realms que rodaram anos separados TEM nomes repetidos — e' o caso comum,
# nao a excecao. Quem vem da origem ganha sufixo (`Bruno` -> `Bruno-SA02`), e a
# simulacao lista todos antes de qualquer escrita.
#
# O merge NAO mistura economia (items, skills, drops, lojas): sao conteudo do
# jogo, iguais nos dois lados, e vem do seed.
set -euo pipefail

ORIGEM="${1:?uso: merge-realms.sh <banco_origem> <banco_destino> [--aplicar]}"
DESTINO="${2:?uso: merge-realms.sh <banco_origem> <banco_destino> [--aplicar]}"
APLICAR="${3:-}"
PG="${PG_CONTAINER:-mmo-pg}"
PGUSER_="${PG_USER:-solar}"
SUFIXO="${MERGE_SUFIXO:--$(echo "$ORIGEM" | tr '[:lower:]' '[:upper:]' | sed 's/^MMO_//')}"

TABELAS="accounts characters inventory equipment vault proficiencies player_skills character_quests"

psql_() { docker exec -i "$PG" psql -U "$PGUSER_" -v ON_ERROR_STOP=1 -d "$1" "${@:2}"; }

echo "origem  $ORIGEM"
echo "destino $DESTINO"
echo "sufixo  $SUFIXO"
echo

# ── 1. Traz a origem pra dentro do destino, num schema separado ─────────────
# Com o dado dos dois lados no MESMO banco, o merge vira SQL comum — sem
# copiar linha por linha por fora, sem transacao distribuida. Falhou no meio?
# Dropa o schema e nada vazou pro jogo.
ARGS=""; for t in $TABELAS; do ARGS="$ARGS -t $t"; done
# `--section=pre-data --section=data` traz CREATE TABLE + dados e deixa de
# fora o post-data: indices e CHAVES ESTRANGEIRAS. Isso e' proposital — o
# `player_skills` referencia `skills`, que NAO vem no merge (e' conteudo de
# jogo, ja existe no destino). Trazer a FK faria o import procurar
# `origem.skills` e falhar. O schema `origem` e' area de passagem, nao precisa
# de integridade propria; quem valida e' o destino, no INSERT final.
docker exec "$PG" pg_dump -U "$PGUSER_" -d "$ORIGEM" $ARGS --no-owner --no-acl \
  --section=pre-data --section=data > /tmp/merge_origem.sql
psql_ "$DESTINO" -q -c "DROP SCHEMA IF EXISTS origem CASCADE; CREATE SCHEMA origem;"
# O pg_dump QUALIFICA tudo como `public.accounts`, entao mexer em search_path
# nao adianta — o arquivo ignora. Reescrever o prefixo e' o que faz a origem
# cair em `origem.` em vez de tentar recriar `public` por cima do banco vivo.
sed "s/\bpublic\./origem./g" /tmp/merge_origem.sql | psql_ "$DESTINO" -q >/dev/null

# ── 2. Relatorio ────────────────────────────────────────────────────────────
psql_ "$DESTINO" -c "
SELECT (SELECT count(*) FROM origem.accounts)   AS contas_a_mover,
       (SELECT count(*) FROM origem.characters) AS chars_a_mover,
       (SELECT count(*) FROM origem.accounts o JOIN public.accounts d USING (username))   AS contas_em_colisao,
       (SELECT count(*) FROM origem.characters o JOIN public.characters d USING (name))   AS chars_em_colisao;"

psql_ "$DESTINO" -c "
SELECT o.name AS personagem, o.name || '${SUFIXO}' AS vira
  FROM origem.characters o JOIN public.characters d USING (name)
 ORDER BY 1 LIMIT 40;"

if [ "$APLICAR" != "--aplicar" ]; then
  psql_ "$DESTINO" -q -c "DROP SCHEMA origem CASCADE;"
  echo "SIMULACAO — nada foi escrito. rode com --aplicar pra valer."
  exit 0
fi

echo "!! os servidores dos DOIS realms precisam estar PARADOS !!"
echo "   merge com servidor no ar perde tudo que for salvo durante a copia."
# A confirmacao le do TERMINAL, nao do stdin: os `docker exec -i` acima comem
# o stdin do script, e um `read` normal passaria direto sem ninguem responder.
if [ "${MERGE_CONFIRMA:-}" = "1" ]; then
  ok=s
elif [ -r /dev/tty ]; then
  read -r -p "os dois estao parados? [s/N] " ok < /dev/tty
else
  echo "sem terminal: rode com MERGE_CONFIRMA=1 pra confirmar"; ok=n
fi
[ "$ok" = "s" ] || { psql_ "$DESTINO" -q -c "DROP SCHEMA origem CASCADE;"; echo "abortado"; exit 1; }

# ── 3. Merge, numa transacao so' ────────────────────────────────────────────
psql_ "$DESTINO" <<SQL
BEGIN;

-- Renomeia dentro do schema de origem ANTES de copiar, pra o INSERT no
-- destino ja sair com o nome final.
--
-- O rename tem que passar em TODAS as tabelas dependentes na mao: o schema de
-- passagem foi importado sem chaves estrangeiras (ver `--section` acima),
-- entao nao ha ON UPDATE CASCADE pra propagar. Trocar so' `characters` deixa
-- inventario e equipamento apontando pro nome velho e o INSERT quebra na
-- chave primaria.
CREATE TEMP TABLE ren AS
SELECT o.name AS antigo, o.name || '${SUFIXO}' AS novo
  FROM origem.characters o
 WHERE EXISTS (SELECT 1 FROM public.characters d WHERE d.name = o.name);

UPDATE origem.characters     t SET name = r.novo           FROM ren r WHERE t.name = r.antigo;
UPDATE origem.inventory      t SET character_name = r.novo FROM ren r WHERE t.character_name = r.antigo;
UPDATE origem.equipment      t SET character_name = r.novo FROM ren r WHERE t.character_name = r.antigo;
UPDATE origem.vault          t SET character_name = r.novo FROM ren r WHERE t.character_name = r.antigo;
UPDATE origem.proficiencies  t SET character_name = r.novo FROM ren r WHERE t.character_name = r.antigo;
UPDATE origem.player_skills  t SET character_name = r.novo FROM ren r WHERE t.character_name = r.antigo;
UPDATE origem.character_quests t SET char_name = r.novo    FROM ren r WHERE t.char_name = r.antigo;

UPDATE origem.accounts o
   SET username = o.username || '${SUFIXO}'
 WHERE EXISTS (SELECT 1 FROM public.accounts d WHERE d.username = o.username);
UPDATE origem.accounts o
   SET email = o.email || '${SUFIXO}'
 WHERE EXISTS (SELECT 1 FROM public.accounts d WHERE d.email = o.email);

-- Contas: o id novo e' gerado pelo destino, entao guarda o de-para.
CREATE TEMP TABLE conta_de_para AS
WITH inseridas AS (
  INSERT INTO public.accounts (username, email, password_hash, class)
  SELECT username, email, password_hash, class FROM origem.accounts
  RETURNING id, username
)
SELECT o.id AS id_antigo, i.id AS id_novo
  FROM inseridas i JOIN origem.accounts o USING (username);

INSERT INTO public.characters SELECT * FROM origem.characters;
UPDATE public.characters c
   SET account_id = d.id_novo
  FROM conta_de_para d
 WHERE c.account_id = d.id_antigo
   AND c.name IN (SELECT name FROM origem.characters);

INSERT INTO public.inventory        SELECT * FROM origem.inventory;
INSERT INTO public.equipment        SELECT * FROM origem.equipment;
INSERT INTO public.vault            SELECT * FROM origem.vault;
INSERT INTO public.proficiencies    SELECT * FROM origem.proficiencies;
INSERT INTO public.player_skills    SELECT * FROM origem.player_skills;
INSERT INTO public.character_quests SELECT * FROM origem.character_quests;

COMMIT;
SQL

psql_ "$DESTINO" -q -c "DROP SCHEMA origem CASCADE;"
echo
echo "pronto. confira o destino e so' depois dropar o banco '$ORIGEM'."
