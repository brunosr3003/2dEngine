#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/check-release-notes.py
python3 scripts/check-shaders.py
export MMO_API_PADRAO="${MMO_API_PADRAO:-mmo.brunji.com.br:80}"
cargo build --release --bin client
python3 - <<'PY'
import pathlib, shutil, zipfile
root = pathlib.Path.cwd()
out = root / 'target/linux'
app = out / 'Tempest'
app.mkdir(parents=True, exist_ok=True)
shutil.copy2(root / 'target/release/client', app / 'Tempest')
shutil.copytree(root / 'assets', app / 'assets', dirs_exist_ok=True)
(app / 'LEIA-ME.txt').write_text('TEMPEST — LINUX x86_64\n\nExtraia o ZIP e execute ./Tempest na pasta extraída.\nSe necessário: chmod +x Tempest\nRequer ambiente gráfico com OpenGL e ALSA (libasound).\n\nWASD: mover | Mouse direito: câmera | Roda: zoom\n1/2/3: habilidades | Espaço: ataque | R: pular | G: defender\n', encoding='utf-8')
with zipfile.ZipFile(out / 'MMORPG-Linux.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for p in sorted(app.rglob('*')):
        if p.is_file(): z.write(p, p.relative_to(out))
print(out / 'MMORPG-Linux.zip')
PY
