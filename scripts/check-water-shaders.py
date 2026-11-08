#!/usr/bin/env python3
"""Compila e liga os shaders reais que têm fallback de precisão GLES2 (água e
mundo sólido) como GLSL ES 1.00.

Requer glslangValidator no PATH. Detecta erros que os testes de geometria
Rust não enxergam, como macros multilinha recusadas por drivers Android, e
testa os dois lados do `#if defined(GL_FRAGMENT_PRECISION_HIGH) ...`: sem
isso a validação normal (sempre com highp disponível, como no desktop) não
pega o fragmento mediump que só aparece no driver Adreno/Mali de verdade.
"""
import pathlib
import re
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
# (arquivo, prefixo do shader, nome da constante de vertex, nome da de fragment)
ARQUIVOS = [
    ('crates/client/src/agua.rs', 'agua', 'VERTICE', 'FRAGMENTO'),
    ('crates/client/src/render3d.rs', 'render3d', 'SOLIDO_VERTICE', 'SOLIDO_FRAGMENTO'),
]
CONDICAO = 'defined(GL_FRAGMENT_PRECISION_HIGH) || !defined(GL_ES)'

with tempfile.TemporaryDirectory(prefix='tempest-water-') as directory:
    for rel, prefixo, nome_vert, nome_frag in ARQUIVOS:
        source = (root / rel).read_text()
        shaders = []
        for name, suffix in [(nome_vert, 'vert'), (nome_frag, 'frag')]:
            match = re.search(r'const ' + name + r': &str = r#"(.*?)"#;', source, re.S)
            if match is None:
                raise SystemExit(f'{rel}: shader {name} não encontrado')
            path = pathlib.Path(directory) / f'{prefixo}.{suffix}'
            path.write_text(match.group(1))
            shaders.append(str(path))
        subprocess.run(['glslangValidator', '-l', *shaders], check=True)
        # Exercita também GLES2 sem highp no fragmento e o caminho desktop.
        original = [pathlib.Path(p).read_text() for p in shaders]
        for high_precision in (False, True):
            for path, shader in zip(shaders, original):
                pathlib.Path(path).write_text(shader.replace(CONDICAO, str(int(high_precision))))
            subprocess.run(['glslangValidator', '-l', *shaders], check=True)
        print(f'{rel}: ok (highp e fallback mediump)')
