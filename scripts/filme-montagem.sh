#!/usr/bin/env bash
# Montage of the ad films (docs/FILME.md): the best shots of each scene,
# joined with quick crossfades, ~45 s.
#
#   scripts/filme-montagem.sh <dir> <size> [out.mp4]
#
# <dir> holds the scene films tempest-<scene>-<size>.mp4 (MMO_FILME output);
# the montage goes to <dir>/tempest-montagem-<size>.mp4 unless given.
set -euo pipefail
dir=${1:?dir}
tam=${2:?size, e.g. 16x9}
out=${3:-$dir/tempest-montagem-$tam.mp4}
# scene:start:seconds — each scene's best stretch.
cortes=(
  planalto:2.0:6.0
  kogen:5.5:5.5
  mar:10.5:6.0
  celeste:1.0:6.0
  abissal:4.0:5.5
  chefe:3.0:5.5
  geleira:10.5:5.0
  auras:4.0:5.5
  bosque:12.0:5.0
)
fade=0.4
args=()
filtros=""
n=0
offset=0
anterior=""
for c in "${cortes[@]}"; do
  IFS=: read -r cena ini dur <<<"$c"
  f="$dir/tempest-$cena-$tam.mp4"
  [ -f "$f" ] || { echo "skip $cena (no $f)"; continue; }
  args+=(-ss "$ini" -t "$dur" -i "$f")
  filtros+="[$n:v]settb=AVTB,setpts=PTS-STARTPTS,fps=60,format=yuv420p[v$n];"
  if [ -z "$anterior" ]; then
    anterior="v$n"
    offset=$(echo "$dur - $fade" | bc -l)
  else
    filtros+="[$anterior][v$n]xfade=transition=fade:duration=$fade:offset=$offset[x$n];"
    anterior="x$n"
    offset=$(echo "$offset + $dur - $fade" | bc -l)
  fi
  n=$((n + 1))
done
[ "$n" -gt 0 ] || { echo "no films in $dir"; exit 1; }
ffmpeg -y -loglevel error "${args[@]}" -filter_complex "${filtros%;}" -map "[$anterior]" -an \
  -c:v libx264 -preset slow -crf 20 -pix_fmt yuv420p -r 60 -movflags +faststart "$out"
echo "$out"
