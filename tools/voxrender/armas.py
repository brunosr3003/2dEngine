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


# ── o segundo, o terceiro e o quarto conjunto ──
PRETO, BRANCO, LACA, LACA_ESC, LATAO, MADEIRA_ESC2 = range(11, 17)
cores.update({
    PRETO: (32, 30, 34), BRANCO: (228, 224, 214), LACA: (120, 28, 34), LACA_ESC: (78, 16, 22),
    LATAO: (196, 150, 64), MADEIRA_ESC2: (92, 56, 32),
})


def katana():
    """30 (Y) x 3 (Z) x 2 (X): cabo 8 com a trama preta e branca, tsuba de
    1, lamina de 21 com o fio claro embaixo, o dorso escuro em cima e a
    curva (sori) subindo um voxel no ultimo terco."""
    v = {}
    for y in range(0, 8):
        for x in range(2):
            v[(x, y, 1)] = PRETO if (y + x) % 2 else BRANCO      # tsuka-ito
    v[(0, 0, 1)] = T[2]                                         # kashira
    for z in range(3):
        for x in range(2):
            v[(x, 8, z)] = T[1]                                  # tsuba
    for y in range(9, 30):
        sori = 1 if y >= 22 else 0
        if y <= 27:
            v[(0, y, 1 + sori)] = ACO_FIO                         # fio
            v[(0, y, 2 + sori if 2 + sori < 3 else 2)] = ACO_ESC  # dorso
        else:
            v[(0, y, 1 + sori)] = ACO_FIO                         # a ponta afina
    v[(1, 3, 1)] = MARCA                                          # meio do cabo
    return v, (2, 30, 3)


def bainha():
    """24 (Y) x 3 (Z) x 2 (X): laca vermelha escura, a boca (koiguchi) e a
    ponta (kojiri) na cor do tier. O marcador fica na BOCA: e' por ali que a
    lamina entra, e o jogo prende a bainha no quadril por esse ponto."""
    v = {}
    for y in range(1, 24):
        for z in range(3):
            for x in range(2):
                v[(x, y, z)] = LACA if (z + y // 6) % 2 else LACA_ESC
    for z in range(3):
        for x in range(2):
            v[(x, 1, z)] = T[1]                                    # koiguchi
            v[(x, 23, z)] = T[2]                                   # kojiri
    v[(0, 0, 1)] = MARCA
    return v, (2, 24, 3)


def pistola():
    """11 (Y) x 6 (Z) x 2 (X): pistola de pederneira, de pirata. Cano de aco
    pra +Y, o fecho em latao, a coronha de madeira descendo curva pra tras
    (a pega), e o marcador no meio dela."""
    v = {}
    for y in range(3, 11):
        v[(0, y, 4)] = ACO_ESC                                     # cano
        v[(0, y, 5)] = ACO
    v[(0, 10, 5)] = FERRO                                          # boca
    for y in range(0, 4):
        for z in range(3, 6):
            v[(0, y, z)] = MADEIRA_ESC2                            # corpo
    v[(1, 2, 4)] = LATAO                                           # fecho
    v[(1, 3, 5)] = LATAO                                           # cao
    for (y, z) in [(1, 2), (0, 2), (0, 1), (1, 1), (0, 0)]:
        v[(0, y, z)] = MADEIRA_ESC2                                 # coronha
    v[(0, 0, 0)] = T[1]                                            # coice de latao
    v[(0, 3, 2)] = LATAO                                           # guarda-mato
    v[(0, 1, 2)] = MARCA                                           # meio da pega
    return v, (2, 11, 6)


def coldre():
    """4 (Y) x 6 (Z) x 3 (X): bolsa de couro. O comprimento (Y) e' a
    profundidade — o jogo pendura o coldre com a boca pra cima e o Y
    descendo. O marcador fica na boca."""
    v = {}
    for y in range(1, 9):
        for z in range(1, 5):
            for x in range(3):
                if x in (0, 2) or z in (1, 4) or y == 8:
                    v[(x, y, z)] = COURO if y % 3 else COURO_ESC
    for z in range(1, 5):
        v[(1, 1, z)] = T[1]                                        # o friso da boca
    v[(1, 0, 2)] = MARCA
    return v, (3, 9, 6)


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "personagem")
    for nome, (vox, tam) in [("espada", espada()), ("escudo", escudo()), ("katana", katana()),
                             ("bainha", bainha()), ("pistola", pistola()), ("coldre", coldre())]:
        escrever_vox(os.path.join(pasta, f"{nome}.vox"), vox, tam, paleta())
        print(f"{nome}.vox: {len(vox)} voxels, tela {tam[0]}x{tam[1]}x{tam[2]}")
