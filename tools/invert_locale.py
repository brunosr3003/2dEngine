#!/usr/bin/env python3
"""Step 2 of the locale inversion: rewrite Portuguese literals to English.

    python3 tools/invert_locale.py --dry-run   # count only, writes nothing
    python3 tools/invert_locale.py             # does it

The existing dictionary IS the map. `idioma/en/*` holds 2,307 (pt, en) pairs
and 99% of the Portuguese keys appear verbatim as quoted literals in the
source, so the rewrite is mechanical.

Two things make it safe:

  * it replaces the FULL QUOTED TOKEN. `"Voltar"` carries its own quotes, so
    `"Voltar agora"` does not contain it and no entry can corrupt another.
  * it skips `idioma/`, so the dictionary is not rewritten by its own map.

The tree will NOT compile straight after this, and that is expected: the
dictionary still points PT -> EN while the source has become English. Steps
3-5 (flip the dictionary to `idioma/pt/*`, make `Idioma::En` the identity,
migrate the seeded names) put it back together. See docs/TRANSLATION.md.
"""
import collections
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
DICT = ROOT / 'crates/shared/src/idioma/en'
PAIR = re.compile(r'\(\s*("(?:[^"\\]|\\.)*")\s*,\s*("(?:[^"\\]|\\.)*")\s*\)')


def pairs():
    out = []
    for f in sorted(DICT.glob('*.rs')):
        out += PAIR.findall(f.read_text(encoding='utf-8'))
    return out


def main():
    dry = '--dry-run' in sys.argv
    # Longest first. Full-token matching already rules out collisions; this
    # only makes the order deterministic between runs.
    mapa = sorted(pairs(), key=lambda p: -len(p[0]))
    if not mapa:
        sys.exit('no pairs found — is idioma/en still there?')
    sources = [p for p in ROOT.joinpath('crates').rglob('*.rs')
               if 'target' not in p.parts and 'idioma' not in p.as_posix()]
    total, touched = 0, collections.Counter()
    for p in sources:
        s = original = p.read_text(encoding='utf-8')
        for pt, en in mapa:
            if pt in s:
                n = s.count(pt)
                total += n
                touched[p.relative_to(ROOT).as_posix()] += n
                s = s.replace(pt, en)
        if s != original and not dry:
            p.write_text(s, encoding='utf-8')
    verb = 'would swap' if dry else 'swapped'
    print(f'{verb} {total} literals across {len(touched)} files '
          f'(from {len(mapa)} dictionary pairs)')
    for f, n in touched.most_common(10):
        print(f'  {n:5}  {f}')
    if dry:
        print('\n--dry-run: nothing was written.')


if __name__ == '__main__':
    main()
