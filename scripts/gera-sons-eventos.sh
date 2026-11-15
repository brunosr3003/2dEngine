#!/usr/bin/env bash
# Synthesizes the new UI/event sounds with sox (no third-party samples, no
# license to track) and encodes them to Ogg Vorbis with ffmpeg.
# Usage: scripts/gera-sons-eventos.sh assets/audio/eventos   (needs sox with Vorbis)
set -euo pipefail
out="${1:?output dir}"
tmp="$(mktemp -d)"
mkdir -p "$out"
R=44100

# A bell-ish note: sine + a quieter octave + a touch of the fifth, quick attack, long decay.
nota() { # name freq dur
    sox -n -r $R -c 1 "$tmp/$1.wav" synth "$3" sine "$2" sine "$(echo "$2*2" | bc)" sine "$(echo "$2*3" | bc)" \
        remix 1v0.6,2v0.25,3v0.08 fade q 0.004 "$3" "$(echo "$3*0.85" | bc)" gain -n -3
}

# ── level up: C5 E5 G5 C6 arpeggio, then a sustained C major chord with a shimmer ──
nota a1 523.25 0.30; nota a2 659.25 0.30; nota a3 783.99 0.30; nota a4 1046.5 0.55
sox -n -r $R -c 1 "$tmp/acorde.wav" synth 1.4 sine 523.25 sine 659.25 sine 783.99 sine 1046.5 \
    remix 1v0.35,2v0.3,3v0.3,4v0.25 tremolo 7 25 fade q 0.02 1.4 1.1 gain -n -4
sox -n -r $R -c 1 "$tmp/brilho.wav" synth 1.2 sine 2093 sine 3136 remix 1v0.12,2v0.08 tremolo 11 60 fade q 0.05 1.2 1.0 gain -n -14
sox "$tmp/a1.wav" "$tmp/a1p.wav" pad 0 1.6
sox "$tmp/a2.wav" "$tmp/a2p.wav" pad 0.09 1.5
sox "$tmp/a3.wav" "$tmp/a3p.wav" pad 0.18 1.4
sox "$tmp/a4.wav" "$tmp/a4p.wav" pad 0.27 1.1
sox "$tmp/acorde.wav" "$tmp/acordep.wav" pad 0.42 0
sox "$tmp/brilho.wav" "$tmp/brilhop.wav" pad 0.55 0.2
sox -m "$tmp/a1p.wav" "$tmp/a2p.wav" "$tmp/a3p.wav" "$tmp/a4p.wav" "$tmp/acordep.wav" "$tmp/brilhop.wav" "$tmp/subir_nivel.wav" \
    reverb 40 50 60 gain -n -1

# ── quest complete: "ta-da" (G5 short, C6 long) ──
nota q1 783.99 0.16; nota q2 1046.5 0.6
sox "$tmp/q2.wav" "$tmp/q2p.wav" pad 0.12 0
sox -m "$tmp/q1.wav" "$tmp/q2p.wav" "$tmp/missao.wav" reverb 30 gain -n -2

# ── chest open: low thump + rising sparkle ──
sox -n -r $R -c 1 "$tmp/baque.wav" synth 0.25 sine 90:45 fade q 0.002 0.25 0.2 gain -n -2
sox -n -r $R -c 1 "$tmp/faisca.wav" synth 0.6 sine 1200:2600 tremolo 18 70 fade q 0.01 0.6 0.45 gain -n -10
sox "$tmp/faisca.wav" "$tmp/faiscap.wav" pad 0.08 0
sox -m "$tmp/baque.wav" "$tmp/faiscap.wav" "$tmp/bau.wav" reverb 20 gain -n -2

# ── energy gather: soft rising whoosh with a shimmer ──
sox -n -r $R -c 1 "$tmp/energia.wav" synth 0.45 sine 400:1400 sine 800:2800 remix 1v0.5,2v0.2 \
    tremolo 22 40 fade q 0.02 0.45 0.3 reverb 25 gain -n -4

# ── refine ok: two bright pings; refine fail: descending low buzz ──
nota r1 1318.5 0.18; nota r2 1760 0.45
sox "$tmp/r2.wav" "$tmp/r2p.wav" pad 0.1 0
sox -m "$tmp/r1.wav" "$tmp/r2p.wav" "$tmp/refino_ok.wav" reverb 25 gain -n -2
sox -n -r $R -c 1 "$tmp/refino_falha.wav" synth 0.55 sawtooth 220:110 lowpass 900 fade q 0.01 0.55 0.35 gain -n -6

# ── mail: soft bell ding ──
nota c1 1174.66 0.7
sox "$tmp/c1.wav" "$tmp/correio.wav" reverb 30 gain -n -3

# ── craft: metallic hit (inharmonic partials, short) ──
sox -n -r $R -c 1 "$tmp/craft.wav" synth 0.4 sine 620 sine 1430 sine 2310 remix 1v0.5,2v0.3,3v0.2 \
    fade q 0.001 0.4 0.37 reverb 15 gain -n -3

for n in subir_nivel missao bau energia refino_ok refino_falha correio craft; do
    sox "$tmp/$n.wav" -C 5 "$out/$n.ogg"
    echo "$out/$n.ogg $(soxi -D "$tmp/$n.wav")s"
done
rm -rf "$tmp"
