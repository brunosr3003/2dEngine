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

# ── A janela do emulador nao pode ser TILADA ─────────────────────────────────
#
# Tilada, o Hyprland a redimensiona DEPOIS do mapa, e o emulador continua
# mapeando o toque pelo tamanho que ele mesmo escolheu: o clique do mouse cai
# no lugar errado (ou em lugar nenhum) enquanto `adb shell input tap` continua
# funcionando — foi assim que o defeito apareceu, em 20/09/2026. E' o mesmo
# que ja' acontecia com a janela do QEMU, e a licao e' a mesma: a regra tem
# que existir ANTES do mapa, porque float/resize em runtime devolvem "ok" e
# nao mudam nada.
#
# Registrada por `eval` e nao no arquivo de config de proposito: recarregar a
# config derruba o stream do Moonlight.
#
# A tela do aparelho e' 2400x1080 deitada (o jogo e' paisagem); a janela segue
# essa proporcao pra nao sobrar tarja, que e' area onde o clique nao faz nada.
#
# A POSICAO nao vem daqui e nem adianta forcar: o emulador guarda a dele em
# `~/.android/avd/tempest.avd/emulator-user.ini` e ignora o gerenciador de
# janelas (`move` na regra e `movewindowpixel` em runtime nao surtem efeito).
# Escrever o ini na mao tambem nao resolve, porque a posicao e' aplicada com a
# janela ainda EM PE' e a rotacao pra paisagem a desloca — num teste foi parar
# em y negativo, fora da tela.
#
# O jeito que funciona e' o simples: ARRASTE a janela uma vez pra onde voce
# quiser e o emulador lembra dali em diante.
hyprctl eval 'hl.window_rule({
    name  = "android-emulador",
    match = { class = "^(Emulator)$" },
    float = true,
    fullscreen = false,
    fullscreen_state = 0,
    size  = { "monitor_w*0.47", "monitor_h*0.39" },
    move  = { "monitor_w*0.52", "monitor_h*0.30" },
})' >/dev/null 2>&1

# A JANELA TEM QUE GIRAR JUNTO COM O APARELHO
#
# O jogo forca paisagem. O Android gira a tela (o `screencap` sai 2400x1080),
# mas a janela do emulador continua em PE' e mostra o quadro deitado dentro do
# quadro retrato. O emulador entao mapeia o toque pela geometria RETRATO — e o
# clique do mouse nao acerta nada, enquanto `adb shell input tap` continua
# funcionando, porque entra direto no Android sem passar pela janela. Foi
# assim que o defeito apareceu, em 20/09/2026.
#
# `adb emu rotate` gira a janela 90 graus. Aqui a gente compara a orientacao
# do guest com a da janela e gira ate' casar (no maximo 3 vezes, que e' a
# volta inteira).
casa_a_rotacao() {
    for _ in 1 2 3; do
        local g j
        g=$("$ADB" exec-out screencap -p 2>/dev/null | python3 -c "
import sys, struct
d = sys.stdin.buffer.read()
i = d.index(b'IHDR')
w, h = struct.unpack('>II', d[i + 4:i + 12])
print('deitado' if w > h else 'empe')
" 2>/dev/null)
        j=$(hyprctl clients -j 2>/dev/null | python3 -c "
import json, sys
for c in json.load(sys.stdin):
    if c.get('class') == 'Emulator' and 'tempest' in c.get('title', ''):
        w, h = c['size']
        print('deitado' if w > h else 'empe')
        break
" 2>/dev/null)
        [ -z "$g" ] || [ -z "$j" ] && return 0
        [ "$g" = "$j" ] && return 0
        "$ADB" emu rotate >/dev/null 2>&1
        sleep 2
    done
}

if [ "${1:-}" = "--instalar" ]; then
    (
        "$ADB" wait-for-device
        until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
        "$ADB" install -r "$APK" && "$ADB" shell monkey -p com.brunji.tempest -c android.intent.category.LAUNCHER 1 >/dev/null
        sleep 12
        casa_a_rotacao
    ) &
fi

# -gpu host: GL da placa (a RTX), nao o SwiftShader em CPU. `GPU=...` troca
# (swiftshader_indirect e' o software; swangle_indirect DERRUBA o app).
DISPLAY="${DISPLAY:-:1}" \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" \
QT_QPA_PLATFORM="${QT_PLAT:-xcb}" \
"$ANDROID_HOME/emulator/emulator" -avd tempest -gpu "${GPU:-host}" -no-boot-anim -no-audio -no-metrics
