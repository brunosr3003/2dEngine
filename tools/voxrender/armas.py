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
import math, os, sys
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


# ── as ferramentas de coleta ──
# Mesma convencao das armas: cabo ao longo do Y, a cabeca pra +Y, o marcador
# no meio da pega. A picareta sai em QUATRO arquivos, um por cor de pedra: a
# cabeca e' da cor do veio que se esta' minerando (cinza, verde, azul, roxa),
# e quem esta' de fora le' de longe o que o outro minera.
CABO, CABO_ESC, CUNHA, CUNHA_ESC, CUNHA_FIO = range(17, 22)
cores.update({
    CABO: (150, 104, 62), CABO_ESC: (112, 76, 44),
    CUNHA: (170, 176, 186), CUNHA_ESC: (120, 126, 136), CUNHA_FIO: (226, 232, 238),
})
COR_DA_PEDRA = {
    1: ((150, 150, 156), (110, 110, 118)),   # cinza
    2: ((86, 170, 96), (56, 120, 66)),       # verde
    3: ((74, 124, 214), (48, 84, 160)),      # azul
    4: ((156, 94, 206), (108, 60, 150)),     # roxa
}
PICO, PICO_ESC = 22, 23


def machado():
    """20 (Y) x 7 (Z) x 2 (X): cabo de madeira com fita de couro na pega, a
    cunha de aco atravessada no alto com o fio claro na frente (+Z)."""
    v = {}
    for y in range(0, 17):
        for x in range(2):
            v[(x, y, 2)] = CABO if (y // 3) % 2 else CABO_ESC
    for y in range(1, 5):
        v[(0, y, 2)] = COURO                                     # fita da pega
    for y in range(13, 19):
        for z in range(0, 7):
            if z <= 1 and (y in (13, 18)):
                continue                                          # a cunha afina atras
            cor = CUNHA_FIO if z >= 5 else (CUNHA_ESC if z <= 1 else CUNHA)
            v[(0, y, z)] = cor
            v[(1, y, z)] = cor
    v[(1, 3, 2)] = MARCA
    return v, (2, 19, 7)


def picareta(tier):
    """20 (Y) x 13 (Z) x 2 (X): cabo de madeira, a cabeca atravessada no alto
    com dois bicos curvos, na cor da pedra do tier."""
    clara, escura = COR_DA_PEDRA[tier]
    cores[PICO], cores[PICO_ESC] = clara, escura
    v = {}
    for y in range(0, 18):
        for x in range(2):
            v[(x, y, 6)] = CABO if (y // 3) % 2 else CABO_ESC
    for y in range(1, 5):
        v[(0, y, 6)] = COURO
    for z in range(0, 13):
        d = abs(z - 6)
        y = 17 - (1 if d >= 5 else 0)                            # os bicos descem
        for x in range(2):
            v[(x, y, z)] = PICO if d <= 4 else PICO_ESC
            if d <= 2:
                v[(x, y + 1, z)] = PICO                           # o olho, mais grosso
    v[(1, 3, 6)] = MARCA
    return v, (2, 19, 13)


def arco():
    """O ARCO do arqueiro, com a corda RETA e FIXA.

    A corda era desenhada puxada pelo gesto do braço — e não havia arco nenhum
    no rig (as dez peças do humanoide saem "sem a arma"), então o arqueiro
    fazia o movimento de puxar no vazio, com o antebraço girando 94°. O dono
    pediu o óbvio: a corda não precisa ser dinâmica, pode ser fixa.

    Reta e fixa também é o que o voxel faz bem — uma corda curva em blocos de
    meio bloco vira escada, e puxada vira escada que se mexe.

    Em pé no eixo Z, o cabo no meio: é por ele que a mão esquerda fecha.
    """
    v = {}
    meio = 13
    for z in range(2, 25):
        d = abs(z - meio)
        # a madeira arqueia pra fora nas pontas
        x = 2 if d < 5 else 1 if d < 9 else 0
        v[(x, 1, z)] = MADEIRA if d < 10 else MADEIRA_ESC
        v[(x, 2, z)] = MADEIRA_ESC
    # A CORDA: uma reta de ponta a ponta, sem puxar.
    for z in range(3, 24):
        v[(0, 1, z)] = PINTURA_CLARA
    # O cabo, no meio, e o marcador da pega dentro dele.
    for z in range(meio - 2, meio + 3):
        v[(2, 1, z)] = COURO
        v[(2, 2, z)] = COURO_ESC
    v[(2, 1, meio)] = MARCA
    return v, (3, 3, 26)


# ── WEAPON SKINS (`shared::aparencia::SKINS_DE_ARMA`) ──
#
# Each skin = the default shapes, a palette of its own and a few voxels that
# change the silhouette (serrations, coils, a sun boss). The grip marker and
# the canvas stay where the default has them: the hands, the sheath and the
# holster were fitted to those, and a skin must fit the same. The tier trim
# (241-244) stays too — the colour of the equipped item's tier still shows.
EXTRA1, EXTRA2, EXTRA3 = 30, 31, 32


def paleta_com(troca):
    antigas = dict(cores)
    cores.update(troca)
    p = paleta()
    cores.clear(); cores.update(antigas)
    return p


def espada_skin(estilo):
    v, tam = espada()
    if estilo == "abissal":
        for y in range(9, 22, 3):                         # serrated edges
            v[(1, y, 1)] = ACO_FIO
            v[(1, y, 5)] = ACO_FIO
    if estilo == "solar":
        v[(0, 6, 3)] = EXTRA1; v[(2, 6, 3)] = EXTRA1      # sun stone in the guard
        v[(1, 7, 3)] = EXTRA1
    if estilo == "real":
        v[(1, 6, 0)] = EXTRA1; v[(1, 6, 6)] = EXTRA1      # gems at the guard's tips
        v[(0, 3, 3)] = EXTRA2; v[(2, 3, 3)] = EXTRA2      # gold grip ring
    return v, tam


def escudo_skin(estilo):
    v, tam = escudo()
    c = 6.5
    for (x, y, z), cor in list(v.items()):
        if x != 0 or cor in (T[0], T[1]):
            continue
        d = ((y - c) ** 2 + (z - c) ** 2) ** 0.5
        if estilo == "solar":
            if d <= 1.6:
                v[(x, y, z)] = EXTRA1                     # the sun
            else:
                raio = int((math.atan2(z - c, y - c) + math.pi) / (math.pi / 6)) % 2
                v[(x, y, z)] = PINTURA_CLARA if raio else PINTURA
        elif estilo == "abissal":
            if d <= 1.2:
                v[(x, y, z)] = EXTRA1                     # the pupil
            elif 2.2 <= d <= 3.3:
                v[(x, y, z)] = EXTRA2                     # the glowing iris
            else:
                v[(x, y, z)] = PINTURA
        elif estilo == "real":
            if abs(y - c) <= 0.6 or abs(z - c) <= 0.6:
                v[(x, y, z)] = EXTRA1                     # gold cross
            else:
                v[(x, y, z)] = PINTURA if (y < c) == (z < c) else PINTURA_CLARA
    return v, tam


def katana_skin(estilo):
    v, tam = katana()
    if estilo == "tempestade":
        for y in range(10, 27):                           # lightning along the flat
            if y % 4 in (0, 1):
                v[(1, y, 1 + (1 if y >= 22 else 0) + (y // 2) % 2 * 0)] = EXTRA1
    if estilo == "lua":
        for y in (9, 10):
            v[(1, y, 1)] = EXTRA1                         # red habaki
    return v, tam


def bainha_skin(estilo):
    v, tam = bainha()
    for (x, y, z), cor in list(v.items()):
        if cor not in (LACA, LACA_ESC):
            continue
        if estilo == "sakura":
            v[(x, y, z)] = LACA_ESC if (y * 7 + z * 3 + x * 5) % 9 == 0 else LACA     # petals
        elif estilo == "tempestade":
            v[(x, y, z)] = LACA_ESC if (y + z * 2) % 7 == 0 else LACA               # streaks
        elif estilo == "lua":
            lua = 9 <= y <= 14 and x == 0 and (y, z) not in ((11, 1), (12, 1), (11, 2), (12, 2))
            v[(x, y, z)] = LACA_ESC if lua and (y in (9, 14) or z != 1) else LACA   # crescent
    return v, tam


def pistola_skin(estilo):
    v, tam = pistola()
    if estilo == "relampago":
        for y in (5, 7, 9):
            v[(1, y, 4)] = EXTRA1; v[(1, y, 5)] = EXTRA1  # coils on the barrel
    if estilo == "dourada":
        v[(1, 6, 5)] = EXTRA1; v[(1, 8, 5)] = EXTRA1      # engraving
    if estilo == "coral":
        v[(1, 1, 3)] = EXTRA1; v[(1, 0, 1)] = EXTRA1      # pearl inlay
    return v, tam


def coldre_skin(estilo):
    v, tam = coldre()
    if estilo == "relampago":
        for z in range(1, 5):
            v[(1, 5, z)] = EXTRA1
    return v, tam


SKINS = {
    # sword and shield
    "solar": ([("espada", espada_skin), ("escudo", escudo_skin)], {
        ACO: (240, 198, 76), ACO_ESC: (196, 136, 40), ACO_FIO: (255, 246, 204),
        COURO: (150, 44, 32), COURO_ESC: (110, 30, 22),
        PINTURA: (226, 132, 36), PINTURA_CLARA: (255, 214, 96), EXTRA1: (255, 250, 220)}),
    "abissal": ([("espada", espada_skin), ("escudo", escudo_skin)], {
        ACO: (66, 38, 96), ACO_ESC: (32, 18, 50), ACO_FIO: (96, 244, 222),
        COURO: (34, 30, 44), COURO_ESC: (22, 20, 30),
        PINTURA: (34, 22, 52), EXTRA1: (10, 8, 14), EXTRA2: (96, 244, 222)}),
    "real": ([("espada", espada_skin), ("escudo", escudo_skin)], {
        ACO: (224, 230, 240), ACO_ESC: (44, 74, 176), ACO_FIO: (250, 252, 255),
        COURO: (40, 62, 146), COURO_ESC: (28, 44, 108),
        PINTURA: (40, 66, 160), PINTURA_CLARA: (236, 236, 240),
        EXTRA1: (232, 190, 70), EXTRA2: (232, 190, 70)}),
    # katana
    "sakura": ([("katana", katana_skin), ("bainha", bainha_skin)], {
        PRETO: (224, 120, 160), BRANCO: (250, 244, 246), ACO_FIO: (255, 226, 236), ACO_ESC: (206, 204, 218),
        LACA: (246, 238, 238), LACA_ESC: (236, 140, 176)}),
    "tempestade": ([("katana", katana_skin), ("bainha", bainha_skin)], {
        PRETO: (22, 32, 74), BRANCO: (84, 204, 255), ACO_FIO: (176, 232, 255), ACO_ESC: (42, 62, 114),
        LACA: (24, 34, 82), LACA_ESC: (70, 190, 250), EXTRA1: (130, 230, 255)}),
    "lua": ([("katana", katana_skin), ("bainha", bainha_skin)], {
        PRETO: (24, 22, 26), BRANCO: (180, 24, 34), ACO_FIO: (226, 40, 48), ACO_ESC: (26, 24, 30),
        LACA: (20, 18, 22), LACA_ESC: (200, 26, 36), EXTRA1: (200, 26, 36)}),
    # pistols
    "dourada": ([("pistola", pistola_skin), ("coldre", coldre_skin)], {
        ACO: (244, 204, 84), ACO_ESC: (200, 150, 52), FERRO: (120, 90, 30),
        MADEIRA_ESC2: (236, 226, 200), LATAO: (255, 236, 150),
        COURO: (124, 32, 32), COURO_ESC: (92, 22, 22), EXTRA1: (255, 250, 210)}),
    "coral": ([("pistola", pistola_skin), ("coldre", coldre_skin)], {
        ACO: (70, 180, 176), ACO_ESC: (34, 116, 124), FERRO: (20, 70, 80),
        MADEIRA_ESC2: (232, 104, 92), LATAO: (244, 242, 232),
        COURO: (40, 82, 124), COURO_ESC: (28, 58, 92), EXTRA1: (250, 248, 240)}),
    "relampago": ([("pistola", pistola_skin), ("coldre", coldre_skin)], {
        ACO: (56, 60, 74), ACO_ESC: (32, 34, 44), FERRO: (90, 220, 255),
        MADEIRA_ESC2: (30, 30, 36), LATAO: (90, 220, 255),
        COURO: (28, 28, 32), COURO_ESC: (18, 18, 22), EXTRA1: (110, 230, 255)}),
}


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "personagem")
    for nome, (vox, tam) in [("espada", espada()), ("escudo", escudo()), ("katana", katana()),
                             ("bainha", bainha()), ("pistola", pistola()), ("coldre", coldre()),
                             ("arco", arco())]:
        escrever_vox(os.path.join(pasta, f"{nome}.vox"), vox, tam, paleta())
        print(f"{nome}.vox: {len(vox)} voxels, tela {tam[0]}x{tam[1]}x{tam[2]}")
    ferramentas = [("machado", machado)] + [(f"picareta_{t}", (lambda t=t: picareta(t))) for t in (1, 2, 3, 4)]
    for nome, gera in ferramentas:
        vox, tam = gera()  # a picareta troca a cor da cabeca na paleta antes
        escrever_vox(os.path.join(pasta, f"{nome}.vox"), vox, tam, paleta())
        print(f"{nome}.vox: {len(vox)} voxels, tela {tam[0]}x{tam[1]}x{tam[2]}")
    for sufixo, (pecas, troca) in SKINS.items():
        p = paleta_com(troca)
        for base, gera in pecas:
            vox, tam = gera(sufixo)
            escrever_vox(os.path.join(pasta, f"{base}_{sufixo}.vox"), vox, tam, p)
            print(f"{base}_{sufixo}.vox: {len(vox)} voxels")
