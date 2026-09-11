#!/usr/bin/env python3
"""O piratinha, refeito no rig de dez pecas.

O `pirate_captain.vox` de antes era uma peca so', em pose T, mais fino e de
cabeca menor que a ficha (`docs/character create.md`): tronco 8x4, pernas
3x3, cabeca 1:6. Copiar voxel a voxel quebraria as juntas. Entao ele e'
REFEITO nas medidas da ficha mantendo o que faz ele ser ele: a mesma paleta,
camisa branca com costas cinza, faixa vermelha e cinto de fivela dourada,
calca preta, bota marrom, tapa-olho, um olho azul, barba escura emoldurando o
rosto — e o tricornio preto de aba dourada com a pena rosa, que vai num
arquivo proprio (`cabelo_01.vox`), por cima da cabeca.

Uso:  python3 tools/voxrender/pirata.py [--plano saida.vox]
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M

BRANCO, CINZA, PRETO, CARVAO, BOTA, COURO, OURO, OURO_ESC, FAIXA, PENA, OLHO_AZUL, PENA_ESC = range(1, 13)
CAB, PELE = M.CABELO, M.PELE
paleta = {
    # as cores do pirate_captain.vox, uma a uma
    BRANCO: (255, 255, 255), CINZA: (220, 220, 220), PRETO: (25, 25, 25), CARVAO: (44, 44, 48),
    BOTA: (90, 55, 30), COURO: (60, 35, 15), OURO: (235, 190, 50), OURO_ESC: (215, 170, 70),
    FAIXA: (180, 40, 40), PENA: (240, 60, 80), OLHO_AZUL: (0, 160, 240), PENA_ESC: (180, 40, 60),
    # Cinza como o original, mas CLARO o bastante pra separar do chapeu preto:
    # com o (60,60,60) de antes a cabeca virava um bloco escuro com uma
    # janelinha de rosto. Fica na faixa do cabelo, entao trocar custa zero.
    CAB[0]: (120, 120, 124), CAB[1]: (96, 96, 100), CAB[2]: (74, 74, 78), CAB[3]: (52, 52, 56),
    PELE[0]: (255, 225, 195), PELE[1]: (255, 209, 169), PELE[2]: (226, 154, 122), PELE[3]: (190, 120, 95),
    M.MARCA: (255, 0, 255),
}
cx = M.caixa

def poe(d, pts, c):
    for p in pts: d[p] = c

def torso():
    t = cx(11, 20, 9, 14, 20, 32, BRANCO)
    t.update(cx(11, 20, 9, 14, 20, 20, PRETO))            # cos da calca
    t.update(cx(11, 20, 9, 14, 21, 21, FAIXA))            # faixa vermelha
    t.update(cx(11, 20, 9, 14, 22, 22, COURO))            # cinto
    poe(t, [(15, 14, 22), (16, 14, 22)], OURO)             # fivela
    t.update(cx(11, 20, 9, 9, 23, 32, CINZA))             # costas da camisa
    # gola aberta em V
    poe(t, [(14, 14, 32), (15, 14, 32), (16, 14, 32), (17, 14, 32),
            (15, 14, 31), (16, 14, 31), (15, 14, 30), (16, 14, 30)], PELE[1])
    poe(t, [(15, 14, 29), (16, 14, 28)], CINZA)            # cordao da gola
    # ponta da faixa pendurada atras do quadril esquerdo
    poe(t, [(12, 8, 21), (12, 8, 20), (12, 8, 19), (13, 8, 19), (12, 8, 18)], FAIXA)
    return t

def braco(x0, x1):
    b = cx(x0, x1, 10, 13, 25, 32, BRANCO)
    b.update(cx(x0, x1, 10, 10, 25, 32, CINZA))           # parte de tras da manga
    return b

def antebraco(x0, x1):
    a = cx(x0, x1, 10, 13, 23, 24, BRANCO)                # manga
    a.update(cx(x0, x1, 10, 13, 22, 22, CINZA))           # punho dobrado
    a.update(cx(x0, x1, 10, 13, 16, 21, PELE[1]))         # antebraco e mao
    a.update(cx(x0, x1, 10, 13, 16, 16, PELE[2]))         # ponta dos dedos
    a.update(M.tampa(x0, x1, 25, BRANCO))
    return a

def coxa(x0, x1):
    c = cx(x0, x1, 10, 13, 10, 19, PRETO)
    c.update(cx(x0, x1, 10, 10, 10, 19, CARVAO))          # tras
    return c

def canela(x0, x1):
    c = cx(x0, x1, 10, 13, 8, 9, PRETO)
    c.update(cx(x0, x1, 10, 13, 2, 6, BOTA))
    c.update(cx(x0, x1, 10, 13, 7, 7, COURO))             # dobra do cano
    c.update(cx(x0, x1, 10, 15, 0, 1, BOTA))              # pe
    c.update(cx(x0, x1, 10, 15, 0, 0, COURO))             # sola
    c.update(cx(x0, x1, 14, 15, 2, 2, BOTA))              # peito do pe
    c.update(M.tampa(x0, x1, 10, PRETO))
    return c

def cabeca():
    h = cx(14, 17, 10, 13, 33, 33, PELE[2])               # pescoco
    h.update(cx(12, 19, 8, 15, 34, 41, CAB[1]))           # cabelo; o rosto e' pintado por cima
    F = 15                                                # a face da frente
    for x in range(13, 19):
        for z in range(36, 41): h[(x, F, z)] = PELE[1]
    h[(13, F, 38)] = BRANCO; h[(14, F, 38)] = OLHO_AZUL    # olho esquerdo DELE (x menor)
    h[(13, F, 39)] = CAB[2]; h[(14, F, 39)] = CAB[2]       # sobrancelha
    for x in (17, 18):                                    # tapa-olho no direito (x maior)
        for z in (37, 38): h[(x, F, z)] = PRETO
    h[(16, F, 39)] = PRETO; h[(15, F, 40)] = PRETO         # tira do tapa-olho
    h[(15, F, 36)] = PELE[2]; h[(16, F, 36)] = PELE[2]     # narinas
    h[(15, 16, 37)] = PELE[1]; h[(16, 16, 37)] = PELE[1]   # ponta do nariz, pra fora
    for x in range(13, 19): h[(x, F, 35)] = CAB[1]        # bigode
    h[(15, F, 35)] = PELE[3]; h[(16, F, 35)] = PELE[3]     # boca
    for x in range(12, 20): h[(x, F, 34)] = CAB[1]        # barba
    for x in range(14, 18):                               # tufo descendo na frente do pescoco
        for y in (14, 15): h[(x, y, 33)] = CAB[1]
    for y in (11, 12):                                    # orelhas
        for z in (37, 38): h[(12, y, z)] = PELE[2]; h[(19, y, z)] = PELE[2]
    for z in range(35, 40):                               # rabicho atras
        h[(15, 7, z)] = CAB[2]; h[(16, 7, z)] = CAB[2]
    h[(15, 7, 34)] = FAIXA; h[(16, 7, 34)] = FAIXA         # fitinha do rabicho
    return h

def chapeu():
    """O tricornio. A camera do jogo ve' justamente o TOPO dele, e de cima o
    tricornio e' um TRIANGULO: ponta na frente, dois cantos atras. A primeira
    versao era quadrada com a aba levantada em volta toda — de cima lia como
    moldura de quadro, nao como chapeu.

    A faixa que abraca a cabeca e' um voxel mais larga que ela: assim as
    faces de fora do chapeu nunca coincidem com as da cabeca, e a cabeca fica
    escondida dentro dele sem briga de profundidade."""
    c = cx(11, 20, 7, 16, 40, 41, PRETO)                  # faixa que abraca a cabeca
    # aba: ponta na frente (y 18), base reta atras (y 5)
    linhas = {18: (15, 16), 17: (14, 17), 16: (13, 18), 15: (12, 19), 14: (11, 20), 13: (10, 21), 5: (10, 21)}
    aba = {}
    for y in range(5, 19):
        x0, x1 = linhas.get(y, (9, 22))
        for x in range(x0, x1 + 1): aba[(x, y, 42)] = OURO
    c.update(aba)
    # a aba vira pra cima nos tres lados, e NAO nos tres cantos
    cantos = [(15.5, 18), (9.5, 5.5), (21.5, 5.5)]
    for (x, y, z) in aba:
        borda = any((x + dx, y + dy, 42) not in aba for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        perto = any(abs(x - cx_) <= 1.5 and abs(y - cy_) <= 1.5 for cx_, cy_ in cantos)
        if borda and not perto: c[(x, y, 43)] = OURO_ESC
    c.update(cx(11, 20, 7, 16, 43, 44, PRETO))            # copa
    c.update(cx(12, 19, 8, 15, 45, 45, PRETO))
    c[(15, 16, 44)] = OURO; c[(16, 16, 44)] = OURO         # distintivo na frente
    # pena: pluma de dois voxels subindo pra tras pelo lado direito
    for p in [(21, 11, 43), (21, 10, 43), (21, 10, 44), (21, 9, 44), (22, 9, 45),
              (22, 8, 45), (22, 8, 46), (22, 7, 46), (22, 7, 47), (22, 6, 47)]:
        c[p] = PENA
    for p in [(21, 11, 44), (22, 9, 46), (22, 6, 46)]:
        c[p] = PENA_ESC
    return c

pecas = [
    ("cabeca", cabeca()), ("torso", torso()),
    ("braco_d", braco(21, 24)), ("antebraco_d", antebraco(21, 24)),
    ("braco_e", braco(7, 10)),  ("antebraco_e", antebraco(7, 10)),
    ("coxa_d", coxa(16, 19)),   ("canela_d", canela(16, 19)),
    ("coxa_e", coxa(12, 15)),   ("canela_e", canela(12, 15)),
]

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "personagem")
    os.makedirs(pasta, exist_ok=True)
    open(os.path.join(pasta, "corpo.vox"), "wb").write(M.arquivo_cena(pecas, paleta))
    open(os.path.join(pasta, "cabelo_01.vox"), "wb").write(
        M.arquivo_cena([("cabelo", chapeu())], paleta, camada="cabelo"))
    print(f"corpo.vox + cabelo_01.vox em {pasta}")
    if "--plano" in sys.argv:
        p = sys.argv[sys.argv.index("--plano") + 1]
        open(p, "wb").write(M.arquivo_plano(pecas + [("cabelo", chapeu())], paleta))
