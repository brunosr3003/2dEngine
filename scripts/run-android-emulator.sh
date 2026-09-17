#!/usr/bin/env bash
# Sobe o emulador de Android (AVD "tempest") lado a lado com o terminal e, com
# --instalar, instala e abre o APK assim que o sistema termina de bootar.
#
# Mesmo cuidado do run-client.sh: o swallow do Hyprland engoliria o kitty que
# lancou o emulador, entao fica desligado enquanto o emulador roda.
#
# Toolchain e AVD: scripts/android-sdk-setup.sh (o AVD e' criado la').
#
# Uso:  scripts/run-android-emulator.sh [--instalar]
set -u
cd "$(dirname "$0")/.."

export JAVA_HOME="${JAVA_HOME:-$HOME/opt/jdk-17}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
ADB="$ANDROID_HOME/platform-tools/adb"
APK=target/android-artifacts/release/apk/client.apk

swallow() { hyprctl eval "hl.config({ misc = { enable_swallow = $1 } })" >/dev/null 2>&1; }
trap 'swallow true' EXIT INT TERM
swallow false

if [ "${1:-}" = "--instalar" ]; then
    (
        "$ADB" wait-for-device
        until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
        "$ADB" install -r "$APK" && "$ADB" shell monkey -p com.brunji.tempest -c android.intent.category.LAUNCHER 1 >/dev/null
    ) &
fi

# -gpu host: GL da placa (a RTX), nao o SwiftShader em CPU.
DISPLAY="${DISPLAY:-:1}" \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" \
QT_QPA_PLATFORM=xcb \
"$ANDROID_HOME/emulator/emulator" -avd tempest -gpu host -no-boot-anim -no-audio -no-metrics
