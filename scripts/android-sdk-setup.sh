#!/usr/bin/env bash
# Instala, NA HOME e sem root, o que a build e o emulador de Android precisam:
# JDK 17, SDK do Android (cmdline-tools, platform-tools, emulador, plataforma,
# build-tools, NDK) e uma imagem de sistema x86_64 pro emulador.
#
# Sem root de proposito: nesta maquina nao ha' sudo sem senha nem helper do AUR.
# Um SDK so' serve as duas coisas (emulador e build) — a imagem Docker do
# macroquad (notfl3/cargo-apk) faria a build sozinha, mas sao ~4 GB a mais.
#
# Idempotente: o que ja' esta' instalado e' pulado.
#
# Uso:  scripts/android-sdk-setup.sh
# Depois:  export JAVA_HOME=~/opt/jdk-17 ANDROID_HOME=~/Android/Sdk
set -euo pipefail

OPT="$HOME/opt"
JDK="$OPT/jdk-17"
SDK="$HOME/Android/Sdk"
mkdir -p "$OPT" "$SDK/cmdline-tools"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ ! -x "$JDK/bin/java" ]; then
  echo "== JDK 17 (Temurin)"
  curl -fL --retry 3 -o "$TMP/jdk17.tar.gz" \
    "https://api.adoptium.net/v3/binary/latest/17/ga/linux/x64/jdk/hotspot/normal/eclipse"
  rm -rf "$JDK" && mkdir -p "$JDK"
  tar -xzf "$TMP/jdk17.tar.gz" -C "$JDK" --strip-components=1
fi
export JAVA_HOME="$JDK"
export PATH="$JAVA_HOME/bin:$PATH"
java -version 2>&1 | head -1

if [ ! -x "$SDK/cmdline-tools/latest/bin/sdkmanager" ]; then
  echo "== Android cmdline-tools"
  curl -fL --retry 3 -o "$TMP/cmdline-tools.zip" \
    "https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip"
  unzip -q "$TMP/cmdline-tools.zip" -d "$TMP/ct"
  rm -rf "$SDK/cmdline-tools/latest"
  mv "$TMP/ct/cmdline-tools" "$SDK/cmdline-tools/latest"
fi

SDKM="$SDK/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --sdk_root="$SDK" --licenses >/dev/null || true
echo "== pacotes do SDK (o maior e' a imagem de sistema, ~1,5 GB)"
"$SDKM" --sdk_root="$SDK" \
  "platform-tools" \
  "emulator" \
  "platforms;android-34" \
  "build-tools;34.0.0" \
  "ndk;25.2.9519653" \
  "system-images;android-34;google_apis;x86_64"

AVD="$HOME/.android/avd/tempest.avd"
if [ ! -d "$AVD" ]; then
  echo "== AVD tempest (Pixel 6, x86_64, GPU do host, teclado fisico)"
  echo no | "$SDK/cmdline-tools/latest/bin/avdmanager" create avd -n tempest \
    -k "system-images;android-34;google_apis;x86_64" -d pixel_6 >/dev/null
  sed -i 's/^hw.keyboard=.*/hw.keyboard=yes/; s/^hw.ramSize=.*/hw.ramSize=4096/;
          s/^hw.gpu.enabled=.*/hw.gpu.enabled=yes/; s/^hw.gpu.mode=.*/hw.gpu.mode=host/' "$AVD/config.ini"
fi

echo "== pronto"
echo "   JAVA_HOME=$JDK"
echo "   ANDROID_HOME=$SDK"
