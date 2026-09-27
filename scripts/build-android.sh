#!/usr/bin/env bash
# Build Android do cliente Rust (macroquad) -> APK assinado com a chave de debug.
# Roda NESTE PC. O toolchain fica na home (scripts/android-sdk-setup.sh).
#
# O `cargo quad-apk` le' o [package.metadata.android] do crates/client/Cargo.toml
# (pacote, paisagem, permissao de internet, icone, assets) e gera um .apk com
# arm64 (celular) e x86_64 (emulador).
#
# Uso:
#   scripts/build-android.sh              build release
#   scripts/build-android.sh --instalar   build + adb install no aparelho/emulador conectado
#   scripts/build-android.sh --abrir      build + install + abre o app
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
python3 scripts/check-release-notes.py

# Compilar Rust não verifica GLSL: drivers Android podem recusar o shader.
python3 scripts/check-shaders.py
python3 scripts/check-water-shaders.py

export JAVA_HOME="${JAVA_HOME:-$HOME/opt/jdk-17}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK_VERSAO="25.2.9519653"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSAO}"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"

PACOTE="com.brunji.tempest"   # = package_name do crates/client/Cargo.toml
INSTALAR=0
ABRIR=0
while [ $# -gt 0 ]; do
  case "$1" in
    --instalar) INSTALAR=1; shift;;
    --abrir) INSTALAR=1; ABRIR=1; shift;;
    *) echo "flag desconhecida: $1"; exit 2;;
  esac
done

[ -d "$NDK_HOME" ] || { echo "ERRO: sem NDK em $NDK_HOME (rode scripts/android-sdk-setup.sh)"; exit 1; }
# A 0.1.4 do crates.io embute o Cargo 1.61, que nao entende `version.workspace`
# e recusa o workspace. A do GitHub (Cargo 0.87) funciona: revisao fixada.
QUAD_APK_REV="d411c8fe1c08339e46d7dc6b40ac04ea5e3a01b6"
if ! cargo install --list | grep -q "cargo-quad-apk.*$QUAD_APK_REV"; then
  cargo install --locked --force --git https://github.com/not-fl3/cargo-quad-apk --rev "$QUAD_APK_REV"
fi
rustup target add aarch64-linux-android x86_64-linux-android >/dev/null

cargo quad-apk build --release -p client

APK="$ROOT/target/android-artifacts/release/apk/client.apk"
[ -f "$APK" ] || { echo "ERRO: APK nao saiu em $APK"; exit 1; }

# O quad-apk nomeia a lib que a MainActivity carrega pelo FIM do package_name
# (com.brunji.tempest -> System.loadLibrary("tempest")), mas o .so sai com o
# nome do binario (libclient.so): o app fechava ao abrir com
# `dlopen failed: library "libtempest.so" not found`. Renomeia dentro do APK e
# assina de novo (a assinatura cobre os nomes dos arquivos).
LIB_JAVA="${PACOTE##*.}"
BT="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
BRUTO="$(mktemp --suffix=.apk)"
python3 - "$APK" "$BRUTO" "$LIB_JAVA" <<'PY'
import sys, zipfile
origem, destino, lib = sys.argv[1:]
with zipfile.ZipFile(origem) as a, zipfile.ZipFile(destino, "w") as b:
    for e in a.infolist():
        if e.filename.startswith("META-INF/"):
            continue  # assinatura velha
        nome = e.filename
        if nome.startswith("lib/") and nome.endswith("/libclient.so"):
            nome = nome[: -len("libclient.so")] + f"lib{lib}.so"
        # mesma compressao da entrada original: resources.arsc TEM que ficar
        # sem compressao a partir do targetSdk 30
        b.writestr(zipfile.ZipInfo(nome, e.date_time), a.read(e), compress_type=e.compress_type)
PY
"$BT/zipalign" -f 4 "$BRUTO" "$APK"
rm -f "$BRUTO"
"$BT/apksigner" sign --ks "$HOME/.android/debug.keystore" --ks-pass pass:android "$APK"
echo "==> $APK ($(du -h "$APK" | cut -f1))"

if [ "$INSTALAR" = 1 ]; then
  adb install -r "$APK"
fi
if [ "$ABRIR" = 1 ]; then
  adb shell am start -n "$PACOTE/.MainActivity"
fi
