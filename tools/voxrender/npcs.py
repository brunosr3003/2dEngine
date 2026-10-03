#!/usr/bin/env python3
"""Os NPCs da vila: um rig por OFICIO, no mesmo corpo de dez pecas.

A regra e' a dos humanoides (`humanoides.py`): mesmo molde, mesmos pivos,
mesma tela — o que muda e' o que le' de longe na camera alta. De cima o que
aparece e' CHAPEU, OMBRO e o que esta' na MAO, entao e' ali que mora a
diferenca: o chapeu pontudo do alquimista, a careca do ferreiro, o tricornio
do capitao, o caixote no ombro do estivador.

Objeto na mao vai DENTRO da peca do antebraco: acompanha o braco no parado
sem precisar do encaixe de arma. Chapeu e oculos vao dentro da cabeca.

Cada NPC sai em `assets/vox/npcs/<nome>.vox` (as dez pecas nomeadas). O
cliente escolhe pelo papel que vem no `EntityMeta.kind`
(`shared::npc_papel_de_kind`, ver `render3d::rig_do_npc`).

Uso:  python3 tools/voxrender/npcs.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M
import pirata as P

cx = M.caixa
F = 15   # face da frente da cabeca
PELE, CAB = M.PELE, M.CABELO

(CAMISA, CAMISA_ESC, CALCA, CALCA_ESC, BOTA, SOLA, CINTO, FIVELA, MANGA, MANGA_ESC,
 OLHO, OLHO_BR, CHAPEU, CHAPEU_ESC, DET, DET2, AVENTAL, AVENTAL_ESC, METAL, METAL_ESC,
 MADEIRA, MADEIRA_ESC, VIDRO1, VIDRO2, VIDRO3, PAPEL, PENA, OURO, LENTE, BOCA, MANCHA) = range(1, 32)

# ── paletas-base ─────────────────────────────────────────────────────────
PELES = {
    "clara":  [(252, 222, 192), (240, 200, 166), (214, 168, 134), (180, 132, 104)],
    "media":  [(226, 184, 140), (206, 160, 116), (176, 128, 92), (140, 98, 70)],
    "morena": [(176, 124, 86), (150, 102, 70), (122, 80, 54), (94, 60, 40)],
    "escura": [(126, 88, 62), (104, 70, 48), (82, 54, 36), (62, 40, 26)],
}
CABELOS = {
    "castanho": [(122, 86, 54), (98, 68, 42), (76, 52, 32), (54, 36, 22)],
    "preto":    [(58, 52, 50), (42, 38, 36), (30, 27, 26), (20, 18, 17)],
    "ruivo":    [(196, 104, 52), (168, 84, 40), (138, 66, 30), (104, 48, 22)],
    "loiro":    [(236, 206, 128), (214, 180, 100), (184, 150, 76), (150, 118, 56)],
    "grisalho": [(200, 200, 204), (170, 170, 176), (140, 140, 148), (110, 110, 118)],
    "branco":   [(244, 244, 246), (224, 224, 228), (196, 196, 202), (168, 168, 176)],
}
FIXAS = {
    OLHO: (46, 70, 110), OLHO_BR: (240, 240, 236), BOCA: (150, 84, 70), SOLA: (52, 36, 24),
    FIVELA: (220, 180, 70), METAL: (150, 156, 166), METAL_ESC: (98, 104, 116),
    MADEIRA: (132, 88, 50), MADEIRA_ESC: (92, 60, 34), PAPEL: (236, 226, 196),
    OURO: (232, 188, 60), LENTE: (190, 230, 240), M.MARCA: (255, 0, 255),
}

def paleta(pele="clara", cabelo="castanho", **cores):
    p = dict(FIXAS)
    for i, c in enumerate(PELES[pele]): p[PELE[i]] = c
    for i, c in enumerate(CABELOS[cabelo]): p[CAB[i]] = c
    p.update(cores)
    return p

# ── cabeca ───────────────────────────────────────────────────────────────
def cabeca(o):
    h = cx(14, 17, 10, 13, 33, 33, PELE[2])                    # pescoco
    h.update(cx(12, 19, 8, 15, 34, 41, PELE[1]))               # cabeca 8x8x8
    if not o.get("careca"):
        h.update(cx(12, 19, 8, 15, 41, 41, CAB[1]))            # topo
        h.update(cx(12, 19, 8, 8, 35, 41, CAB[1]))             # nuca
        for y in range(8, 14):
            h[(12, y, 40)] = CAB[1]; h[(19, y, 40)] = CAB[1]
        for y in range(8, 12):
            for z in (37, 38, 39): h[(12, y, z)] = CAB[2]; h[(19, y, z)] = CAB[2]
    else:
        h.update(cx(12, 19, 8, 8, 35, 37, CAB[2]))             # coroa de cabelo baixa
        h[(15, 9, 41)] = PELE[0]; h[(16, 10, 41)] = PELE[0]     # brilho da careca
    if o.get("cabelo_selvagem"):
        # Wild white hair standing out on every side, in tufts — the
        # professor who drives the flying bus (Kōgen-tō).
        for z in range(36, 45):
            for x in range(10, 22):
                for y in range(6, 15):
                    dentro = 12 <= x <= 19 and 8 <= y <= 15 and z <= 41
                    frente = y >= 13 and z <= 40
                    if dentro or frente:
                        continue
                    borda = min(x - 10, 21 - x, y - 6, 44 - z)
                    if borda >= 2 or (x * 7 + y * 13 + z * 5) % 3 != 0:
                        h[(x, y, z)] = CAB[0] if (x + y + z) % 2 else CAB[1]
    if o.get("coque"):
        h.update(cx(14, 17, 6, 7, 38, 41, CAB[1])); h.update(cx(15, 16, 5, 5, 39, 40, CAB[2]))
    if o.get("trancas"):
        for z in range(30, 38): h[(12, 9, z)] = CAB[2]; h[(19, 9, z)] = CAB[2]
    # rosto
    h[(13, F, 38)] = OLHO_BR; h[(14, F, 38)] = OLHO
    h[(17, F, 38)] = OLHO;    h[(18, F, 38)] = OLHO_BR
    for x in (13, 14, 17, 18): h[(x, F, 39)] = CAB[2]          # sobrancelha
    h[(15, 16, 37)] = PELE[1]; h[(16, 16, 37)] = PELE[1]       # nariz
    h[(15, F, 35)] = BOCA; h[(16, F, 35)] = BOCA
    for y in (11, 12):                                          # orelhas
        for z in (37, 38): h[(12, y, z)] = PELE[2]; h[(19, y, z)] = PELE[2]
    if o.get("rugas"):
        h[(13, F, 37)] = PELE[2]; h[(18, F, 37)] = PELE[2]; h[(14, F, 40)] = PELE[2]; h[(17, F, 40)] = PELE[2]
    barba = o.get("barba")
    if barba:
        cor = CAB[barba - 1] if isinstance(barba, int) else CAB[1]
        for x in range(13, 19): h[(x, F, 36)] = cor             # bigode
        h[(15, F, 36)] = PELE[2]; h[(16, F, 36)] = PELE[2]
        for x in range(12, 20): h[(x, F, 34)] = cor; h[(x, F, 35)] = cor
        h[(15, F, 35)] = BOCA; h[(16, F, 35)] = BOCA
        for y in range(11, 15):
            for z in (34, 35, 36): h[(12, y, z)] = cor; h[(19, y, z)] = cor
        for x in range(13, 19): h[(x, 15, 33)] = cor; h[(x, 14, 33)] = cor
        if o.get("barba_longa"):
            for z in range(26, 33):
                w = 3 if z > 29 else 2 if z > 27 else 1
                for x in range(16 - w, 16 + w): h[(x, 15, z)] = cor
    elif o.get("bigode"):
        for x in range(13, 19): h[(x, F, 36)] = CAB[1]
        h[(12, F, 35)] = CAB[1]; h[(19, F, 35)] = CAB[1]
        if o.get("bigode_farto"):
            for x in range(12, 20): h[(x, 16, 36)] = CAB[0]
            for x in range(13, 19): h[(x, 16, 35)] = CAB[1]
    if o.get("oculos"):
        for x in range(12, 20): h[(x, 16, 38)] = METAL_ESC
        for x in (13, 14, 17, 18): h[(x, 16, 38)] = LENTE
    if o.get("monoculo"):
        h[(17, 16, 38)] = LENTE; h[(18, 16, 38)] = LENTE; h[(17, 16, 39)] = OURO; h[(18, 16, 37)] = OURO
        for z in (33, 34, 35, 36): h[(19, 16, z)] = OURO        # correntinha
    chapeu = o.get("chapeu")
    if chapeu: h.update(CHAPEUS[chapeu]())
    return h

def ch_capuz():
    c = {}
    for (x, y, z), _ in cx(11, 20, 7, 16, 36, 42, CHAPEU).items():
        dentro = 12 <= x <= 19 and 8 <= y <= 15 and z <= 41
        rosto = 13 <= x <= 18 and y >= 15 and 35 <= z <= 40
        if not dentro and not rosto: c[(x, y, z)] = CHAPEU
    for z in range(28, 36):
        for x in range(12, 20): c[(x, 7, z)] = CHAPEU_ESC
    return c

def ch_pontudo():
    """Chapeu de bruxo: aba larga e cone torto pra tras. De cima e' um disco
    com uma ponta — nada mais no jogo tem essa silhueta."""
    c = {}
    for x in range(9, 23):
        for y in range(5, 19):
            if (x - 15.5) ** 2 + (y - 11.5) ** 2 <= 44: c[(x, y, 42)] = CHAPEU_ESC
    for x in range(11, 21):
        for y in range(7, 17): c[(x, y, 42)] = CHAPEU
    raios = [(43, 4.6, 0), (44, 3.8, 0), (45, 3.0, -1), (46, 2.1, -2), (47, 1.2, -3)]
    for z, r, dy in raios:
        for x in range(10, 22):
            for y in range(4, 20):
                if (x - 15.5) ** 2 + (y - 11.5 - dy) ** 2 <= r * r: c[(x, y, z)] = CHAPEU
    for x in range(11, 21):
        for y in range(7, 17):
            if (x - 15.5) ** 2 + (y - 11.5) ** 2 <= 22 and (x - 15.5) ** 2 + (y - 11.5) ** 2 > 12:
                c[(x, y, 43)] = DET                              # fita
    c[(15, 8, 47)] = DET2; c[(16, 8, 47)] = DET2                 # estrelinha na ponta
    return c

def ch_tricornio():
    mapa = {P.PRETO: CHAPEU, P.OURO: DET, P.OURO_ESC: DET2, P.PENA: PENA, P.PENA_ESC: PENA}
    return {p: mapa.get(c, CHAPEU) for p, c in P.chapeu().items() if p[2] <= 47}

def ch_aba():
    """Chapeu de aba larga, de viajante."""
    c = {}
    for x in range(8, 24):
        for y in range(4, 20):
            if (x - 15.5) ** 2 + (y - 11.5) ** 2 <= 60: c[(x, y, 42)] = CHAPEU_ESC
    c.update(cx(12, 19, 8, 15, 43, 43, DET))
    c.update(cx(12, 19, 8, 15, 44, 45, CHAPEU))
    c.update(cx(13, 18, 9, 14, 46, 46, CHAPEU))
    return c

def ch_boina_pena():
    c = cx(11, 20, 7, 16, 41, 42, CHAPEU)
    c.update(cx(10, 20, 8, 17, 43, 43, CHAPEU))
    c.update(cx(12, 19, 9, 16, 44, 44, CHAPEU_ESC))
    for p in [(20, 10, 43), (21, 9, 44), (21, 8, 45), (22, 7, 45), (22, 6, 46), (22, 5, 46), (23, 4, 47)]:
        c[p] = PENA
    return c

def ch_lenco():
    c = {p: CHAPEU for p in cx(11, 20, 7, 16, 40, 41, CHAPEU)}
    c.update(cx(12, 19, 8, 15, 42, 42, CHAPEU))
    for z in (36, 37, 38, 39): c[(15, 6, z)] = CHAPEU_ESC; c[(16, 6, z)] = CHAPEU_ESC
    for x in range(11, 21, 2): c[(x, 16, 40)] = DET             # bolinhas
    return c

def ch_faixa():
    return {p: CHAPEU for p in cx(11, 20, 7, 16, 39, 39, CHAPEU)} | {(15, 6, 38): CHAPEU, (15, 6, 37): CHAPEU_ESC}

def ch_touca():
    c = cx(11, 20, 7, 16, 40, 42, CHAPEU)
    c.update(cx(12, 19, 8, 15, 43, 44, CHAPEU))
    c.update(cx(11, 20, 7, 16, 40, 40, CHAPEU_ESC))
    return c

CHAPEUS = {"capuz": ch_capuz, "pontudo": ch_pontudo, "tricornio": ch_tricornio, "aba": ch_aba,
           "boina_pena": ch_boina_pena, "lenco": ch_lenco, "faixa": ch_faixa, "touca": ch_touca}

# ── corpo ────────────────────────────────────────────────────────────────
def torso(o):
    t = cx(11, 20, 9, 14, 20, 32, CAMISA)
    t.update(cx(11, 20, 9, 9, 23, 32, CAMISA_ESC))
    t.update(cx(11, 20, 9, 14, 20, 21, CALCA))
    t.update(cx(11, 20, 9, 14, 22, 22, CINTO))
    t[(15, 14, 22)] = FIVELA; t[(16, 14, 22)] = FIVELA
    frente = 14
    if o.get("barriga"):
        t.update(cx(12, 19, 15, 15, 21, 30, CAMISA)); t.update(cx(13, 18, 16, 16, 23, 28, CAMISA))
        t.update(cx(12, 19, 15, 15, 22, 22, CINTO)); t[(15, 15, 22)] = FIVELA; t[(16, 15, 22)] = FIVELA
        frente = 16
    if o.get("listras"):
        for z in (24, 26, 28, 30):
            for x in range(11, 21): t[(x, 14, z)] = CAMISA_ESC
            for y in range(10, 14): t[(11, y, z)] = CAMISA_ESC; t[(20, y, z)] = CAMISA_ESC
    if o.get("colete"):
        t.update(cx(11, 13, 14, 14, 23, 32, DET)); t.update(cx(18, 20, 14, 14, 23, 32, DET))
        t.update(cx(11, 20, 9, 9, 23, 32, DET))
        for z in (25, 28, 31): t[(14, 14, z)] = OURO
    if o.get("casaco"):
        # casaco longo aberto na frente: laterais e costas descem ate' o joelho
        t.update(cx(11, 13, 14, 14, 20, 32, DET)); t.update(cx(18, 20, 14, 14, 20, 32, DET))
        t.update(cx(11, 20, 9, 9, 12, 32, DET)); t.update(cx(11, 11, 9, 14, 12, 32, DET))
        t.update(cx(20, 20, 9, 14, 12, 32, DET))
        for z in (24, 27, 30): t[(13, 14, z)] = OURO; t[(18, 14, z)] = OURO
        t.update(cx(11, 20, 9, 14, 32, 32, DET2))               # gola
    if o.get("capa"):
        t.update(cx(10, 21, 8, 8, 10, 32, CHAPEU_ESC)); t.update(cx(11, 20, 9, 14, 32, 32, CHAPEU))
        t[(13, 15, 31)] = OURO; t[(18, 15, 31)] = OURO           # broche
    if o.get("peitoral"):
        t.update(cx(11, 20, 15, 15, 23, 31, METAL)); t.update(cx(11, 20, 8, 8, 23, 31, METAL_ESC))
        for x in (12, 19):
            for z in (24, 30): t[(x, 15, z)] = METAL_ESC
        t.update(cx(13, 18, 15, 15, 27, 27, METAL_ESC))
    if o.get("cota"):
        for x in range(11, 21):
            for z in range(23, 32):
                if (x + z) % 2: t[(x, 14, z)] = METAL_ESC
    if o.get("avental"):
        y = frente + 1
        t.update(cx(12, 19, y, y, 20, 30, AVENTAL))
        for z in (31, 32): t[(13, y, z)] = AVENTAL_ESC; t[(18, y, z)] = AVENTAL_ESC
        t.update(cx(12, 19, y, y, 22, 22, AVENTAL_ESC))
        for (x, z) in o.get("manchas", []): t[(x, y, z)] = MANCHA
        if o.get("bolso"): t.update(cx(14, 17, y + 1, y + 1, 24, 26, AVENTAL_ESC))
    if o.get("frascos"):
        y = frente + (2 if o.get("avental") else 1)
        for x, c in ((12, VIDRO1), (14, VIDRO2), (17, VIDRO3), (19, VIDRO1)):
            t.update(cx(x, x, y, y, 19, 21, c)); t[(x, y, 22)] = MADEIRA
    if o.get("fita"):
        for z in range(24, 33):
            c = DET if z % 2 else DET2
            t[(13, 15, z)] = c; t[(18, 15, z)] = c
    if o.get("lenco_pescoco"):
        t.update(cx(13, 18, 15, 15, 31, 32, DET)); t[(15, 15, 30)] = DET; t[(16, 15, 29)] = DET
    if o.get("caixote"):
        # no ombro direito, segurado pela lateral
        t.update(cx(20, 26, 8, 15, 33, 38, MADEIRA))
        for x in (20, 26):
            for y in (8, 15): t.update(cx(x, x, y, y, 33, 38, MADEIRA_ESC))
        t.update(cx(20, 26, 8, 15, 36, 36, MADEIRA_ESC))
    if o.get("bolsa"):
        t.update(cx(10, 10, 11, 14, 17, 22, DET)); t[(10, 12, 23)] = DET2
        for z in range(23, 33): t[(11 + (z - 23) // 2, 14, z)] = DET2
    if o.get("livro_cinto"):
        t.update(cx(19, 21, 11, 13, 17, 21, DET2))
    if o.get("pergaminho_cinto"):
        t.update(cx(10, 10, 10, 11, 16, 23, PAPEL)); t[(10, 10, 15)] = MADEIRA_ESC; t[(10, 10, 24)] = MADEIRA_ESC
    if o.get("saia"):
        t.update(cx(10, 21, 9, 14, 12, 19, CALCA)); t.update(cx(10, 21, 8, 15, 12, 13, CALCA_ESC))
    return t

def braco(x0, x1, o):
    cor = PELE[1] if o.get("sem_manga") else MANGA
    b = cx(x0, x1, 10, 13, 25, 32, cor)
    b.update(cx(x0, x1, 10, 10, 25, 32, PELE[2] if o.get("sem_manga") else MANGA_ESC))
    if o.get("sem_manga"):
        b.update(cx(x0, x1, 10, 13, 31, 32, MANGA))              # alca da camiseta
    if o.get("ombreiras"):
        fora = x1 + 1 if x0 > 15 else x0 - 1
        lo, hi = min(x0, fora), max(x1, fora)
        b.update(cx(lo, hi, 9, 14, 31, 33, METAL)); b.update(cx(lo, hi, 9, 14, 33, 33, METAL_ESC))
    if o.get("capa") or o.get("casaco_manga"):
        b.update(cx(x0, x1, 10, 13, 32, 32, DET))
    return b

def antebraco(x0, x1, o, lado):
    a = {}
    manga = PELE[1] if o.get("sem_manga") or o.get("manga_arregacada") else MANGA
    a.update(cx(x0, x1, 10, 13, 23, 24, manga))
    a.update(cx(x0, x1, 10, 13, 22, 22, MANGA_ESC if manga == MANGA else PELE[2]))
    if o.get("manga_arregacada") and not o.get("sem_manga"):
        a.update(cx(x0, x1, 10, 13, 24, 24, MANGA))
    a.update(cx(x0, x1, 10, 13, 16, 21, PELE[1]))
    a.update(cx(x0, x1, 10, 13, 16, 16, PELE[2]))
    if o.get("luvas"):
        a.update(cx(x0, x1, 10, 13, 16, 20, DET2))
    a.update(M.tampa(x0, x1, 25, manga))
    item = o.get("mao_" + lado)
    if item: a.update(ITENS[item](lado))
    return a

def coxa(x0, x1, o):
    c = cx(x0, x1, 10, 13, 10, 19, CALCA)
    c.update(cx(x0, x1, 10, 10, 10, 19, CALCA_ESC))
    if o.get("avental_longo"):
        c.update(cx(x0, x1, 14, 14, 12, 19, AVENTAL))
    return c

def canela(x0, x1, o):
    c = cx(x0, x1, 10, 13, 8, 9, CALCA)
    c.update(cx(x0, x1, 10, 13, 2, 6, BOTA))
    c.update(cx(x0, x1, 10, 13, 7, 7, SOLA if not o.get("tunica") else CALCA))
    c.update(cx(x0, x1, 10, 15, 0, 1, BOTA))
    c.update(cx(x0, x1, 10, 15, 0, 0, SOLA))
    if o.get("tunica"):
        c.update(cx(x0, x1, 10, 13, 4, 9, CALCA)); c.update(cx(x0, x1, 10, 13, 4, 4, CALCA_ESC))
    c.update(M.tampa(x0, x1, 10, CALCA))
    return c

# ── o que se leva na mao (dentro do antebraco) ───────────────────────────
# Mao direita: x 21-24; esquerda: x 7-10. Mao em y 10-13, z 16-19.
def xs(lado): return (21, 24) if lado == "d" else (7, 10)

def it_martelo(lado):
    x0, x1 = xs(lado)
    a = cx(x0 + 1, x1 - 1, 11, 12, 9, 17, MADEIRA)
    a.update(cx(x0 - 1, x1 + 1, 10, 13, 6, 8, METAL)); a.update(cx(x0 - 1, x1 + 1, 10, 13, 6, 6, METAL_ESC))
    return a

def it_caneca(lado):
    x0, x1 = xs(lado)
    a = cx(x0, x1, 14, 16, 16, 19, MADEIRA)
    a.update(cx(x0, x1, 14, 16, 17, 17, METAL_ESC))
    a.update(cx(x0, x1, 14, 16, 20, 20, PAPEL))                  # espuma
    a[(x0 + 1, 15, 21)] = PAPEL
    return a

def it_livro(lado):
    x0, x1 = xs(lado)
    a = cx(x0, x1, 14, 16, 16, 22, DET2)
    a.update(cx(x0 + 1, x1, 14, 16, 17, 21, PAPEL))
    a.update(cx(x0, x0, 14, 16, 16, 22, DET2)); a[(x0, 16, 19)] = OURO
    return a

def it_cajado(lado):
    x0, x1 = xs(lado)
    xm = (x0 + x1) // 2
    a = cx(xm, xm + 1, 14, 14, 2, 41, MADEIRA)
    a.update(cx(xm, xm + 1, 14, 14, 36, 41, MADEIRA_ESC))
    a.update(cx(xm - 1, xm + 2, 13, 15, 42, 45, VIDRO1)); a[(xm, 14, 46)] = VIDRO1
    return a

def it_frasco(lado):
    x0, x1 = xs(lado)
    a = cx(x0, x1, 14, 16, 16, 19, VIDRO2)
    a.update(cx(x0 + 1, x1 - 1, 15, 15, 20, 21, VIDRO2)); a[(x0 + 1, 15, 22)] = MADEIRA
    a[(x0 + 2, 15, 22)] = MADEIRA; a[(x0 + 1, 14, 18)] = VIDRO3
    return a

def it_mapa(lado):
    x0, x1 = xs(lado)
    a = cx(x0 - 2, x1 + 2, 14, 15, 18, 19, PAPEL)
    a.update(cx(x0 - 2, x0 - 2, 14, 15, 18, 19, MADEIRA_ESC)); a.update(cx(x1 + 2, x1 + 2, 14, 15, 18, 19, MADEIRA_ESC))
    a[(x0 + 1, 15, 20)] = DET
    return a

def it_espada_madeira(lado):
    x0, x1 = xs(lado)
    a = cx(x0 + 1, x1 - 1, 14, 15, 17, 18, MADEIRA_ESC)          # cabo
    a.update(cx(x0, x1, 16, 16, 16, 19, MADEIRA_ESC))            # guarda
    a.update(cx(x0 + 1, x1 - 1, 17, 22, 17, 18, MADEIRA))        # lamina pra frente
    return a

def it_luneta(lado):
    x0, x1 = xs(lado)
    a = cx(x0 + 1, x1 - 1, 14, 20, 17, 18, OURO)
    a.update(cx(x0 + 1, x1 - 1, 20, 20, 17, 18, LENTE)); a.update(cx(x0, x1, 17, 17, 16, 19, METAL_ESC))
    return a

def it_elmo(lado):
    x0, x1 = xs(lado)
    a = cx(x0 - 1, x1 + 1, 14, 18, 15, 19, METAL)
    a.update(cx(x0, x1, 18, 18, 16, 17, METAL_ESC)); a.update(cx(x0 - 1, x1 + 1, 14, 18, 19, 19, METAL_ESC))
    a[((x0 + x1) // 2, 16, 20)] = DET2
    return a

def it_tecido(lado):
    x0, x1 = xs(lado)
    a = cx(x0, x1, 14, 16, 15, 21, DET2)
    for z in (15, 18, 21): a.update(cx(x0, x1, 14, 16, z, z, DET))
    return a

def it_pergaminho(lado):
    x0, x1 = xs(lado)
    a = cx(x0 + 1, x1 - 1, 14, 15, 14, 22, PAPEL)
    a.update(cx(x0, x1, 14, 15, 14, 14, MADEIRA_ESC)); a.update(cx(x0, x1, 14, 15, 22, 22, MADEIRA_ESC))
    a[(x0 + 1, 15, 18)] = DET2
    return a

def it_moedas(lado):
    x0, x1 = xs(lado)
    a = cx(x0, x1, 14, 16, 14, 18, DET)
    a.update(cx(x0 + 1, x1 - 1, 15, 15, 19, 19, DET2)); a[(x0 + 1, 16, 16)] = OURO
    return a

def it_cesta(lado):
    x0, x1 = xs(lado)
    a = cx(x0 - 1, x1 + 1, 13, 16, 11, 14, MADEIRA)
    a.update(cx(x0 - 1, x1 + 1, 13, 16, 12, 12, MADEIRA_ESC))
    a.update(cx(x0, x1, 14, 15, 15, 15, VIDRO3))                  # frutas
    a.update(cx(x0 + 1, x0 + 1, 14, 15, 15, 19, MADEIRA_ESC))    # alca
    return a

ITENS = {"martelo": it_martelo, "caneca": it_caneca, "livro": it_livro, "cajado": it_cajado,
         "frasco": it_frasco, "mapa": it_mapa, "espada_madeira": it_espada_madeira, "luneta": it_luneta,
         "elmo": it_elmo, "tecido": it_tecido, "pergaminho": it_pergaminho, "moedas": it_moedas, "cesta": it_cesta}

def pecas(o):
    return [
        ("cabeca", cabeca(o)), ("torso", torso(o)),
        ("braco_d", braco(21, 24, o)), ("antebraco_d", antebraco(21, 24, o, "d")),
        ("braco_e", braco(7, 10, o)),  ("antebraco_e", antebraco(7, 10, o, "e")),
        ("coxa_d", coxa(16, 19, o)),   ("canela_d", canela(16, 19, o)),
        ("coxa_e", coxa(12, 15, o)),   ("canela_e", canela(12, 15, o)),
    ]

# ── os NPCs ──────────────────────────────────────────────────────────────
NPCS = {
    # Tunica verde-escura ate' o pe', chapeu pontudo roxo, avental manchado de
    # pocao, oculos, frascos coloridos no cinto e um na mao.
    "alquimista": (
        dict(chapeu="pontudo", oculos=True, barba=2, rugas=True, avental=True, bolso=True, frascos=True,
             tunica=True, mao_d="frasco", mao_e="livro",
             manchas=[(13, 26), (14, 27), (18, 24), (17, 29), (15, 21)]),
        paleta("clara", "grisalho",
               CAMISA=(46, 94, 64), CAMISA_ESC=(32, 70, 48), CALCA=(46, 94, 64), CALCA_ESC=(32, 70, 48),
               BOTA=(70, 48, 34), CINTO=(84, 56, 34), MANGA=(46, 94, 64), MANGA_ESC=(32, 70, 48),
               CHAPEU=(104, 58, 150), CHAPEU_ESC=(80, 42, 118), DET=(222, 186, 70), DET2=(130, 240, 190),
               AVENTAL=(206, 192, 160), AVENTAL_ESC=(160, 146, 116), MANCHA=(120, 196, 90),
               VIDRO1=(120, 240, 150), VIDRO2=(220, 90, 200), VIDRO3=(90, 180, 250))),
    # Careca, barba ruiva, braco de fora bronzeado, avental de couro, martelo.
    "ferreiro": (
        dict(careca=True, barba=2, sem_manga=True, avental=True, avental_longo=True, luvas=True, mao_d="martelo",
             manchas=[(14, 25), (17, 27)]),
        paleta("morena", "ruivo",
               CAMISA=(96, 84, 72), CAMISA_ESC=(70, 60, 52), CALCA=(62, 52, 44), CALCA_ESC=(46, 38, 32),
               BOTA=(52, 36, 26), CINTO=(60, 40, 26), MANGA=(96, 84, 72), MANGA_ESC=(70, 60, 52),
               AVENTAL=(118, 76, 42), AVENTAL_ESC=(88, 54, 28), MANCHA=(50, 44, 40),
               DET=(90, 60, 36), DET2=(74, 50, 30))),
    # Peitoral, ombreiras e cota: o armeiro veste o que vende. Elmo na mao.
    "armeiro": (
        dict(peitoral=True, ombreiras=True, cota=True, bigode=True, mao_e="elmo"),
        paleta("media", "preto",
               CAMISA=(122, 128, 138), CAMISA_ESC=(92, 98, 108), CALCA=(70, 58, 48), CALCA_ESC=(52, 42, 34),
               BOTA=(60, 44, 32), CINTO=(78, 52, 32), MANGA=(110, 116, 126), MANGA_ESC=(84, 90, 100),
               DET=(150, 40, 40), DET2=(170, 50, 50))),
    # Barriga, avental branco, careca de bigode, caneca espumando.
    "taberneiro": (
        dict(careca=True, bigode=True, barriga=True, avental=True, manga_arregacada=True, mao_d="caneca",
             manchas=[(15, 24)]),
        paleta("clara", "castanho",
               CAMISA=(214, 196, 160), CAMISA_ESC=(180, 162, 128), CALCA=(84, 62, 44), CALCA_ESC=(62, 46, 32),
               BOTA=(70, 48, 32), CINTO=(90, 60, 36), MANGA=(214, 196, 160), MANGA_ESC=(180, 162, 128),
               AVENTAL=(246, 244, 236), AVENTAL_ESC=(210, 206, 196), MANCHA=(170, 120, 60))),
    # Roupa fina de duas cores, boina com pena, fita metrica no pescoco, tecido.
    "alfaiate": (
        dict(chapeu="boina_pena", colete=True, fita=True, mao_e="tecido"),
        paleta("clara", "loiro",
               CAMISA=(236, 232, 220), CAMISA_ESC=(206, 200, 186), CALCA=(64, 52, 110), CALCA_ESC=(46, 36, 84),
               BOTA=(40, 32, 36), CINTO=(60, 40, 60), MANGA=(236, 232, 220), MANGA_ESC=(206, 200, 186),
               CHAPEU=(170, 40, 70), CHAPEU_ESC=(130, 28, 52), PENA=(250, 220, 90),
               DET=(170, 40, 70), DET2=(250, 220, 90))),
    # Armadura leve de couro, faixa na testa, espada de madeira.
    "treinador": (
        dict(chapeu="faixa", ombreiras=True, cota=False, barba=3, mao_d="espada_madeira"),
        paleta("escura", "preto",
               CAMISA=(142, 96, 58), CAMISA_ESC=(110, 72, 42), CALCA=(92, 70, 50), CALCA_ESC=(70, 52, 36),
               BOTA=(58, 40, 28), CINTO=(60, 40, 26), MANGA=(142, 96, 58), MANGA_ESC=(110, 72, 42),
               CHAPEU=(190, 44, 44), CHAPEU_ESC=(150, 30, 30), METAL=(126, 86, 52), METAL_ESC=(98, 64, 38))),
    # Tunica azul, monoculo, livro aberto e outro no cinto.
    "identificador": (
        dict(monoculo=True, tunica=True, livro_cinto=True, rugas=True, mao_e="livro", chapeu="touca"),
        paleta("clara", "grisalho",
               CAMISA=(52, 82, 150), CAMISA_ESC=(38, 60, 116), CALCA=(52, 82, 150), CALCA_ESC=(38, 60, 116),
               BOTA=(48, 40, 50), CINTO=(200, 170, 80), MANGA=(52, 82, 150), MANGA_ESC=(38, 60, 116),
               CHAPEU=(38, 60, 116), CHAPEU_ESC=(200, 170, 80), DET=(200, 170, 80), DET2=(110, 36, 36))),
    # Casaco de viagem, chapeu de aba larga, bolsa a tiracolo, rolo de mapa.
    "cartografo": (
        dict(chapeu="aba", casaco=True, bolsa=True, barba=2, mao_e="mapa"),
        paleta("media", "castanho",
               CAMISA=(214, 204, 176), CAMISA_ESC=(180, 170, 144), CALCA=(96, 80, 56), CALCA_ESC=(72, 60, 42),
               BOTA=(80, 56, 36), CINTO=(84, 58, 36), MANGA=(120, 90, 58), MANGA_ESC=(96, 70, 44),
               CHAPEU=(112, 82, 52), CHAPEU_ESC=(86, 62, 38), DET=(120, 90, 58), DET2=(96, 70, 44))),
    # Camisa listrada, lenco na cabeca, braco de fora, caixote no ombro.
    "estivador": (
        dict(chapeu="lenco", listras=True, sem_manga=True, caixote=True),
        paleta("escura", "preto",
               CAMISA=(236, 236, 230), CAMISA_ESC=(40, 70, 140), CALCA=(70, 80, 100), CALCA_ESC=(52, 60, 78),
               BOTA=(58, 44, 34), CINTO=(70, 50, 34), MANGA=(236, 236, 230), MANGA_ESC=(40, 70, 140),
               CHAPEU=(196, 50, 44), CHAPEU_ESC=(150, 36, 32), DET=(240, 230, 210))),
    # Tricornio, casaco longo azul-marinho com botoes dourados, barba, luneta.
    "capitao": (
        dict(chapeu="tricornio", casaco=True, barba=2, lenco_pescoco=True, mao_d="luneta"),
        paleta("media", "grisalho",
               CAMISA=(236, 232, 220), CAMISA_ESC=(200, 196, 184), CALCA=(236, 232, 220), CALCA_ESC=(200, 196, 184),
               BOTA=(34, 30, 30), CINTO=(60, 40, 26), MANGA=(30, 44, 86), MANGA_ESC=(22, 32, 64),
               CHAPEU=(26, 26, 32), CHAPEU_ESC=(40, 40, 48), DET=(30, 44, 86), DET2=(232, 188, 60),
               PENA=(236, 236, 236))),
    # O ancião das missoes: capuz e capa marrons, barba branca comprida,
    # cajado com pedra acesa e um pergaminho. A cabeca e' a mais alta da vila.
    "mestre_missoes": (
        dict(chapeu="capuz", capa=True, tunica=True, barba=1, barba_longa=True, rugas=True,
             mao_d="cajado", mao_e="pergaminho", pergaminho_cinto=True),
        paleta("clara", "branco",
               CAMISA=(186, 170, 130), CAMISA_ESC=(150, 136, 100), CALCA=(186, 170, 130), CALCA_ESC=(150, 136, 100),
               BOTA=(80, 60, 40), CINTO=(110, 40, 40), MANGA=(186, 170, 130), MANGA_ESC=(150, 136, 100),
               CHAPEU=(110, 72, 44), CHAPEU_ESC=(86, 54, 32), VIDRO1=(250, 214, 90), DET2=(110, 40, 40))),
    # Mercador: colete, chapeu de aba, bolsa de moedas.
    "mercador": (
        dict(chapeu="aba", colete=True, barriga=True, bigode=True, mao_e="moedas"),
        paleta("media", "preto",
               CAMISA=(200, 170, 110), CAMISA_ESC=(166, 138, 86), CALCA=(90, 60, 40), CALCA_ESC=(68, 44, 28),
               BOTA=(60, 42, 28), CINTO=(60, 40, 26), MANGA=(200, 170, 110), MANGA_ESC=(166, 138, 86),
               CHAPEU=(60, 40, 70), CHAPEU_ESC=(44, 28, 52), DET=(120, 40, 40), DET2=(232, 188, 60))),
    # Kōgen-tō's flying-bus driver, in Skyreach and in the Docks: the
    # professor — wild white hair, a bushy moustache, a brown tweed jacket.
    "motorista": (
        dict(cabelo_selvagem=True, bigode=True, bigode_farto=True, rugas=True, casaco=True, lenco_pescoco=True),
        paleta("clara", "branco",
               CAMISA=(232, 228, 214), CAMISA_ESC=(196, 192, 180), CALCA=(96, 80, 62), CALCA_ESC=(74, 62, 48),
               BOTA=(52, 38, 28), CINTO=(60, 40, 26), MANGA=(132, 104, 72), MANGA_ESC=(104, 80, 54),
               DET=(132, 104, 72), DET2=(150, 40, 40))),
    # Aldeoes: roupa simples, cada um de uma cor e de um jeito.
    "aldeao_1": (
        dict(manga_arregacada=True, mao_e="cesta"),
        paleta("clara", "castanho",
               CAMISA=(170, 130, 90), CAMISA_ESC=(140, 104, 70), CALCA=(90, 100, 70), CALCA_ESC=(70, 78, 54),
               BOTA=(76, 54, 36), CINTO=(70, 48, 30), MANGA=(170, 130, 90), MANGA_ESC=(140, 104, 70),
               MADEIRA=(170, 130, 70), MADEIRA_ESC=(130, 96, 50), VIDRO3=(210, 60, 50))),
    "aldeao_2": (
        dict(coque=True, saia=True, lenco_pescoco=True),
        paleta("morena", "preto",
               CAMISA=(220, 200, 150), CAMISA_ESC=(186, 166, 120), CALCA=(150, 60, 60), CALCA_ESC=(120, 44, 44),
               BOTA=(70, 50, 34), CINTO=(90, 60, 36), MANGA=(220, 200, 150), MANGA_ESC=(186, 166, 120),
               DET=(60, 110, 150))),
    "aldeao_3": (
        dict(chapeu="touca", barba=3, sem_manga=False),
        paleta("escura", "grisalho",
               CAMISA=(80, 110, 130), CAMISA_ESC=(60, 86, 104), CALCA=(110, 96, 80), CALCA_ESC=(86, 74, 60),
               BOTA=(56, 42, 30), CINTO=(60, 40, 26), MANGA=(80, 110, 130), MANGA_ESC=(60, 86, 104),
               CHAPEU=(150, 120, 70), CHAPEU_ESC=(120, 94, 52))),
}

def valida(nome, ps):
    for peca, vox in ps:
        for (x, y, z), c in vox.items():
            assert 0 <= x < M.W and 0 <= y < M.D and 0 <= z < M.H, f"{nome}/{peca}: voxel fora da tela {(x, y, z)}"
            assert 1 <= c <= 255, f"{nome}/{peca}: cor {c}"

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "npcs"); os.makedirs(pasta, exist_ok=True)
    for nome, (o, pal) in NPCS.items():
        ps = pecas(o)
        valida(nome, ps)
        open(os.path.join(pasta, f"{nome}.vox"), "wb").write(M.arquivo_cena(ps, pal, camada=nome))
        print(f"{nome:15} {sum(len(v) for _, v in ps):5} voxels")
