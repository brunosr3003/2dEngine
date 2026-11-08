#!/usr/bin/env bash
# Build Android do cliente Rust (macroquad) -> APK assinado com a chave de debug.
# Roda NESTE PC. O toolchain fica na home (scripts/android-sdk-setup.sh).
#
# Migrado de `cargo quad-apk` pra' Gradle em 03/11/2026 (ver docs/BUILD.md):
# a Play Billing Library so' e' distribuida como AAR com uma arvore de
# dependencias (androidx, Play Services, datatransport) que o pipeline cru de
# javac+d8 do quad-apk nao resolvia. O projeto Gradle de verdade fica em
# crates/client/android/ (modulo `app`). Este script so' exporta as
# variaveis do NDK que as tasks `cargoBuildClientArm64`/`cargoBuildClientX86_64`
# do app/build.gradle.kts leem pra' compilar e linkar o `.so` do Rust, e
# chama o gradlew.
#
# Uso:
#   scripts/build-android.sh              build debug
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
export ANDROID_NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSAO}"
NDK_HOME="$ANDROID_NDK_HOME"
export PATH="$HOME/.cargo/bin:$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"

PACOTE="com.niertech.tempest"   # = applicationId do crates/client/android/app/build.gradle.kts
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
rustup target add aarch64-linux-android x86_64-linux-android >/dev/null

# Linker/CC por alvo, pro `cargo build` dentro das tasks do Gradle
# (app/build.gradle.kts le' essas variaveis do ambiente do PROCESSO que
# chamou o gradlew — por isso o --no-daemon abaixo: um daemon reaproveitado
# manteria o ambiente da PRIMEIRA chamada, nao desta).
NDK_BIN="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android24-clang"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android24-clang"
export CC_x86_64_linux_android="$NDK_BIN/x86_64-linux-android24-clang"
export AR_x86_64_linux_android="$NDK_BIN/llvm-ar"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$NDK_BIN/x86_64-linux-android24-clang"

cd crates/client/android
./gradlew --no-daemon assembleDebug
cd "$ROOT"

APK="$ROOT/crates/client/android/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$APK" ] || { echo "ERRO: APK nao saiu em $APK"; exit 1; }
echo "==> $APK ($(du -h "$APK" | cut -f1))"

if [ "$INSTALAR" = 1 ]; then
  adb install -r "$APK"
fi
if [ "$ABRIR" = 1 ]; then
  adb shell am start -n "$PACOTE/.MainActivity"
fi
