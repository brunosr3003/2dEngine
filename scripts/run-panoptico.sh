#!/usr/bin/env bash
# Sobe o painel de observabilidade em cima dos canais que ja' estao rodando.
#
# O panoptico ve' conta, ouro e posicao de TODO MUNDO. Ele escuta em
# 127.0.0.1 (`PANOPTICO_WEB_BIND`) e, pra acesso remoto, fica atras de um
# proxy HTTPS (nginx) ou de tunel SSH. Sem sessao nada passa: senha em
# `PANOPTICO_SENHA` (16+ chars), cookie HttpOnly. Ver docs/PANOPTICO.md.
#
# Os canais so' respondem se tiverem sido subidos com PANOPTICO_BIND — a porta
# do painel de um canal e' a do jogo + PANOPTICO_OFFSET (1000).
set -eu
cd "$(dirname "$0")/.."

: "${MMO_ADMIN_TOKEN:?defina MMO_ADMIN_TOKEN (o mesmo dos processos de jogo)}"
: "${PANOPTICO_SENHA:=${PANOPTICO_TOKEN:-}}"
[ ${#PANOPTICO_SENHA} -ge 16 ] || { echo "defina PANOPTICO_SENHA (16+ chars) — a senha do painel"; exit 1; }
export PANOPTICO_SENHA
# Local em http: sem isto o navegador nao guarda o cookie `Secure` fora de localhost.
: "${PANOPTICO_COOKIE_SEGURO:=0}"; export PANOPTICO_COOKIE_SEGURO

[ "${1:-}" = "--build" ] && { cargo build --release -p panoptico; shift; }

# Recompila sozinho quando o fonte esta' na frente do binario.
#
# A pagina e' `include_str!`: ela entra no binario em tempo de COMPILACAO.
# Editar o HTML e reiniciar o processo nao muda nada — o painel continua
# servindo a versao velha, e parece que a mudanca nao funcionou.
BIN=target/release/panoptico
[ -x "$BIN" ] || BIN=target/debug/panoptico
if [ ! -x "$BIN" ] || [ -n "$(find crates/panoptico crates/shared -newer "$BIN" -print -quit 2>/dev/null)" ]; then
    echo "fonte mais novo que o binario — recompilando"
    cargo build -p panoptico || exit 1
    BIN=target/debug/panoptico
fi

echo "painel:  http://${PANOPTICO_WEB_BIND:-127.0.0.1:8090}${PANOPTICO_PREFIXO:-}/  (entra com a PANOPTICO_SENHA)"
exec "$BIN"
