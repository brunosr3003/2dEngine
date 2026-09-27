#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/check-release-notes.py
python3 scripts/check-shaders.py
python3 scripts/check-water-shaders.py
export MMO_API_PADRAO="${MMO_API_PADRAO:-mmo.brunji.com.br:80}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/windows-build}"
mingw="$HOME/.local/share/tempest-mingw/usr/bin"
if [[ -d "$mingw" ]]; then export PATH="$mingw:$PATH"; fi
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo build --release --target x86_64-pc-windows-gnu --bin client
python3 - "$CARGO_TARGET_DIR" <<'PY'
import pathlib, shutil, subprocess, sys, zipfile
root = pathlib.Path.cwd()
out = root / 'target/windows'
app = out / 'Tempest'
app.mkdir(parents=True, exist_ok=True)
shutil.copy2(pathlib.Path(sys.argv[1]) / 'x86_64-pc-windows-gnu/release/client.exe', app / 'Tempest.exe')
shutil.copytree(root / 'assets', app / 'assets', dirs_exist_ok=True)
# Distribui runtimes MinGW caso o linker os importe.
for dll in ('libwinpthread-1.dll', 'libgcc_s_seh-1.dll'):
    path = subprocess.check_output(['x86_64-w64-mingw32-gcc', '-print-file-name=' + dll], text=True).strip()
    if pathlib.Path(path).is_file(): shutil.copy2(path, app / dll)
(app / 'LEIA-ME.txt').write_text('TEMPEST — WINDOWS 64 BITS\n\nExtraia o ZIP inteiro e abra Tempest.exe. Mantenha assets junto do executável.\n\nWASD: mover | Mouse direito + arrastar: câmera | Roda: zoom\n1/2/3: habilidades | Espaço: ataque | R: pular | G: defender\n\nSuporte: https://mmo.brunji.com.br/support.html\n', encoding='utf-8')
with zipfile.ZipFile(out / 'MMORPG-Windows.zip', 'w', zipfile.ZIP_DEFLATED) as z:
    for p in sorted(app.rglob('*')):
        if p.is_file(): z.write(p, p.relative_to(out))
print(out / 'MMORPG-Windows.zip')
PY
