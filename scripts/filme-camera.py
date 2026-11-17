#!/usr/bin/env python3
"""Frame-to-frame check of the ad capture's camera (docs/FILME.md).

    scripts/filme-camera.py camera.csv

Reads the MMO_FILME_CAMERA file and, per shot, reports the eye's lowest
clearance over the ground and its worst "kink": the second difference of the
eye position (|p[i+1] - 2 p[i] + p[i-1]|) against the shot's median. A smooth
path has a second difference that changes slowly; a jump (the eye or target
riding a block edge) shows as a spike many times the median. The same is done
for the eye the old code drew (`v*` columns) as the before/after.
"""
import csv
import math
import sys
from collections import defaultdict


def picos(pts):
    d2 = [math.dist((0, 0, 0), tuple(pts[i + 1][k] - 2 * pts[i][k] + pts[i - 1][k] for k in range(3))) for i in range(1, len(pts) - 1)]
    if not d2:
        return 0.0, 0.0, 0
    med = sorted(d2)[len(d2) // 2]
    pior = max(d2)
    # A spike: over 1 cm of kink and over 8x the shot's median.
    n = sum(1 for x in d2 if x > 0.01 and x > 8 * max(med, 1e-6))
    return med, pior, n


def main():
    tomadas = defaultdict(list)
    for r in csv.DictReader(open(sys.argv[1])):
        if None in r.values():
            continue  # a file still being written: the last row is cut
        tomadas[(r["cena"], int(r["tomada"]))].append(r)
    print(f"{'shot':14} {'frames':>6} {'min clear':>9} | {'new: med kink':>13} {'worst':>7} {'spikes':>6} | {'old: worst':>10} {'spikes':>6}  {'old max dy/frame':>16}")
    for (cena, t), rs in sorted(tomadas.items()):
        f = lambda r, p: (float(r[p + "x"]), float(r[p + "y"]), float(r[p + "z"]))
        novo = [f(r, "o") for r in rs]
        velho = [f(r, "v") for r in rs]
        folga = min(float(r["folga"]) for r in rs)
        mn, pn, nn = picos(novo)
        mv, pv, nv = picos(velho)
        dy = max((abs(velho[i][1] - velho[i - 1][1]) for i in range(1, len(velho))), default=0.0)
        print(f"{cena}:{t:<8} {len(rs):6} {folga:9.2f} | {mn:13.4f} {pn:7.3f} {nn:6} | {pv:10.3f} {nv:6}  {dy:16.3f}")


if __name__ == "__main__":
    main()
