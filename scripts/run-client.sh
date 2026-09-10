#!/usr/bin/env bash
# Sobe o cliente lado a lado com o terminal.
#
# ── O motivo do script existir ───────────────────────────────────────────────
# O Hyprland esta com `misc:enable_swallow = true` e `swallow_regex` casando
# kitty. Quando um app grafico e' lancado a partir de um terminal, o compositor
# ENGOLE o terminal: esconde ele e da a janela nova o tile inteiro. Por isso o
# jogo aparecia cobrindo o kitty — e por isso abrir um kitty pela keybind
# funciona (nao tem terminal pai) mas abrir pelo terminal nao.
#
# Nao e' bug do cliente: um `kitty --class teste` lancado do terminal se
# comporta igual. Ver docs/DESENVOLVIMENTO_LADO_A_LADO.md.
#
# `swallow_exception_regex` foi testado (por classe e por titulo) e nao surtiu
# efeito nesta versao, entao o jeito e' desligar o swallow enquanto o jogo roda
# e religar na saida.
#
# Uso:  ./scripts/run-client.sh [--build]
set -u
cd "$(dirname "$0")/.."

[ "${1:-}" = "--build" ] && { cargo build --bin client || exit 1; shift; }

# Recompila sozinho quando o fonte esta' na frente do binario.
#
# Rodar `cargo test` NAO reconstroi o binario do jogo — ele constroi o
# executavel de teste, que e' outro. Ja' aconteceu de eu tirar codigo, ver os
# testes passarem e lancar um binario velho que ainda tinha o codigo removido:
# o defeito volta e parece que a correcao nao funcionou.
BIN=target/debug/client
if [ ! -x "$BIN" ] || [ -n "$(find crates -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]; then
    echo "fonte mais novo que o binario — recompilando"
    cargo build --bin client || exit 1
fi

swallow() { hyprctl eval "hl.config({ misc = { enable_swallow = $1 } })" >/dev/null 2>&1; }

# ── LADO A LADO COM O TERMINAL ───────────────────────────────────────────────
#
# A janela nasce no workspace ATIVO, que nem sempre e' o do terminal que
# chamou o script — rodando por fora (de outra aba, de uma sessao remota, de
# uma ferramenta), o jogo abre sozinho num canto e o terminal fica noutro.
#
# Entao o script guarda onde o terminal esta' e, se o jogo cair em outro
# lugar, traz de volta. `hl.dsp.window.move` age na janela ATIVA, e janela
# nova nasce com foco — por isso a correcao e' logo depois de ela aparecer.
workspace_do_terminal() {
    local pid=$PPID
    for _ in 1 2 3 4 5 6; do
        [ -z "$pid" ] || [ "$pid" = 1 ] && break
        local ws
        ws=$(hyprctl clients -j 2>/dev/null | python3 -c "
import json,sys
alvo=int(sys.argv[1])
for c in json.load(sys.stdin):
    if c['pid']==alvo: print(c['workspace']['id']); break
" "$pid" 2>/dev/null)
        [ -n "$ws" ] && { echo "$ws"; return; }
        pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')
    done
}

ALVO=$(workspace_do_terminal)
if [ -n "$ALVO" ]; then
    (
        for _ in $(seq 1 40); do
            sleep 0.5
            atual=$(hyprctl clients -j 2>/dev/null | python3 -c "
import json,sys
for c in json.load(sys.stdin):
    if c['class']=='tempest': print(c['workspace']['id']); break
" 2>/dev/null)
            [ -z "$atual" ] && continue
            [ "$atual" = "$ALVO" ] && break
            hyprctl dispatch "hl.dsp.window.move({ workspace = $ALVO })" >/dev/null 2>&1
            break
        done
    ) &
fi

# Religa o swallow aconteca o que acontecer — Ctrl+C, crash, kill.
trap 'swallow true' EXIT INT TERM
swallow false

hyprctl eval "hl.window_rule({ name = 'tempest-cliente', match = { class = '^(tempest)\$' }, content = 'none', float = false, fullscreen = false, fullscreen_state = 0 })" >/dev/null 2>&1

# A escolha de servidor e canal virou TELA no cliente (ver `api.rs`/`ui.rs`).
# Setar MMO_HOST aqui pula a tela — util pro teste de carga, atrapalha o resto.
DISPLAY="${DISPLAY:-:1}" \
WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}" \
XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" \
./target/debug/client "$@"
