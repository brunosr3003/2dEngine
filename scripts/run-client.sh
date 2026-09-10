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

swallow() { hyprctl eval "hl.config({ misc = { enable_swallow = $1 } })" >/dev/null 2>&1; }

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
