#!/usr/bin/env python3
"""Compila e liga os shaders reais da água como GLSL ES 1.00.

Requer glslangValidator no PATH. Detecta erros que os testes de geometria
Rust não enxergam, como macros multilinha recusadas por drivers Android.
"""
import pathlib
import re
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
source = (root / 'crates/client/src/agua.rs').read_text()
with tempfile.TemporaryDirectory(prefix='tempest-water-') as directory:
    shaders = []
    for name, suffix in [('VERTICE', 'vert'), ('FRAGMENTO', 'frag')]:
        match = re.search(r'const ' + name + r': &str = r#"(.*?)"#;', source, re.S)
        if match is None:
            raise SystemExit(f'Shader {name} não encontrado')
        path = pathlib.Path(directory) / f'agua.{suffix}'
        path.write_text(match.group(1))
        shaders.append(str(path))
    subprocess.run(['glslangValidator', '-l', *shaders], check=True)
    # Exercita também GLES2 sem highp no fragmento e o caminho desktop.
    condition = 'defined(GL_FRAGMENT_PRECISION_HIGH) || !defined(GL_ES)'
    original = [pathlib.Path(p).read_text() for p in shaders]
    for high_precision in (False, True):
        for path, shader in zip(shaders, original):
            pathlib.Path(path).write_text(shader.replace(condition, str(int(high_precision))))
        subprocess.run(['glslangValidator', '-l', *shaders], check=True)
