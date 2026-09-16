#!/usr/bin/env python3
"""Icones do HUD, das skills e do mapa: SVG proprio -> atlas PNG + indice Rust.

Estilo "MMO mobile moderno" que casa com o HUD de vidro escuro:
  * HUD/menu/mapa: silhueta BRANCA com gradiente vertical suave e contorno
    escuro translucido. O cliente TINGE (multiplica) pela cor do estado, e o
    contorno escuro continua escuro -> legivel em qualquer fundo.
  * Skills: disco colorido da arma (gradiente radial + aro) com o glifo claro
    por cima. Nao se tinge: so' alfa.

Pipeline (determinstico): cada icone vira um SVG 64x64, `rsvg-convert` rasteriza
a 3x o tamanho da celula, o Pillow reduz em alfa PRE-MULTIPLICADO (sem franja
escura na borda) com LANCZOS e monta o atlas. Mesma entrada -> mesmo PNG.

    python3 tools/icones/gerar_icones_ui.py

Saidas: assets/icones/hud.png, mapa.png, skills.png e
crates/client/src/icones_ui_indice.rs.
"""

import math
import os
import subprocess
import tempfile

from PIL import Image

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SAIDA = os.path.join(RAIZ, "assets", "icones")
INDICE = os.path.join(RAIZ, "crates", "client", "src", "icones_ui_indice.rs")

# ─────────────────────────────── geometria ───────────────────────────────


def f(v):
    return ("%.2f" % v).rstrip("0").rstrip(".")


def circ(cx, cy, r):
    return "M%s,%sa%s,%s 0 1,0 %s,0a%s,%s 0 1,0 %s,0Z" % (
        f(cx - r), f(cy), f(r), f(r), f(2 * r), f(r), f(r), f(-2 * r))


def elipse(cx, cy, rx, ry):
    return "M%s,%sa%s,%s 0 1,0 %s,0a%s,%s 0 1,0 %s,0Z" % (
        f(cx - rx), f(cy), f(rx), f(ry), f(2 * rx), f(rx), f(ry), f(-2 * rx))


def rrect(x, y, w, h, r):
    r = min(r, w / 2, h / 2)
    return ("M%s,%sh%sa%s,%s 0 0 1 %s,%sv%sa%s,%s 0 0 1 %s,%sh%sa%s,%s 0 0 1 %s,%sv%sa%s,%s 0 0 1 %s,%sZ"
            % (f(x + r), f(y), f(w - 2 * r), f(r), f(r), f(r), f(r), f(h - 2 * r), f(r), f(r), f(-r), f(r),
               f(-(w - 2 * r)), f(r), f(r), f(-r), f(-r), f(-(h - 2 * r)), f(r), f(r), f(r), f(-r)))


def poly(pts):
    return "M" + "L".join("%s,%s" % (f(x), f(y)) for x, y in pts) + "Z"


def linha(*pts):
    return "M" + "L".join("%s,%s" % (f(x), f(y)) for x, y in pts)


def arco(cx, cy, r, a0, a1):
    """Arco de a0 a a1 (graus, 0 = direita, sentido horario na tela)."""
    x0, y0 = cx + r * math.cos(math.radians(a0)), cy + r * math.sin(math.radians(a0))
    x1, y1 = cx + r * math.cos(math.radians(a1)), cy + r * math.sin(math.radians(a1))
    grande = 1 if (a1 - a0) % 360 > 180 else 0
    return "M%s,%sA%s,%s 0 %d 1 %s,%s" % (f(x0), f(y0), f(r), f(r), grande, f(x1), f(y1))


def estrela(cx, cy, ro, ri, n=5, rot=-90):
    pts = []
    for k in range(2 * n):
        r = ro if k % 2 == 0 else ri
        a = math.radians(rot + k * 180.0 / n)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return poly(pts)


def engrenagem(cx, cy, ro, ri, n, furo):
    pts = []
    passo = 360.0 / n
    for k in range(n):
        base = k * passo
        for da, r in ((-passo * 0.30, ri), (-passo * 0.18, ro), (passo * 0.18, ro), (passo * 0.30, ri)):
            a = math.radians(base + da)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return poly(pts) + circ(cx, cy, furo)


def gira(pts, cx, cy, ang, esc=1.0):
    a = math.radians(ang)
    ca, sa = math.cos(a), math.sin(a)
    return [(cx + (x * ca - y * sa) * esc, cy + (x * sa + y * ca) * esc) for x, y in pts]


def espada(cx, cy, ang, esc=1.0):
    """Espada com a ponta pra cima (ang 0), centro no meio da lamina."""
    lamina = gira([(0, -24), (3.4, -18), (3.4, 7), (-3.4, 7), (-3.4, -18)], cx, cy, ang, esc)
    guarda = gira([(-9, 7), (9, 7), (9, 11), (-9, 11)], cx, cy, ang, esc)
    punho = gira([(-2.2, 11), (2.2, 11), (2.2, 20), (-2.2, 20)], cx, cy, ang, esc)
    pomo = gira([(0, 23)], cx, cy, ang, esc)[0]
    return [("F", poly(lamina)), ("F", poly(guarda)), ("F", poly(punho)), ("F", circ(pomo[0], pomo[1], 3.2 * esc))]


def katana(cx, cy, ang, esc=1.0):
    """Katana com a ponta pra cima: lamina fina com leve curva, tsuba e cabo."""
    costas = [(-1.8 + 0.0035 * (y - 8) ** 2 * -1 + 1.2, y) for y in range(-27, 9, 3)]
    fio = [(x + 3.4, y) for x, y in costas]
    lamina = gira([(costas[0][0] + 3.0, -28)] + fio + list(reversed(costas)), cx, cy, ang, esc)
    tsuba = gira([(0.0, 10.5)], cx, cy, ang, esc)[0]
    punho = gira([(-2.2, 13), (2.6, 13), (2.6, 26), (-2.2, 26)], cx, cy, ang, esc)
    return [("F", poly(lamina)), ("F", elipse(tsuba[0], tsuba[1], 5.2 * esc, 5.2 * esc)), ("F", poly(punho))]


def picareta(cx, cy, ang, esc=1.0):
    cabo = gira([(-2.4, -12), (2.4, -12), (2.4, 24), (-2.4, 24)], cx, cy, ang, esc)
    cabeca = []
    for t in range(0, 11):
        u = t / 10.0
        x = -22 + 44 * u
        y = -12 - 9 * math.sin(math.pi * u)
        cabeca.append((x, y))
    for t in range(10, -1, -1):
        u = t / 10.0
        x = -19 + 38 * u
        y = -8 - 5 * math.sin(math.pi * u)
        cabeca.append((x, y))
    return [("F", poly(cabo)), ("F", poly(gira(cabeca, cx, cy, ang, esc)))]


def crescente(cx, cy, r, ang, grossura=0.42):
    """Meia-lua (corte) centrada em (cx, cy) virada pra `ang`."""
    pts = []
    for t in range(0, 17):
        a = math.radians(ang - 80 + 160 * t / 16.0)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    for t in range(16, -1, -1):
        a = math.radians(ang - 70 + 140 * t / 16.0)
        rr = r * (1.0 - grossura * math.sin(math.pi * t / 16.0))
        pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
    return poly(pts)


def pessoa(cx, cy, esc=1.0):
    return [("F", circ(cx, cy - 12 * esc, 7 * esc)),
            ("F", "M%s,%sa%s,%s 0 0 1 %s,0Z" % (f(cx - 13 * esc), f(cy + 16 * esc), f(13 * esc), f(12 * esc), f(26 * esc)))]


def setas_girando(r=25):
    return [("S", arco(32, 32, r, 200, 320), 4),
            ("F", poly(gira([(0, -5), (6, 0), (0, 5)], 32 + r * math.cos(math.radians(320)), 32 + r * math.sin(math.radians(320)), 320 + 90))),
            ("S", arco(32, 32, r, 20, 140), 4),
            ("F", poly(gira([(0, -5), (6, 0), (0, 5)], 32 + r * math.cos(math.radians(140)), 32 + r * math.sin(math.radians(140)), 140 + 90)))]


def pergaminho(dx=0.0, dy=0.0, esc=1.0):
    def t(x, y):
        return 32 + (x - 32) * esc + dx, 32 + (y - 32) * esc + dy
    x0, y0 = t(18, 13)
    x1, y1 = t(14, 8)
    x2, y2 = t(14, 48)
    return [("F", rrect(x0, y0, 28 * esc, 38 * esc, 4 * esc)),
            ("F", rrect(x1, y1, 36 * esc, 9 * esc, 4.5 * esc)),
            ("F", rrect(x2, y2, 36 * esc, 9 * esc, 4.5 * esc)),
            ("DS", linha(t(24, 24), t(40, 24)), 3 * esc),
            ("DS", linha(t(24, 31), t(40, 31)), 3 * esc),
            ("DS", linha(t(24, 38), t(35, 38)), 3 * esc)]


def coroa_glifo():
    return [("F", poly([(11, 45), (8, 18), (21, 30), (32, 12), (43, 30), (56, 18), (53, 45)])),
            ("F", rrect(11, 45, 42, 9, 3)),
            ("D", circ(32, 36, 3.4)), ("D", circ(20, 38, 2.4)), ("D", circ(44, 38, 2.4))]


# ─────────────────────────────── os icones ───────────────────────────────

HUD = {
    "bolsa": [("S", "M24,22V16a8,8 0 0 1 16,0V22", 5), ("F", rrect(14, 20, 36, 36, 9)),
              ("DS", "M16,33Q32,41 48,33", 3), ("DS", rrect(24, 40, 16, 10, 3), 2.5)],
    "missoes": pergaminho(),
    "todas_missoes": pergaminho(-6, -5, 0.78) + pergaminho(7, 6, 0.78),
    "diarias": [("F", rrect(11, 15, 42, 40, 8)), ("D", rrect(11, 15, 42, 10, 8)),
                ("S", linha((23, 9), (23, 19)), 5), ("S", linha((41, 9), (41, 19)), 5),
                ("DS", linha((22, 39), (29, 46), (43, 31)), 5)],
    "grupo": pessoa(16, 34, 0.78) + pessoa(48, 34, 0.78) + pessoa(32, 30, 1.0),
    "amigos": pessoa(19, 38, 0.82) + pessoa(45, 38, 0.82) +
              [("F", "M32,12c-2.4,-4 -8.5,-3.4 -8.5,1.6c0,4.4 8.5,9.4 8.5,9.4c0,0 8.5,-5 8.5,-9.4c0,-5 -6.1,-5.6 -8.5,-1.6Z")],
    "avisos": [("F", "M17,45C17,24 22,14 32,14C42,14 47,24 47,45L51,49H13Z"),
               ("F", circ(32, 53, 4.2)), ("F", circ(32, 11, 3.2))],
    "menu": [("F", rrect(13, 16, 38, 6.5, 3.25)), ("F", rrect(13, 28.75, 38, 6.5, 3.25)), ("F", rrect(13, 41.5, 38, 6.5, 3.25))],
    "engrenagem": [("F", engrenagem(32, 32, 25, 18.5, 8, 7.5))],
    "configuracoes": [("S", linha((11, 18), (53, 18)), 4), ("S", linha((11, 32), (53, 32)), 4), ("S", linha((11, 46), (53, 46)), 4),
                      ("F", circ(24, 18, 6)), ("F", circ(42, 32, 6)), ("F", circ(28, 46, 6))],
    "cadeado": [("S", "M22,30V22a10,10 0 0 1 20,0V30", 6), ("F", rrect(13, 28, 38, 28, 7)),
                ("D", circ(32, 39.5, 3.8)), ("D", poly([(30.2, 41), (33.8, 41), (35, 49), (29, 49)]))],
    "atacar": espada(32, 31, 45, 1.18) + espada(32, 31, -45, 1.18),
    "pulo": [("F", circ(32, 13, 7)),
             ("F", "M25,22h14l4,13l-6,2l-1,9h-8l-1,-9l-6,-2Z"),
             ("S", linha((28, 24), (16, 13)), 5),
             ("S", linha((36, 24), (48, 13)), 5),
             ("S", linha((28, 44), (21, 55)), 5),
             ("S", linha((36, 44), (43, 55)), 5),
             ("DS", linha((12, 58), (52, 58)), 4),
             ("DS", linha((18, 34), (12, 40)), 2.6),
             ("DS", linha((46, 34), (52, 40)), 2.6)],
    "auto_combate": setas_girando() + espada(32, 32, 35, 0.66),
    "auto_coleta": setas_girando() + picareta(32, 33, -30, 0.62),
    "coleta": picareta(32, 32, -32, 0.95),
    "mais": [("F", rrect(28.5, 12, 7, 40, 3.5)), ("F", rrect(12, 28.5, 40, 7, 3.5))],
    "voltar": [("S", linha((38, 14), (20, 32), (38, 50)), 7)],
    "fechar": [("S", linha((19, 19), (45, 45)), 7), ("S", linha((45, 19), (19, 45)), 7)],
    "coroa": coroa_glifo(),
    "ficha": [("S", rrect(10, 9, 44, 46, 9), 3.5)] + pessoa(32, 36, 0.95),
    "habilidades": [("F", "M8,15Q21,10 32,17Q43,10 56,15V50Q43,45 32,52Q21,45 8,50Z"),
                    ("DS", "M32,17V52", 2.5),
                    ("D", poly([(46, 18), (38, 32), (43, 32), (40, 44), (49, 28), (44, 28)]))],
    "montaria": [("S", "M18,52V33a14,14 0 0 1 28,0V52", 9), ("D", circ(18, 44, 1.8)), ("D", circ(46, 44, 1.8)),
                 ("D", circ(20, 32, 1.8)), ("D", circ(44, 32, 1.8))],
    "recuperar_xp": [("S", arco(32, 32, 24, 150, 400), 4),
                     ("F", poly(gira([(0, -5.5), (6.5, 0), (0, 5.5)], 32 + 24 * math.cos(math.radians(150)), 32 + 24 * math.sin(math.radians(150)), 150 - 90))),
                     ("F", estrela(32, 33, 15, 6.5))],
    "conquistas": [("S", "M20,18h-6a6,6 0 0 0 6,12", 4), ("S", "M44,18h6a6,6 0 0 1 -6,12", 4),
                   ("F", "M19,11H45V26a13,13 0 0 1 -26,0Z"), ("F", rrect(29, 37, 6, 9, 1.5)), ("F", rrect(19, 45, 26, 9, 3)),
                   ("D", estrela(32, 23, 6, 2.6))],
    "craft": [("F", poly(gira([(-2.6, -6), (2.6, -6), (2.6, 26), (-2.6, 26)], 32, 32, 45))),
              ("F", poly(gira([(-11, -16), (11, -16), (11, -5), (-11, -5)], 32, 32, 45))),
              ("S", linha(*gira([(0, -8), (0, 22)], 32, 32, -45)), 5.5),
              ("F", "M%s,%s" % (f(32 + 16 * math.cos(math.radians(-135))), f(32 + 16 * math.sin(math.radians(-135)))) +
               "a8,8 0 1,0 0.01,0Z"),
              ("D", circ(32 + 16 * math.cos(math.radians(-135)) + 1.5, 32 + 16 * math.sin(math.radians(-135)) - 5, 3.6))],
    "forja": [("F", "M6,23H45a11,11 0 0 1 11,10H40V40H46V47H14V40H22V33H14a8,8 0 0 1 -8,-10Z"),
              ("F", rrect(12, 47, 36, 7, 3)),
              ("F", estrela(20, 12, 7, 2.2, 4, -90)), ("F", estrela(33, 8, 4.5, 1.6, 4, -90))],
    "encantar": [("F", poly([(18, 25), (25, 15), (39, 15), (46, 25), (32, 50)])),
                 ("DS", linha((18, 25), (46, 25)), 2.2), ("DS", linha((25, 15), (32, 25), (39, 15)), 2.2), ("DS", linha((32, 25), (32, 50)), 2.2),
                 ("F", estrela(51, 12, 6, 1.8, 4)), ("F", estrela(12, 44, 5, 1.6, 4))],
    "mapa": [("F", poly([(7, 18), (22, 12), (42, 18), (57, 12), (57, 46), (42, 52), (22, 46), (7, 52)])),
             ("DS", linha((22, 12), (22, 46)), 2.5), ("DS", linha((42, 18), (42, 52)), 2.5),
             ("D", "M32,24a6,6 0 0 1 6,6c0,5 -6,11 -6,11c0,0 -6,-6 -6,-11a6,6 0 0 1 6,-6Z")],
    "aventuras": [("F", "M10,55V31a22,22 0 0 1 44,0V55Z" + "M21,55V33a11,11 0 0 1 22,0V55Z"),
                  ("DS", linha((10, 42), (21, 42)), 2), ("DS", linha((43, 42), (54, 42)), 2),
                  ("D", "M21,55V33a11,11 0 0 1 22,0V55Z")],
    "correio": [("F", rrect(9, 15, 46, 34, 6)), ("DS", linha((12, 19), (32, 35), (52, 19)), 3.5)],
    "clan": [("S", linha((15, 9), (15, 57)), 4.5),
             ("F", "M18,11H52L44,24L52,37H18Z"), ("D", estrela(32, 24, 6.5, 2.8))],
    "lojas": [("F", "M7,27L14,11H50L57,27Q51,32 45,27Q39,32 32,27Q25,32 19,27Q13,32 7,27Z"),
              ("F", rrect(11, 31, 42, 23, 3)), ("D", rrect(26, 38, 12, 16, 2))],
    "mercado": [("S", linha((32, 11), (32, 49)), 4.5), ("S", linha((12, 20), (52, 20)), 4.5),
                ("S", linha((18, 20), (10, 36)), 2), ("S", linha((18, 20), (26, 36)), 2),
                ("S", linha((46, 20), (38, 36)), 2), ("S", linha((46, 20), (54, 36)), 2),
                ("F", "M7,36a11,8 0 0 0 22,0Z"), ("F", "M35,36a11,8 0 0 0 22,0Z"),
                ("F", rrect(21, 49, 22, 6, 3)), ("F", circ(32, 10, 3.5))],
    "loja_tp": [("F", poly([(14, 23), (23, 12), (41, 12), (50, 23), (32, 53)])),
                ("DS", linha((14, 23), (50, 23)), 2.2), ("DS", linha((23, 12), (28, 23), (32, 53)), 2), ("DS", linha((41, 12), (36, 23), (32, 53)), 2)],
    "barra_itens": [("F", rrect(6, 20, 15, 24, 4.5)), ("F", rrect(24.5, 20, 15, 24, 4.5)), ("F", rrect(43, 20, 15, 24, 4.5)),
                    ("D", rrect(10, 25, 7, 7, 2)), ("D", rrect(28.5, 25, 7, 7, 2)), ("D", rrect(47, 25, 7, 7, 2))],
    "trocar_personagem": pessoa(32, 36, 0.8) +
                         [("S", arco(32, 32, 25, 195, 265), 4),
                          ("F", poly(gira([(0, -5), (6, 0), (0, 5)], 32 + 25 * math.cos(math.radians(265)), 32 + 25 * math.sin(math.radians(265)), 265 + 90))),
                          ("S", arco(32, 32, 25, 15, 85), 4),
                          ("F", poly(gira([(0, -5), (6, 0), (0, 5)], 32 + 25 * math.cos(math.radians(85)), 32 + 25 * math.sin(math.radians(85)), 85 + 90)))],
    "sair": [("F", rrect(10, 9, 26, 46, 4)), ("D", circ(30, 33, 2.6)),
             ("S", linha((34, 32), (52, 32)), 5.5), ("F", poly([(47, 23), (58, 32), (47, 41)]))],
}

MAPA = {
    "jogador": [("F", poly([(32, 7), (53, 55), (32, 43), (11, 55)]))],
    "destino": [("S", circ(32, 32, 19), 5), ("S", linha((32, 5), (32, 16)), 4.5), ("S", linha((32, 48), (32, 59)), 4.5),
                ("S", linha((5, 32), (16, 32)), 4.5), ("S", linha((48, 32), (59, 32)), 4.5), ("F", circ(32, 32, 5.5))],
    "cidade": [("F", "M11,55V26H18V19H24V26H29V15H35V26H40V19H46V26H53V55Z"), ("D", "M26,55V45a6,6 0 0 1 12,0V55Z")],
    "porto": [("S", circ(32, 13, 5.5), 4), ("S", linha((32, 19), (32, 53)), 5), ("S", linha((20, 26), (44, 26)), 5),
              ("S", "M11,39a21,17 0 0 0 42,0", 5),
              ("F", poly([(5, 42), (11, 33), (17, 42)])), ("F", poly([(47, 42), (53, 33), (59, 42)]))],
    "chefe": coroa_glifo(),
    "npc": [("F", rrect(8, 11, 48, 32, 11)), ("F", poly([(19, 40), (15, 55), (32, 40)])),
            ("D", circ(21, 27, 3.4)), ("D", circ(32, 27, 3.4)), ("D", circ(43, 27, 3.4))],
    "pedra": [("F", poly([(9, 53), (15, 31), (26, 22), (31, 53)])),
              ("F", poly([(24, 53), (31, 17), (40, 10), (46, 53)])),
              ("F", poly([(40, 53), (45, 30), (54, 36), (56, 53)])),
              ("DS", linha((31, 17), (35, 53)), 2), ("DS", linha((15, 31), (22, 53)), 2)],
    "madeira": [("F", circ(32, 23, 17)), ("F", circ(19, 31, 10)), ("F", circ(45, 31, 10)),
                ("F", rrect(28, 38, 8, 18, 2.5)), ("D", "M32,42l-6,-6M32,46l6,-6")],
    "lobo": [("F", poly([(32, 56), (15, 38), (11, 8), (24, 21), (32, 18), (40, 21), (53, 8), (49, 38)])),
             ("D", poly([(22, 32), (29, 34), (24, 37)])), ("D", poly([(42, 32), (35, 34), (40, 37)])),
             ("D", poly([(29, 46), (35, 46), (32, 51)]))],
    "urso": [("F", circ(32, 41, 13)), ("F", circ(14, 26, 6.5)), ("F", circ(25, 15, 6.5)), ("F", circ(39, 15, 6.5)), ("F", circ(50, 26, 6.5))],
    "tigre": [("S", "M17,10Q11,32 19,54", 6.5), ("S", "M32,8Q26,32 34,56", 6.5), ("S", "M47,10Q41,32 49,54", 6.5)],
    "owlbear": [("F", poly([(13, 26), (8, 6), (26, 17)])), ("F", poly([(51, 26), (56, 6), (38, 17)])),
                ("F", circ(32, 35, 21)), ("D", circ(23, 32, 6.8)), ("D", circ(41, 32, 6.8)),
                ("F", circ(23, 32, 2.6)), ("F", circ(41, 32, 2.6)), ("D", poly([(28.5, 41), (35.5, 41), (32, 50)]))],
    "pistoleiro": [("F", "M7,19H47V29H31L27,53H15L19,29H7Z"), ("F", rrect(45, 19, 12, 6, 2.5)), ("D", "M31,29a5,6 0 0 1 -6,9")],
    "mago": [("F", "M8,50H56L44,45L34,6L24,28L19,45Z"), ("D", rrect(19, 42, 26, 5, 2)), ("F", estrela(45, 16, 5, 2, 4))],
    "arqueiro": [("S", "M24,7Q54,32 24,57", 5.5), ("S", linha((24, 7), (24, 57)), 2.2),
                 ("S", linha((7, 32), (47, 32)), 3.2), ("F", poly([(45, 25), (58, 32), (45, 39)])),
                 ("F", poly([(7, 27), (13, 32), (7, 37), (10, 32)]))],
    "caranguejo": [("S", linha((21, 33), (14, 25)), 4), ("S", linha((43, 33), (50, 25)), 4),
                   ("F", circ(12, 21, 7.5)), ("F", circ(52, 21, 7.5)), ("D", poly([(12, 21), (5, 15), (10, 13)])), ("D", poly([(52, 21), (59, 15), (54, 13)])),
                   ("S", linha((19, 44), (8, 51)), 3), ("S", linha((22, 48), (13, 57)), 3),
                   ("S", linha((45, 44), (56, 51)), 3), ("S", linha((42, 48), (51, 57)), 3),
                   ("F", elipse(32, 40, 17, 11.5)), ("S", linha((27, 30), (27, 22)), 2.6), ("S", linha((37, 30), (37, 22)), 2.6),
                   ("F", circ(27, 20, 3)), ("F", circ(37, 20, 3))],
}

# Paletas das skills: (claro, escuro, aro, tom do glifo).
PALETAS = {
    "espada": ("#ffd27a", "#5a3206", "#ffe2a3", "#fff4dc"),
    "katana": ("#86e9ff", "#08344d", "#b0f3ff", "#eafcff"),
    "pistola": ("#ffa06a", "#561a04", "#ffc09a", "#fff0e6"),
    "vida": ("#86ffb9", "#0a4428", "#b9ffd6", "#effff5"),
    "arcano": ("#d8b0ff", "#33115c", "#e6ccff", "#f6eeff"),
}

SKILLS = {
    1: ("espada", espada(34, 30, 45, 0.9) + [("S", linha((9, 47), (19, 37)), 3), ("S", linha((14, 55), (24, 45)), 3), ("S", linha((7, 38), (13, 32)), 2.5)]),
    2: ("espada", [("S", arco(32, 32, 24, 150, 300), 5)] + espada(34, 34, -30, 0.78)),
    3: ("espada", [("F", "M32,8L52,16V31C52,44 43,52 32,57C21,52 12,44 12,31V16Z"),
                   ("D", rrect(29, 18, 6, 30, 3)), ("D", rrect(19, 28, 26, 6, 3))]),
    4: ("katana", [("F", poly(gira([(-3.0, 0), (3.0, 0), (3.0, 24), (-3.0, 24)], 24, 38, 40, 0.8))),
                   ("D", poly(gira([(-3.0, 5), (3.0, 5), (3.0, 8), (-3.0, 8)], 24, 38, 40, 0.8)))] +
                  katana(35, 28, 40, 0.8) +
                  [("S", arco(32, 33, 21, 250, 315), 2.6)]),
    5: ("katana", [("S", arco(32, 32, 21, a, a + 95), 5.5) for a in (-100, 20, 140)] +
                  [("S", arco(32, 32, 13, a + 20, a + 75), 3) for a in (-100, 20, 140)] +
                  [("F", circ(32, 32, 4))]),
    6: ("katana", [("S", arco(20, 32, 26, -62, 62), 8), ("S", arco(20, 32, 19, -45, 45), 3),
                   ("S", linha((6, 20), (18, 20)), 3), ("S", linha((4, 32), (14, 32)), 3), ("S", linha((6, 44), (18, 44)), 3)]),
    7: ("pistola", [("S", circ(32, 32, 19), 4), ("S", linha((32, 6), (32, 20)), 4), ("S", linha((32, 44), (32, 58)), 4),
                    ("S", linha((6, 32), (20, 32)), 4), ("S", linha((44, 32), (58, 32)), 4), ("F", circ(32, 32, 5.5))]),
    8: ("pistola", [("F", poly(gira([(-3.5, -10), (0, -15), (3.5, -10), (3.5, 8), (-3.5, 8)], 32 + 18 * math.sin(math.radians(a)), 36 - 18 * math.cos(math.radians(a)), a))) for a in (-32, 0, 32)] +
        [("S", linha((32, 55), (32, 44)), 3)]),
    9: ("pistola", [("F", rrect(17, 21, 30, 35, 9)), ("DS", linha((18, 31), (46, 31)), 2.6), ("DS", linha((18, 45), (46, 45)), 2.6),
                    ("S", "M40,21Q44,12 51,11", 3), ("F", estrela(53, 9, 7.5, 2.5, 6))]),
    10: ("vida", [("F", rrect(26, 12, 12, 40, 5)), ("F", rrect(12, 26, 40, 12, 5)),
                  ("F", estrela(51, 12, 6.5, 2, 4)), ("F", estrela(13, 50, 5, 1.6, 4)), ("F", estrela(52, 49, 4, 1.3, 4))]),
    11: ("vida", [("S", arco(32, 32, 25, 200, 340), 3.5), ("S", arco(32, 32, 25, 20, 160), 3.5),
                  ("S", circ(32, 32, 17), 2.2), ("F", rrect(28, 21, 8, 22, 3.5)), ("F", rrect(21, 28, 22, 8, 3.5))]),
    12: ("arcano", [("S", circ(32, 32, 24), 2.5), ("F", poly([(37, 5), (18, 35), (30, 35), (24, 59), (47, 25), (35, 25), (41, 5)]))]),
}

# ─────────────────────────────── loja (colorido) ───────────────────────────────
# A moeda premium (TP), a arte dos pacotes e a moeda de ouro: desenho
# COLORIDO (nao se tinge), celula de 128 pra ficar nitido de 16 a 128 px.
# Reusado em todo lugar que mostra TP (`hud_estilo::tp_texto`).

HUD["paleta"] = [("F", "M32,9C17,9 8,20 8,33C8,46 18,55 29,55C34,55 36,51 34,47.5C32,44 34,41 38,41H45C51.5,41 56,36.5 56,29C56,17.5 45,9 32,9Z"),
                 ("D", circ(20, 31, 4.2)), ("D", circ(27, 20, 4.2)), ("D", circ(39.5, 18.5, 4.2)), ("D", circ(48, 28, 4.2))]

# Facetas do cristal da tempestade, em volta do centro (32,33).
FACETAS = [
    ([(32, 4), (15, 19), (32, 33)], "#e6f9ff"),
    ([(32, 4), (49, 19), (32, 33)], "#9fd6ff"),
    ([(49, 19), (46, 45), (32, 33)], "#6166e8"),
    ([(46, 45), (32, 60), (32, 33)], "#3b2c9e"),
    ([(32, 60), (18, 45), (32, 33)], "#7080f2"),
    ([(18, 45), (15, 19), (32, 33)], "#b5e8ff"),
]
CONTORNO_CRISTAL = poly([(32, 4), (49, 19), (46, 45), (32, 60), (18, 45), (15, 19)])
RAIO_TP = "M36,13L24.5,35H32L27.5,52L41,28.5H33.5L38.5,13Z"


def cristal(tx, ty, esc=1.0, rot=0.0, raio=True):
    p = ['<g transform="translate(%s,%s) rotate(%s) scale(%s) translate(-32,-33)">' % (f(tx), f(ty), f(rot), f(esc))]
    p.append('<path d="%s" fill="#000" fill-opacity="0.45" transform="translate(1.6,2.6)"/>' % CONTORNO_CRISTAL)
    for pts, cor in FACETAS:
        p.append('<path d="%s" fill="%s"/>' % (poly(pts), cor))
    p.append('<path d="M32,4L15,19L22,23L32,10.5Z" fill="#ffffff" fill-opacity="0.6"/>')
    if raio:
        p.append('<path d="%s" fill="url(#raio)" stroke="#5a2e00" stroke-width="1.7" stroke-linejoin="round"/>' % RAIO_TP)
    p.append('<path d="%s" fill="none" stroke="#150d3a" stroke-width="2.7" stroke-linejoin="round"/>' % CONTORNO_CRISTAL)
    p.append('</g>')
    return "".join(p)


def faisca(cx, cy, r, op=0.95):
    return '<path d="%s" fill="#ffffff" fill-opacity="%s"/>' % (estrela(cx, cy, r, r * 0.26, 4), f(op))


def halo(cx, cy, r, op=0.8, ouro=False):
    return '<circle cx="%s" cy="%s" r="%s" fill="url(#%s)" fill-opacity="%s"/>' % (f(cx), f(cy), f(r), "halo_ouro" if ouro else "halo", f(op))


def bau_de_tp():
    madeira = 'fill="url(#madeira)" stroke="#2a1405" stroke-width="2.2" stroke-linejoin="round"'
    return (halo(32, 34, 32, 1.0, ouro=True) +
            '<path d="M10,33L15,17H49L54,33Z" %s/>' % madeira +
            '<path d="M15,17H49" stroke="#ffd36a" stroke-width="2.4"/>' +
            cristal(21, 28, 0.40, -22) + cristal(43, 27, 0.42, 18) + cristal(32, 22, 0.55) +
            '<rect x="8" y="33" width="48" height="23" rx="4" %s/>' % madeira +
            '<rect x="13" y="33" width="5" height="23" fill="url(#ouro)" stroke="#6b3f05" stroke-width="1"/>' +
            '<rect x="46" y="33" width="5" height="23" fill="url(#ouro)" stroke="#6b3f05" stroke-width="1"/>' +
            '<rect x="27.5" y="38" width="9" height="11" rx="2.5" fill="url(#ouro)" stroke="#6b3f05" stroke-width="1.2"/>' +
            '<circle cx="32" cy="43" r="1.6" fill="#3a2006"/>' +
            '<rect x="8" y="33" width="48" height="3" fill="#ffe7a0" fill-opacity="0.35"/>' +
            faisca(55, 9, 5) + faisca(8, 14, 3.6) + faisca(58, 30, 2.8))


LOJA = {
    "tp": halo(32, 33, 31, 0.9) + cristal(32, 33, 0.92) + faisca(53, 11, 5.2) + faisca(11, 50, 3.6),
    "tp_1": halo(32, 36, 26, 0.7) + cristal(32, 36, 0.62) + faisca(47, 15, 4),
    "tp_2": (halo(32, 36, 30, 0.85) + cristal(19, 42, 0.45, -18, False) + cristal(45, 42, 0.45, 18, False) +
             cristal(32, 34, 0.64) + faisca(52, 12, 4.6) + faisca(10, 21, 3)),
    "tp_3": (halo(32, 34, 31, 0.95) + '<ellipse cx="32" cy="53" rx="27" ry="8" fill="url(#monte)"/>' +
             cristal(13, 47, 0.36, -26, False) + cristal(51, 47, 0.36, 24, False) +
             cristal(22, 42, 0.46, -12, False) + cristal(42, 42, 0.46, 12, False) +
             cristal(32, 33, 0.66) + faisca(54, 11, 5) + faisca(9, 17, 3.4) + faisca(33, 5, 2.6)),
    "tp_4": bau_de_tp(),
    "ouro": ('<circle cx="33.5" cy="34.5" r="25" fill="#000" fill-opacity="0.35"/>'
             '<circle cx="32" cy="32" r="25" fill="url(#ouro)" stroke="#6b3f05" stroke-width="2.6"/>'
             '<circle cx="32" cy="32" r="19" fill="none" stroke="#fff1bf" stroke-opacity="0.7" stroke-width="1.8"/>'
             '<path d="%s" fill="#fff0b0" fill-opacity="0.95" stroke="#9a5d0a" stroke-width="1.2" stroke-linejoin="round"/>'
             '<ellipse cx="24" cy="20" rx="9" ry="4.5" fill="#ffffff" fill-opacity="0.45" transform="rotate(-30 24 20)"/>'
             % estrela(32, 32.5, 11, 4.6, 5)),
}


def svg_cor(corpo):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><defs>'
            '<radialGradient id="halo"><stop offset="0" stop-color="#9ae6ff" stop-opacity="0.8"/>'
            '<stop offset="0.5" stop-color="#7d6bff" stop-opacity="0.28"/><stop offset="1" stop-color="#7d6bff" stop-opacity="0"/></radialGradient>'
            '<radialGradient id="halo_ouro"><stop offset="0" stop-color="#ffe492" stop-opacity="0.85"/>'
            '<stop offset="0.5" stop-color="#ff9f35" stop-opacity="0.28"/><stop offset="1" stop-color="#ff9f35" stop-opacity="0"/></radialGradient>'
            '<radialGradient id="monte"><stop offset="0" stop-color="#a596ff" stop-opacity="0.75"/><stop offset="1" stop-color="#4a3cc0" stop-opacity="0"/></radialGradient>'
            '<linearGradient id="raio" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fffbe2"/>'
            '<stop offset="0.5" stop-color="#ffc53d"/><stop offset="1" stop-color="#ff8a1a"/></linearGradient>'
            '<linearGradient id="ouro" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff2ad"/>'
            '<stop offset="0.55" stop-color="#f2b53b"/><stop offset="1" stop-color="#b8720f"/></linearGradient>'
            '<linearGradient id="madeira" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#a06a34"/>'
            '<stop offset="1" stop-color="#5a3313"/></linearGradient>'
            '</defs>%s</svg>') % corpo


# ─────────────────────────────── SVG ───────────────────────────────


def svg_mono(elems):
    sombra, topo = [], []
    for e in elems:
        k, d = e[0], e[1]
        if k == "F":
            sombra.append('<path d="%s" fill="#000" fill-opacity="0.5" stroke="#000" stroke-opacity="0.5" stroke-width="4.5" stroke-linejoin="round" fill-rule="evenodd"/>' % d)
            topo.append('<path d="%s" fill="url(#g)" fill-rule="evenodd"/>' % d)
        elif k == "S":
            w = e[2]
            sombra.append('<path d="%s" fill="none" stroke="#000" stroke-opacity="0.5" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(w + 4.5)))
            topo.append('<path d="%s" fill="none" stroke="url(#g)" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(w)))
        elif k == "D":
            topo.append('<path d="%s" fill="#0d1320" fill-opacity="0.82" fill-rule="evenodd"/>' % d)
        elif k == "DS":
            topo.append('<path d="%s" fill="none" stroke="#0d1320" stroke-opacity="0.82" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(e[2])))
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
            '<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="6" x2="0" y2="58">'
            '<stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#bcc6d4"/></linearGradient></defs>'
            '<g>%s</g><g>%s</g></svg>') % ("".join(sombra), "".join(topo))


def svg_skill(paleta, elems):
    claro, escuro, aro, glifo = PALETAS[paleta]
    sombra, topo = [], []
    for e in elems:
        k, d = e[0], e[1]
        if k == "F":
            sombra.append('<path d="%s" fill="#000" fill-opacity="0.55" stroke="#000" stroke-opacity="0.55" stroke-width="4" stroke-linejoin="round" fill-rule="evenodd"/>' % d)
            topo.append('<path d="%s" fill="url(#gl)" fill-rule="evenodd"/>' % d)
        elif k == "S":
            sombra.append('<path d="%s" fill="none" stroke="#000" stroke-opacity="0.55" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(e[2] + 4)))
            topo.append('<path d="%s" fill="none" stroke="url(#gl)" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(e[2])))
        elif k == "D":
            topo.append('<path d="%s" fill="%s" fill-opacity="0.9" fill-rule="evenodd"/>' % (d, escuro))
        elif k == "DS":
            topo.append('<path d="%s" fill="none" stroke="%s" stroke-opacity="0.9" stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, escuro, f(e[2])))
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><defs>'
            '<radialGradient id="bg" cx="32" cy="22" r="40" gradientUnits="userSpaceOnUse">'
            '<stop offset="0" stop-color="%s"/><stop offset="1" stop-color="%s"/></radialGradient>'
            '<linearGradient id="gl" gradientUnits="userSpaceOnUse" x1="0" y1="8" x2="0" y2="56">'
            '<stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="%s"/></linearGradient>'
            '<linearGradient id="brilho" gradientUnits="userSpaceOnUse" x1="0" y1="3" x2="0" y2="30">'
            '<stop offset="0" stop-color="#ffffff" stop-opacity="0.28"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></linearGradient>'
            '</defs>'
            '<circle cx="32" cy="32" r="30" fill="url(#bg)"/>'
            '<g>%s</g><g>%s</g>'
            '<path d="M8,28a24,22 0 0 1 48,0a30,14 0 0 0 -48,0Z" fill="url(#brilho)"/>'
            '<circle cx="32" cy="32" r="29" fill="none" stroke="%s" stroke-width="2.4"/>'
            '<circle cx="32" cy="32" r="26" fill="none" stroke="#000" stroke-opacity="0.25" stroke-width="1.2"/>'
            '</svg>') % (claro, escuro, glifo, "".join(sombra), "".join(topo), aro)


# ─────────────────────────────── raster e atlas ───────────────────────────────


def rasteriza(svg, lado, tmp):
    grande = lado * 3
    caminho_svg = os.path.join(tmp, "i.svg")
    caminho_png = os.path.join(tmp, "i.png")
    with open(caminho_svg, "w") as fh:
        fh.write(svg)
    subprocess.run(["rsvg-convert", "-w", str(grande), "-h", str(grande), caminho_svg, "-o", caminho_png], check=True)
    img = Image.open(caminho_png).convert("RGBA")
    img.load()
    # Reduz em alfa pre-multiplicado: sem franja escura na borda do contorno.
    return img.convert("RGBa").resize((lado, lado), Image.LANCZOS).convert("RGBA")


def monta(nomeados, lado, colunas, arquivo, tmp, gerador):
    linhas = (len(nomeados) + colunas - 1) // colunas
    atlas = Image.new("RGBA", (colunas * lado, linhas * lado), (0, 0, 0, 0))
    indice = []
    for n, (chave, dado) in enumerate(nomeados):
        img = rasteriza(gerador(dado), lado, tmp)
        atlas.paste(img, ((n % colunas) * lado, (n // colunas) * lado))
        indice.append((chave, n))
    atlas.save(os.path.join(SAIDA, arquivo), format="PNG", optimize=False)
    return indice, colunas * lado, linhas * lado


def main():
    os.makedirs(SAIDA, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        hud = sorted(HUD.items())
        mapa = sorted(MAPA.items())
        skills = sorted(SKILLS.items())
        i_hud, w_hud, h_hud = monta(hud, 64, 8, "hud.png", tmp, svg_mono)
        i_mapa, w_mapa, h_mapa = monta(mapa, 48, 8, "mapa.png", tmp, svg_mono)
        i_sk, w_sk, h_sk = monta(skills, 96, 6, "skills.png", tmp, lambda d: svg_skill(d[0], d[1]))
        loja = sorted(LOJA.items())
        i_loja, w_loja, h_loja = monta(loja, 128, 4, "loja.png", tmp, svg_cor)

    out = ["// GERADO por tools/icones/gerar_icones_ui.py — nao edite a mao.",
           "// Rode `python3 tools/icones/gerar_icones_ui.py` pra refazer atlas e indice.", ""]
    for nome, lado, colunas, w, h in (("UI", 64, 8, w_hud, h_hud), ("MAPA", 48, 8, w_mapa, h_mapa), ("SKILLS", 96, 6, w_sk, h_sk), ("LOJA", 128, 4, w_loja, h_loja)):
        out += ["pub const LADO_%s: u32 = %d;" % (nome, lado),
                "pub const COLUNAS_%s: u32 = %d;" % (nome, colunas),
                "pub const LARGURA_%s: u32 = %d;" % (nome, w),
                "pub const ALTURA_%s: u32 = %d;" % (nome, h), ""]
    out.append("/// (nome, celula), ordenado por nome.")
    out.append("pub const UI: &[(&str, u16)] = &[")
    out += ['    ("%s", %d),' % e for e in i_hud]
    out += ["];", "", "/// (nome, celula), ordenado por nome.", "pub const MAPA: &[(&str, u16)] = &["]
    out += ['    ("%s", %d),' % e for e in i_mapa]
    out += ["];", "", "/// (id da skill, celula), ordenado por id.", "pub const SKILLS: &[(u32, u16)] = &["]
    out += ["    (%d, %d)," % e for e in i_sk]
    out += ["];", "", "/// (nome, celula), ordenado por nome. Arte colorida da loja (TP, pacotes, ouro).", "pub const LOJA: &[(&str, u16)] = &["]
    out += ['    ("%s", %d),' % e for e in i_loja]
    out += ["];", ""]
    with open(INDICE, "w") as fh:
        fh.write("\n".join(out))
    print("hud %d, mapa %d, skills %d, loja %d icones" % (len(hud), len(mapa), len(skills), len(loja)))


if __name__ == "__main__":
    main()
