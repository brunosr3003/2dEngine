#!/bin/bash
# Reseta a posicao salva de TODOS os personagens pro spawn default do
# mapa atual + restaura HP cheio.
#
# Uso:
#   ./scripts/reset-positions.sh                           # mapa crafted (default)
#   MAP_FILE=path/to.mapfile ./scripts/reset-positions.sh  # mesmo MapFile do server
#
# Por que existe: posicoes salvas no DB podem cair fora do mapa atual
# (mapa regenerado, troca de MapFile, etc). O wall-check do server cobre
# tiles WALL/OOB; tiles validos mas em ilhas inacessiveis nao. Esse
# script forca o reset em massa via mesma logica de spawn que o server.

set -e

cd "$(dirname "$0")/.."

# Server rodando = tem cache em memoria — reset so vale apos restart.
if lsof -nP -iTCP:9000 -sTCP:LISTEN 2>/dev/null | grep -q LISTEN; then
  echo "⚠️  Server local rodando na :9000."
  echo "   O reset NAO afeta sessoes ativas (cache em memoria). Posicoes"
  echo "   atualizadas so serao usadas no PROXIMO login apos restart do server."
  echo
  read -p "Continuar mesmo assim? [y/N] " yn
  case $yn in
    [Yy]*) ;;
    *) echo "abortado."; exit 1;;
  esac
fi

# Garante cargo no PATH (shells non-login do macOS nao herdam ~/.cargo/bin).
export PATH="$HOME/.cargo/bin:$PATH"

cargo run --bin reset_positions --quiet "$@"

echo
echo "Pronto. Pra aplicar nas sessoes existentes:"
echo "  1. Pare o server (Ctrl-C)"
echo "  2. Reinicie:  cargo run --bin server"
echo "  3. Players relogarem"
