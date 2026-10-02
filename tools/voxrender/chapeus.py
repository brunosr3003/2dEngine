#!/usr/bin/env python3
"""The PLAYER's hats, each with its own colours.

The first player hats were borrowed from `npcs.py`, whose palette entries
(CHAPEU, DET, PENA...) are filled per NPC. `personagem.py` wrote them with
the bare base palette, where those entries are (0, 0, 0): every hat came out
pure black, shapes unreadable. Here each hat is a pair (voxels, colours) and
travels in its own .vox with its own palette.

Head: x 12-19, y 8-15 (front face y 15), z 34-41. Anything inside it is cut
by `personagem.sem_cabeca`; anything touching a face (nose at y 16, z 37)
fails `confere_sem_sobreposicao`. A hat lives ONE voxel out (x 11/20, y 7/16,
z 42+) and never above z 47 (canvas height 48).

Hats that leave hair showing (headband, crown, straw hat) carry the hair
shell in the HAIR band (245-248), so the shader paints it in the colour the
player chose — a hat never forces a hair colour.
"""
import math, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M        # noqa: E402

cx = M.caixa
CAB = M.CABELO

# Local palette slots: every hat writes its own colours here.
A1, A2, A3, A4 = 100, 101, 102, 103      # main material, light -> dark
B1, B2, B3 = 104, 105, 106               # accent
C1, C2 = 107, 108                        # trim / metal
G1, G2, G3 = 109, 110, 111               # gems / feathers


def anel(x0, x1, y0, y1, z0, z1, cor):
    """The outline of a box, hollow: a band around the head."""
    return {p: c for p, c in cx(x0, x1, y0, y1, z0, z1, cor).items()
            if p[0] in (x0, x1) or p[1] in (y0, y1)}


def disco(cxr, cyr, r, z, cor, r_dentro=-1.0):
    d = {}
    for x in range(int(cxr - r) - 1, int(cxr + r) + 2):
        for y in range(int(cyr - r) - 1, int(cyr + r) + 2):
            q = (x - cxr) ** 2 + (y - cyr) ** 2
            if r_dentro * r_dentro < q <= r * r if r_dentro >= 0 else q <= r * r:
                d[(x, y, z)] = cor
    return d


def borda(vox, z):
    """The voxels on the rim of a flat layer."""
    camada = {(x, y) for (x, y, zz) in vox if zz == z}
    return {(x, y, z) for (x, y) in camada
            if any((x + dx, y + dy) not in camada for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))}


def cabelo_por_baixo(lados=True):
    """The hair shell of `cabelo_01` (top, nape, sides), in the hair band."""
    c = cx(11, 20, 7, 16, 42, 42, CAB[1])
    c.update(cx(11, 20, 7, 7, 35, 42, CAB[1]))
    if lados:
        for y in range(7, 14):
            c[(11, y, 41)] = CAB[1]; c[(20, y, 41)] = CAB[1]
        for y in range(7, 12):
            for z in (38, 39, 40):
                c[(11, y, z)] = CAB[2]; c[(20, y, z)] = CAB[2]
    return c


# ── the eight free hats, remade ─────────────────────────────────────────

def capuz():
    """Forest-green hood: frames the face, drapes to the shoulders, a soft
    point droops at the back."""
    c = {}
    for (x, y, z) in cx(11, 20, 7, 16, 34, 42, A1):
        if 12 <= x <= 19 and 8 <= y <= 15 and z <= 41:
            continue                                   # the head
        if y == 16 and 12 <= x <= 19 and 34 <= z <= 40:
            continue                                   # the face opening
        c[(x, y, z)] = A2 if z <= 37 else A1
    for x in range(12, 20):                            # shaded inner rim
        c[(x, 16, 41)] = A4
    for z in range(34, 41):
        c[(11, 16, z)] = A3; c[(20, 16, z)] = A3
    c.update(cx(12, 19, 8, 15, 43, 43, A1))            # rounded crown
    c.update(cx(10, 21, 7, 15, 31, 33, A3))            # drape on the shoulders
    c.update(cx(11, 20, 6, 6, 30, 41, A2))
    for p in [(15, 5, 40), (16, 5, 40), (15, 5, 39), (16, 4, 38), (16, 4, 37)]:
        c[p] = A2                                      # the point
    cores = {A1: (78, 128, 74), A2: (62, 106, 60), A3: (46, 82, 46), A4: (26, 44, 28),
             C1: (196, 164, 104)}
    return c, cores


def pontudo():
    """Wizard hat: wide brim and a tall cone bent backwards, gold band and a
    star at the tip — from above, a disc with a point."""
    c = disco(15.5, 11.5, 6.9, 42, A3)
    for p in borda(c, 42):
        c[p] = A4
    raios = [(43, 4.6, 0), (44, 3.8, 0), (45, 3.0, -1), (46, 2.1, -2), (47, 1.3, -3)]
    for z, r, dy in raios:
        c.update(disco(15.5, 11.5 + dy, r, z, A1))
        for p in borda({k: v for k, v in c.items() if k[2] == z}, z):
            if p[1] < 11.5 + dy:
                c[p] = A2                              # the back of the cone in shade
    c.update(disco(15.5, 11.5, 4.6, 43, B1, r_dentro=3.4))   # gold band
    c[(15, 16, 43)] = G1; c[(16, 16, 43)] = G1         # buckle gem
    c[(15, 8, 47)] = B1; c[(16, 8, 47)] = B1           # star at the tip
    for (x, y) in ((11, 13), (19, 10), (13, 7), (18, 15)):
        if (x, y, 42) in c:
            c[(x, y, 42)] = B2                         # stars on the brim
    cores = {A1: (70, 84, 176), A2: (52, 62, 140), A3: (44, 52, 120), A4: (30, 34, 84),
             B1: (236, 196, 80), B2: (250, 230, 150), G1: (120, 220, 240)}
    return c, cores


def tricornio():
    """Tricorne: triangle from above (point forward), turned-up brim with
    gold edging, white plume on the right."""
    c = cx(11, 20, 7, 16, 40, 41, A3)
    linhas = {18: (15, 16), 17: (14, 17), 16: (13, 18), 15: (12, 19), 14: (11, 20), 13: (10, 21), 5: (10, 21)}
    aba = {}
    for y in range(5, 19):
        x0, x1 = linhas.get(y, (9, 22))
        for x in range(x0, x1 + 1):
            aba[(x, y, 42)] = A2
    c.update(aba)
    cantos = [(15.5, 18), (9.5, 5.5), (21.5, 5.5)]
    for p in borda(aba, 42):
        perto = any(abs(p[0] - a) <= 1.5 and abs(p[1] - b) <= 1.5 for a, b in cantos)
        if not perto:
            c[(p[0], p[1], 43)] = A2
            c[(p[0], p[1], 44)] = B1                   # gold edging
        else:
            c[p] = B1
    c.update(cx(11, 20, 7, 16, 43, 44, A1))
    c.update(cx(12, 19, 8, 15, 45, 45, A1))
    c[(15, 17, 44)] = B2; c[(16, 17, 44)] = B2         # cockade
    for p in [(21, 11, 45), (21, 10, 45), (22, 10, 45), (22, 9, 46), (22, 8, 46),
              (22, 8, 47), (22, 7, 47), (21, 9, 46)]:
        c[p] = G1
    for p in [(22, 9, 45), (21, 8, 47)]:
        c[p] = G2
    cores = {A1: (50, 42, 46), A2: (38, 32, 36), A3: (28, 24, 28),
             B1: (226, 182, 72), B2: (176, 40, 44), G1: (244, 242, 236), G2: (206, 204, 198)}
    return c, cores


def aba():
    """Traveller's brimmed hat: tan leather, dark band, a pinched crown."""
    c = disco(15.5, 11.5, 7.6, 42, A2)
    for p in borda(c, 42):
        c[(p[0], p[1], 43)] = A3                       # brim curls up at the edge
    c.update(cx(12, 19, 8, 15, 43, 43, B1))            # band
    c[(19, 16, 43)] = C1; c[(18, 16, 43)] = C1         # buckle
    c.update(cx(12, 19, 8, 15, 44, 45, A1))
    c.update(cx(13, 18, 9, 14, 46, 46, A1))
    for y in range(9, 15):                             # the pinch down the middle
        c[(15, y, 46)] = A3; c[(16, y, 46)] = A3
    cores = {A1: (170, 122, 74), A2: (148, 104, 62), A3: (112, 76, 44),
             B1: (64, 42, 30), C1: (210, 180, 110)}
    return c, cores


def boina_pena():
    """Crimson beret slumped to the right, gold pin and a long white feather."""
    c = cx(11, 20, 7, 16, 41, 41, A3)
    c.update(cx(11, 21, 7, 17, 42, 42, A2))
    c.update(cx(12, 22, 7, 17, 43, 43, A1))
    c.update(cx(14, 22, 8, 16, 44, 44, A1))
    c.update(cx(17, 21, 10, 14, 45, 45, A2))
    c[(15, 17, 42)] = B1; c[(16, 17, 42)] = B1         # pin
    for p in [(11, 12, 43), (10, 11, 44), (10, 10, 45), (9, 9, 45), (9, 8, 46),
              (9, 7, 46), (8, 6, 47), (8, 5, 47)]:
        c[p] = G1
    for p in [(10, 11, 45), (9, 9, 46), (8, 6, 46)]:
        c[p] = G2
    cores = {A1: (176, 40, 52), A2: (148, 30, 42), A3: (112, 22, 32),
             B1: (236, 196, 80), G1: (246, 244, 238), G2: (200, 198, 192)}
    return c, cores


def lenco():
    """Red bandana with white polka dots, knotted at the back with two tails."""
    c = cx(11, 20, 7, 16, 40, 41, A1)
    c.update(cx(11, 20, 7, 16, 42, 42, A1))
    c.update(cx(12, 19, 8, 15, 43, 43, A2))
    for (x, y, z) in list(c):
        if (x + y + z) % 4 == 0 and (y == 16 or x in (11, 20) or z == 43):
            c[(x, y, z)] = B1                          # dots
    c.update(cx(15, 16, 6, 6, 39, 41, A3))             # knot
    for p in [(14, 5, 38), (14, 5, 37), (13, 5, 36), (17, 5, 38), (17, 5, 37), (18, 5, 36)]:
        c[p] = A2                                      # tails
    cores = {A1: (192, 46, 46), A2: (160, 34, 36), A3: (118, 24, 26), B1: (246, 240, 230)}
    return c, cores


def faixa():
    """Blue cloth headband over the player's own hair, with a steel plate on
    the brow and tails fluttering at the back. (It used to be one black
    ring at eye level — it read as a unibrow.)"""
    c = cabelo_por_baixo()
    c.update(anel(11, 20, 7, 16, 40, 41, A1))
    for x in range(11, 21):
        c[(x, 16, 40)] = A2
    c.update(cx(14, 17, 16, 16, 40, 41, C1))           # brow plate
    c[(14, 16, 41)] = C2; c[(17, 16, 40)] = C2
    for p in [(14, 6, 40), (14, 5, 39), (13, 4, 38), (17, 6, 40), (17, 5, 40), (18, 4, 39)]:
        c[p] = A2
    cores = {A1: (52, 96, 172), A2: (38, 72, 136), C1: (178, 186, 198), C2: (120, 128, 142)}
    return c, cores


def touca():
    """Knit beanie: mustard with rust stripes, folded cuff, pompom."""
    c = cx(11, 20, 7, 16, 40, 41, B2)                  # cuff
    c.update(cx(11, 20, 7, 16, 42, 43, A1))
    c.update(cx(12, 19, 8, 15, 44, 44, A1))
    c.update(cx(13, 18, 9, 14, 45, 45, A2))
    for p in borda({k: v for k, v in c.items() if k[2] == 43}, 43):
        c[p] = B1                                      # stripe
    for x in range(11, 21, 2):
        c[(x, 16, 41)] = B3                            # rib on the cuff
    c.update(cx(15, 16, 11, 12, 46, 47, B1))           # pompom
    cores = {A1: (220, 170, 60), A2: (190, 142, 46), B1: (176, 74, 40), B2: (150, 62, 34),
             B3: (120, 48, 26)}
    return c, cores


# ── new hats ─────────────────────────────────────────────────────────────

def palha():
    """Straw hat: wide woven brim with a red band — the free summer hat."""
    c = cabelo_por_baixo(lados=True)
    del_topo = [p for p in c if p[2] == 42]
    for p in del_topo:
        del c[p]
    for (x, y, z) in disco(15.5, 11.5, 8.4, 42, A1):
        c[(x, y, z)] = A1 if (x + y) % 2 else A2       # weave
    for p in borda({k: v for k, v in c.items() if k[2] == 42}, 42):
        c[p] = A3
    c.update(cx(12, 19, 8, 15, 43, 43, B1))            # band
    for (x, y, z) in cx(12, 19, 8, 15, 44, 45, A1):
        c[(x, y, z)] = A1 if (x + y + z) % 2 else A2
    for p in [(19, 16, 43), (19, 16, 42)]:
        c[p] = B2                                      # band tails
    cores = {A1: (236, 206, 120), A2: (212, 178, 92), A3: (178, 144, 70),
             B1: (194, 50, 46), B2: (150, 34, 32)}
    return c, cores


def capitao():
    """Captain's bicorne: wide side to side, black with thick gold braid, a
    skull badge and a tall white plume."""
    c = cx(11, 20, 7, 16, 40, 41, A3)
    for x in range(6, 26):
        meio = 15.5
        alt = 5 - int(abs(x - meio) / 2.6)             # tall in the middle, tips low
        for z in range(42, 42 + max(1, alt)):
            for y in (10, 11, 12, 13):
                c[(x, y, z)] = A1 if y >= 12 else A2
    for x in range(6, 26):                             # gold braid along the top edge
        tops = [z for (xx, y, z) in c if xx == x and y == 12]
        if tops:
            c[(x, 14, max(tops))] = B1
            c[(x, 9, max(tops))] = B1
    for x in range(7, 25):
        c[(x, 14, 42)] = B1; c[(x, 9, 42)] = B1
    c.update(cx(14, 17, 14, 14, 44, 45, C1))           # skull badge
    c[(14, 14, 45)] = A3; c[(17, 14, 45)] = A3
    c[(15, 14, 44)] = A3; c[(16, 14, 44)] = A3
    for p in [(15, 11, 47), (16, 11, 47), (15, 10, 47), (16, 12, 47), (17, 11, 47)]:
        c[p] = G1
    cores = {A1: (44, 38, 44), A2: (32, 28, 34), A3: (20, 18, 22), B1: (232, 190, 70),
             C1: (240, 236, 224), G1: (250, 250, 246)}
    return c, cores


def elmo():
    """Knight's great helm: closed steel, eye slit, breathing holes and a red
    crest. The front plate stands two voxels out so the nose stays inside."""
    c = {}
    c.update(cx(11, 11, 7, 17, 33, 42, A2))
    c.update(cx(20, 20, 7, 17, 33, 42, A2))
    c.update(cx(11, 20, 7, 7, 33, 42, A3))
    c.update(cx(11, 20, 17, 17, 33, 42, A1))           # face plate
    c.update(cx(11, 20, 7, 17, 43, 43, A1))            # top
    c.update(cx(12, 19, 8, 16, 44, 44, A2))
    for x in range(12, 20):
        if x not in (15, 16):
            c[(x, 17, 38)] = A4                        # eye slit
    for y in (10, 12, 14):                             # vents on the cheeks
        for x in (11, 20):
            c[(x, y, 36)] = A4
    c.update(cx(15, 16, 17, 17, 34, 42, C1))           # central ridge
    c.update(cx(11, 20, 7, 17, 33, 33, B2))            # gold rim at the neck
    for y in range(7, 17):                             # red crest front to back
        h = 47 if 9 <= y <= 14 else 46
        for z in range(45, h + 1):
            c[(15, y, z)] = G1 if z < h else G2
            c[(16, y, z)] = G1 if z < h else G2
    cores = {A1: (190, 196, 206), A2: (160, 166, 178), A3: (124, 130, 142), A4: (30, 32, 38),
             B2: (214, 176, 72), C1: (222, 226, 232), G1: (184, 36, 40), G2: (214, 60, 60)}
    return c, cores


def chifres():
    """Horned helm: iron cap with a bronze band and nose guard, two horns
    sweeping out and up."""
    c = cx(11, 20, 7, 16, 40, 42, A1)
    c.update(cx(12, 19, 8, 15, 43, 44, A1))
    c.update(cx(13, 18, 9, 14, 45, 45, A2))
    c.update(anel(11, 20, 7, 16, 40, 40, B1))          # bronze band
    for (x, y, z) in list(c):
        if z == 40 and (x + y) % 3 == 0:
            c[(x, y, z)] = B2                          # rivets
    c.update(cx(15, 16, 16, 17, 41, 42, B1))
    c.update(cx(15, 16, 17, 17, 38, 40, B1))           # nose guard, clear of the nose
    for lado in (-1, 1):
        x0 = 10 if lado < 0 else 21
        for k, (dx, z) in enumerate([(0, 42), (1, 42), (2, 43), (2, 44), (3, 45), (3, 46), (3, 47)]):
            x = x0 + lado * dx
            cor = G1 if k < 3 else (G2 if k < 6 else G3)
            c[(x, 11, z)] = cor; c[(x, 12, z)] = cor
            if k < 3:
                c[(x, 11, z + 1)] = cor; c[(x, 12, z + 1)] = cor
    cores = {A1: (120, 124, 132), A2: (96, 100, 110), B1: (176, 120, 60), B2: (222, 176, 96),
             G1: (232, 222, 196), G2: (212, 200, 170), G3: (90, 78, 64)}
    return c, cores


def turbante():
    """Wrapped turban: cream cloth in spiralling folds, a ruby on gold at the
    front and a small plume."""
    c = {}
    for z, r in ((40, 6.0), (41, 6.4), (42, 6.4), (43, 6.0), (44, 5.2), (45, 4.0), (46, 2.4)):
        c.update(disco(15.5, 11.5, r, z, A1))
    for (x, y, z) in list(c):
        ang = math.atan2(y - 11.5, x - 15.5)
        if int((ang / math.pi * 3 + z * 0.9)) % 2 == 0:
            c[(x, y, z)] = A2                          # the folds
    for p in borda({k: v for k, v in c.items() if k[2] == 40}, 40):
        c[p] = A3
    front = max(y for (x, y, z) in c if z == 42)
    c[(15, front, 42)] = B1; c[(16, front, 42)] = B1   # ruby
    c[(15, front, 43)] = C1; c[(16, front, 43)] = C1
    c[(15, front, 41)] = C1; c[(16, front, 41)] = C1
    c[(14, front, 42)] = C1; c[(17, front, 42)] = C1
    for p in [(15, front - 1, 44), (15, front - 1, 45), (16, front - 2, 46), (16, front - 2, 47)]:
        c[p] = G1
    cores = {A1: (240, 230, 206), A2: (214, 202, 174), A3: (180, 166, 136),
             B1: (200, 30, 50), C1: (232, 188, 64), G1: (60, 150, 120)}
    return c, cores


def coroa():
    """Gold crown over the player's own hair: points with gems, a velvet cap
    inside."""
    c = cabelo_por_baixo()
    c.update(anel(10, 21, 6, 17, 41, 43, A1))
    for p in borda({k: v for k, v in c.items() if k[2] == 41 and v == A1}, 41):
        c[p] = A2                                      # lower rim
    pontas = []
    for (x, y, z) in anel(10, 21, 6, 17, 44, 44, A1):
        lado = x if y in (6, 17) else y
        if lado % 3 == 0:
            pontas.append((x, y))
    for (x, y) in pontas:
        c[(x, y, 44)] = A1; c[(x, y, 45)] = A3
    c.update(cx(12, 19, 8, 15, 43, 44, B1))            # velvet
    c.update(cx(14, 17, 10, 13, 45, 45, B1))
    c.update(cx(15, 16, 11, 12, 46, 46, A1))           # orb
    c[(15, 11, 47)] = A1
    for (x, z, g) in ((15, 42, G1), (16, 42, G1), (12, 42, G2), (19, 42, G2)):
        c[(x, 17, z)] = g
    for y in (11, 12):
        c[(10, y, 42)] = G3; c[(21, y, 42)] = G3
    cores = {A1: (240, 198, 70), A2: (196, 150, 44), A3: (255, 232, 140),
             B1: (140, 26, 44), G1: (220, 40, 60), G2: (60, 110, 220), G3: (60, 190, 110)}
    return c, cores


# Order matches `shared::aparencia::CHAPEUS` (file names).
CHAPEUS = {
    "capuz": capuz, "pontudo": pontudo, "tricornio": tricornio, "aba": aba,
    "boina_pena": boina_pena, "lenco": lenco, "faixa": faixa, "touca": touca,
    "palha": palha, "capitao": capitao, "elmo": elmo, "chifres": chifres,
    "turbante": turbante, "coroa": coroa,
}
