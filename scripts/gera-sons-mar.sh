#!/usr/bin/env bash
# The open sea's combat sounds (gameplay remake, 16/11/2026), synthesized
# with sox like `gera-sons-eventos.sh`: no third-party samples, no license to
# track. Short and mono at 22 kHz with Vorbis quality 3, so each file is a few
# KB (mobile: small downloads, little memory).
# Usage: scripts/gera-sons-mar.sh assets/audio/mar   (needs sox with Vorbis)
set -euo pipefail
out="${1:?output dir}"
tmp="$(mktemp -d)"
mkdir -p "$out"
R=22050

# ── cannon fire: a low boom (falling sine) under a burst of brown noise ──
sox -n -r $R -c 1 "$tmp/boom.wav" synth 0.55 sine 70:38 fade q 0.003 0.55 0.5 gain -n -2
sox -n -r $R -c 1 "$tmp/estouro.wav" synth 0.4 brownnoise lowpass 1400 fade q 0.001 0.4 0.36 gain -n -4
sox -m "$tmp/boom.wav" "$tmp/estouro.wav" "$tmp/canhao.wav" reverb 25 gain -n -1

# ── splash: filtered white noise, a quick rise and a long wash ──
sox -n -r $R -c 1 "$tmp/agua.wav" synth 0.6 whitenoise gain -6 highpass 500 lowpass 4200 fade q 0.02 0.6 0.5 \
    tremolo 23 30 gain -n -6

# ── impact on wood: a dull knock and a crack of splinters ──
sox -n -r $R -c 1 "$tmp/baque.wav" synth 0.18 sine 180:90 fade q 0.001 0.18 0.16 gain -n -2
sox -n -r $R -c 1 "$tmp/racha.wav" synth 0.22 pinknoise bandpass 2200 900 fade q 0.001 0.22 0.2 gain -n -5
sox -m "$tmp/baque.wav" "$tmp/racha.wav" "$tmp/impacto.wav" gain -n -1

for n in canhao agua impacto; do
    sox "$tmp/$n.wav" -C 3 "$out/$n.ogg"
    echo "$out/$n.ogg $(soxi -D "$tmp/$n.wav")s $(wc -c < "$out/$n.ogg") bytes"
done
rm -rf "$tmp"
