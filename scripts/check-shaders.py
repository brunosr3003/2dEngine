#!/usr/bin/env python3
"""Valida os pares GLSL ES do cliente antes de gerar um APK.

Lê o código real, incluindo shaders inline. A ligação verifica também a
compatibilidade entre vertex e fragment. Requer glslangValidator no PATH.
"""
import pathlib
import re
import shutil
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
validator = shutil.which('glslangValidator')
if validator is None:
    raise SystemExit('Instale glslangValidator para validar os shaders antes do build Android.')

count = 0
with tempfile.TemporaryDirectory(prefix='tempest-shaders-') as directory:
    for source in sorted((root / 'crates/client/src').rglob('*.rs')):
        blocks = re.findall(r'r#"(#version\b.*?)"#', source.read_text(), re.S)
        if not blocks:
            continue
        if len(blocks) % 2:
            raise SystemExit(f'{source}: shader sem par vertex/fragment; revise a validação')
        for index in range(0, len(blocks), 2):
            vertex, fragment = blocks[index:index + 2]
            if 'gl_Position' not in vertex or 'gl_FragColor' not in fragment:
                raise SystemExit(f'{source}: ordem vertex/fragment desconhecida; revise a validação')
            paths = []
            for block, suffix in [(vertex, 'vert'), (fragment, 'frag')]:
                path = pathlib.Path(directory) / f'{source.stem}-{index // 2}.{suffix}'
                path.write_text(block)
                paths.append(str(path))
            subprocess.run([validator, '-l', *paths], check=True)
            count += 1
if count == 0:
    raise SystemExit('Nenhum shader encontrado; validação não executada')
print(f'{count} pares de shaders do cliente compilados e ligados com sucesso.')
