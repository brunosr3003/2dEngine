#!/usr/bin/env python3
"""Cada mudança do cliente exige notas novas antes de empacotar uma release.

Edite docs/PATCHNOTES.txt e execute este script com --seal. Os scripts de
build validam o selo; recompilar o mesmo código mantém as mesmas notas.
"""
from pathlib import Path
import hashlib
import json
import sys

root = Path(__file__).resolve().parents[1]
notes = root / 'docs/PATCHNOTES.txt'
seal = root / 'docs/patchnotes-release.json'
text = notes.read_text().strip()
if len(text.splitlines()) < 3 or len(text) < 80:
    raise SystemExit('Escreva a data e as novidades da release em docs/PATCHNOTES.txt.')
files = [root / 'Cargo.toml', root / 'Cargo.lock']
for directory in ('crates/client', 'crates/shared', 'vendor/miniquad', 'assets'):
    files.extend(p for p in (root / directory).rglob('*') if p.is_file()
        and ('target' not in p.parts) and (directory == 'assets' or p.suffix in ('.rs', '.toml')))
h = hashlib.sha256()
for path in sorted(files):
    h.update(path.relative_to(root).as_posix().encode() + b'\0' + path.read_bytes() + b'\0')
current = {'source_sha256': h.hexdigest(), 'notes_sha256': hashlib.sha256(notes.read_bytes()).hexdigest()}
old = json.loads(seal.read_text()) if seal.exists() else {}
if '--seal' in sys.argv:
    if old and old['source_sha256'] != current['source_sha256'] and old['notes_sha256'] == current['notes_sha256']:
        raise SystemExit('O cliente mudou. Atualize o texto dos patch notes antes de selar a release.')
    seal.write_text(json.dumps(current, indent=2) + '\n')
    print('Patch notes vinculados à versão atual do cliente.')
elif old != current:
    raise SystemExit('Build bloqueada: atualize docs/PATCHNOTES.txt e rode python3 scripts/check-release-notes.py --seal.')
else:
    print('Patch notes desta build verificados.')
