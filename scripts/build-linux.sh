#!/usr/bin/env bash
# Build cross-compile do server pra Linux x86_64 (rodar no VPS).
#
# Uso (do Mac):
#   ./scripts/build-linux.sh
#
# Pré-req (Mac, 1x só):
#   rustup target add x86_64-unknown-linux-gnu
#   brew install x86_64-linux-musl-cross filosottile/musl-cross/musl-cross  # se musl
#   brew install messense/macos-cross-toolchains/x86_64-unknown-linux-gnu   # se gnu
#
# Saída: target/x86_64-unknown-linux-gnu/release/server
#        + tarball pronto pra scp em dist/server-linux.tar.gz
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="x86_64-unknown-linux-gnu"
PROFILE="release"

if ! rustup target list --installed | grep -q "^${TARGET}\$"; then
    echo "instalando target rust ${TARGET}..."
    rustup target add "${TARGET}"
fi

# Configura linker se ainda não tem
LINKER_BIN="x86_64-unknown-linux-gnu-gcc"
if ! command -v "${LINKER_BIN}" &>/dev/null; then
    echo "ERRO: ${LINKER_BIN} não encontrado."
    echo "Instale com: brew install messense/macos-cross-toolchains/x86_64-unknown-linux-gnu"
    exit 1
fi

# Variáveis de ambiente do cargo pra usar o linker correto
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="${LINKER_BIN}"
export CC_x86_64_unknown_linux_gnu="${LINKER_BIN}"

cargo build --release --bin server --target "${TARGET}"

BIN="target/${TARGET}/${PROFILE}/server"
test -f "${BIN}" || { echo "build falhou: ${BIN} não existe"; exit 1; }

mkdir -p dist
TAR="dist/server-linux-x86_64.tar.gz"

# Stage server binary + mapfile num diretorio temp e empacota tudo de uma vez.
# Não da pra `tar -rzf` em tarball gzipado (silenciosamente falha) — por isso
# o stage. Layout alvo no VPS: /opt/mmorpg/server + /opt/mmorpg/data/maps/game.json.
STAGE="$(mktemp -d)"
trap 'rm -rf "${STAGE}"' EXIT
cp "target/${TARGET}/${PROFILE}/server" "${STAGE}/server"
mkdir -p "${STAGE}/data/maps"
# Copia TODOS os mapas (game.json + tutorial.json) — o processo de tutorial
# sobe com MAP_FILE=data/maps/tutorial.json.
cp data/maps/*.json "${STAGE}/data/maps/"
tar -C "${STAGE}" -czf "${TAR}" server data/maps

ls -lh "${BIN}" "${TAR}"
echo
echo "Pronto. Sobe pro VPS:"
echo "  scp ${TAR} mmo@VPS_IP:/opt/mmorpg/"
echo "  ssh mmo@VPS_IP"
echo "  cd /opt/mmorpg && tar -xzf server-linux-x86_64.tar.gz"
echo "  sudo systemctl restart mmorpg-server"
