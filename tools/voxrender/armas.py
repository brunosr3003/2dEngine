#!/usr/bin/env python3
"""As armas do primeiro conjunto: espada e escudo.

Medidas e convencoes em `docs/ARTE_DO_PERSONAGEM.md`, secao "Skins de arma":

* orientacao de quem segura com o braco caido: cabo ao longo do Y, lamina
  pra +Y (frente);
* um voxel MAGENTA (255) no centro da pega, onde a mao fecha. O jogo le' esse
  ponto como o encaixe e apaga o voxel;
* 1,25x o tamanho real — de cima e numa tela de celular, arma em tamanho real
  vira um risco;
* frisos em 241-244: a cor sai do tier do item EQUIPADO, nao da skin.

Estas sao as skins PADRAO. Uso:  python3 tools/voxrender/armas.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voxsimplify import escrever_vox  # noqa: E402
import molde_corpo as M  # noqa: E402

T, MARCA = M.TIER, M.MARCA
ACO, ACO_ESC, ACO_FIO, COURO, COURO_ESC, MADEIRA, MADEIRA_ESC, PINTURA, PINTURA_CLARA, FERRO = range(1, 11)
cores = {
    ACO: (196, 204, 214), ACO_ESC: (140, 150, 164), ACO_FIO: (232, 238, 244),
    COURO: (92, 58, 34), COURO_ESC: (64, 40, 24),
    MADEIRA: (132, 92, 56), MADEIRA_ESC: (104, 70, 42),
    PINTURA: (38, 66, 124), PINTURA_CLARA: (226, 222, 206), FERRO: (82, 86, 94),
    # os frisos, na rampa cinza (tier 1): o cliente troca pela cor do tier
    T[0]: (214, 220, 228), T[1]: (176, 182, 190), T[2]: (138, 144, 152), T[3]: (100, 104, 112),
    MARCA: (255, 0, 255),
}


def paleta():
    p = [(0, 0, 0, 0)] + [(0, 0, 0, 255)] * 255
    for i, (r, g, b) in cores.items():
        p[i] = (r, g, b, 255)
    return p


def espada():
    """26 (Y) x 7 (Z) x 3 (X): pomo, cabo 5, guarda 1 com 7 de largura,
    lamina 19 com o fio claro e o sulco escuro no meio."""
    v = {}
    for x in range(3):
        for z in range(2, 5):
            v[(x, 0, z)] = T[1]                           # pomo
    v[(1, 0, 3)] = T[0]
    for y in range(1, 6):
        v[(1, y, 3)] = COURO if y % 2 else COURO_ESC      # cabo enrolado
        v[(1, y, 2)] = COURO_ESC
        v[(1, y, 4)] = COURO_ESC
    for z in range(7):
        v[(1, 6, z)] = T[2] if z in (0, 6) else T[1]      # guarda
    for x in (0, 2):
        for z in range(2, 5):
            v[(x, 6, z)] = T[1]
    for y in range(7, 26):
        if y <= 22:
            v[(1, y, 2)] = ACO_FIO
            v[(1, y, 4)] = ACO_FIO
            v[(1, y, 3)] = ACO_ESC if 8 <= y <= 19 else ACO
        else:
            v[(1, y, 3)] = ACO_FIO                         # a ponta afina
    v[(1, 23, 2)] = ACO
    v[(1, 3, 3)] = MARCA                                   # meio do cabo
    return v, (3, 26, 7)


def escudo():
    """14 (Y) x 14 (Z) x 3 (X), redondo. A face pintada olha pro -X (o lado
    de fora do braco esquerdo); aro de metal na cor do tier; alca de couro
    atras, com o marcador nela."""
    v = {}
    c = 6.5
    for y in range(14):
        for z in range(14):
            d = ((y - c) ** 2 + (z - c) ** 2) ** 0.5
            if d > 7.0:
                continue
            aro = d > 5.7
            v[(1, y, z)] = T[1] if aro else (MADEIRA_ESC if y % 3 == 0 else MADEIRA)
            if aro:
                v[(0, y, z)] = T[0] if (y + z) % 2 else T[1]
            elif d <= 1.6:
                v[(0, y, z)] = FERRO                        # umbo
            elif abs(z - c) <= 1.0:
                v[(0, y, z)] = PINTURA_CLARA                # a faixa
            else:
                v[(0, y, z)] = PINTURA
    for y in (6, 7):
        for z in range(3, 11):
            v[(2, y, z)] = COURO                            # alca
    v[(2, 6, 7)] = MARCA
    return v, (3, 14, 14)


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "personagem")
    for nome, (vox, tam) in [("espada", espada()), ("escudo", escudo())]:
        escrever_vox(os.path.join(pasta, f"{nome}.vox"), vox, tam, paleta())
        print(f"{nome}.vox: {len(vox)} voxels, tela {tam[0]}x{tam[1]}x{tam[2]}")
