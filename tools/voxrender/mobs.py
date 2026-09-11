#!/usr/bin/env python3
"""Os modelos das CRIATURAS, tirados do estoque do zone14.

Cada bicho passa por duas etapas:

1. **reduzir ate' caber no orcamento de faces** (`reduzir`). Mob comum
   aparece as dezenas; o modelo detalhado e' de CHEFE (docs/PIPELINE_ARTE.md);
2. **encostar no canto** e dimensionar a caixa pelo que sobrou.

O chefe (`lobo`) nao passa por nenhuma: ele e' o unico na tela e pode ter o
detalhe inteiro.

**As patas SOLTAS sao o estilo, nao defeito.** O `generate_wolf.py` do zone14
diz "SOMENTE AS PATAS FLUTUANTES DE LOBO, SEM PERNAS" e deixa de proposito um
vao de ar entre pata e tronco; tigre e urso seguem o mesmo desenho. Uma
primeira versao deste script "consertava" isso subindo as patas ate' o corpo —
e mudava o desenho dos bichos (o lobo nem encaixava: as patas dele ficam do
LADO do tronco, nao embaixo). Aqui a forma passa intacta.

Uso:  python3 tools/voxrender/mobs.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voxrender import parse_vox  # noqa: E402
from voxsimplify import reduzir, escrever_vox  # noqa: E402

ZONE14 = os.path.expanduser("~/zone14")

# (arquivo no jogo, fonte no zone14)
#
# O TAMANHO na tela nao se decide aqui: o cliente desenha cada bicho na altura
# dele (`render3d::altura_do_modelo`), calculando a escala pela altura do
# proprio arquivo. Aqui se decide so' a RESOLUCAO — e ela sai do orcamento de
# faces, nao da altura. A primeira versao reduzia ate' uma altura-alvo e o
# urso saiu com 8 mil faces, cinco vezes o lobo pequeno antigo.
CRIATURAS = [
    ("lobo_pequeno", "lobo.vox"),          # Grunt
    ("urso",         "urso.vox"),          # Tank
    ("tigre",        "tigre.vox"),         # Ninja
    ("owlbear",      "urso_coruja.vox"),   # Berserker
]

# Faces expostas por mob comum, no maximo. O lobo pequeno antigo tinha 1.788
# e virava 1.324 triangulos depois do greedy; mob aparece as dezenas.
ORCAMENTO_FACES = 2400
CHEFE = ("lobo", "lobo.vox")

def origem(voxels):
    mx = min(k[0] for k in voxels); my = min(k[1] for k in voxels); mz = min(k[2] for k in voxels)
    v = {(x - mx, y - my, z - mz): c for (x, y, z), c in voxels.items()}
    return v, tuple(max(k[i] for k in v) + 1 for i in range(3))

def carrega(fonte):
    return max(parse_vox(os.path.join(ZONE14, fonte)), key=lambda x: len(x.voxels))

def faces(v):
    s = set(v)
    return sum(1 for (x, y, z) in s for d in ((1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1))
               if (x + d[0], y + d[1], z + d[2]) not in s)

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox")
    for nome, fonte in CRIATURAS:
        m = carrega(fonte)
        for fator in range(2, 12):
            v, size = origem(reduzir(m, fator))
            if faces(v) <= ORCAMENTO_FACES: break
        assert faces(v) <= ORCAMENTO_FACES, (nome, faces(v))
        escrever_vox(os.path.join(pasta, f"{nome}.vox"), v, size, m.palette)
        print(f"{nome:13} {fonte:16} fator {fator} -> caixa {size}  {len(v):5} voxels  {faces(v):5} faces")
    nome, fonte = CHEFE
    m = carrega(fonte)
    v, size = origem(m.voxels)
    escrever_vox(os.path.join(pasta, f"{nome}.vox"), v, size, m.palette)
    print(f"{nome:13} {fonte:16} (chefe, sem reduzir) caixa {size}  {len(v)} voxels")
