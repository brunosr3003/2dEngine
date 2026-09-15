#!/usr/bin/env python3
"""Icones dos itens: um icone ILUSTRADO e unico por item, atlas + indice Rust.

Mesma tecnica dos icones do HUD/skills/mapa (`gerar_icones_ui.py`): cada item
e' um SVG escrito aqui (viewBox 64), `rsvg-convert` rasteriza a 3x a celula e
o Pillow reduz com LANCZOS em alfa PRE-MULTIPLICADO (sem franja escura na
borda). Deterministico: mesma entrada -> mesmo PNG, byte a byte.

    python3 tools/icones/gerar_icones.py

Saidas:
  assets/icones/itens.png            atlas RGBA, celulas de LADO x LADO
  crates/client/src/icones_indice.rs tabela item_id -> celula (ordenada por id)

Estilo, igual ao HUD novo: silhueta clara por categoria, contorno escuro
uniforme tirado da propria cor, luz de cima-esquerda (gradiente de 3 tons),
brilho especular pequeno e sombra curta embaixo-direita. A cor do TIER
(cinza/verde/azul/roxo, a mesma de `shared::items::tier_color_hex`) entra nos
materiais, na madeira e no couro, com aura nos tiers altos. A moldura de
raridade NAO e' assada: o cliente desenha em volta (`icones::icone`), porque a
raridade de uma peca e' da instancia e nao do id.
"""

import math
import os
import subprocess
import tempfile

from PIL import Image

LADO = 96
COLUNAS = 10
RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

TIER = {1: (191, 191, 191), 2: (95, 211, 95), 3: (85, 119, 255), 4: (170, 85, 255), 5: (255, 119, 51)}

ACO = (196, 204, 216)
OURO = (240, 192, 70)
COBRE = (206, 118, 66)
COURO = (150, 96, 56)
MADEIRA = (140, 94, 54)
MADEIRA_CLARA = (222, 180, 120)
VIDRO = (214, 234, 244)
BRANCO = (255, 255, 255)
ESCURO = (34, 30, 44)
VERMELHO = (222, 44, 58)
AZUL = (58, 108, 240)
VERDE = (72, 196, 92)


# ─────────────────────────────── cor ───────────────────────────────


def mul(c, k):
    return tuple(max(0, min(255, int(round(v * k)))) for v in c[:3])


def mix(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def hx(c):
    return "#%02x%02x%02x" % tuple(c[:3])


# ─────────────────────────────── geometria ───────────────────────────────


def f(v):
    return ("%.2f" % v).rstrip("0").rstrip(".")


def circ(cx, cy, r):
    return elipse(cx, cy, r, r)


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


def estrela(cx, cy, ro, ri, n=5, rot=-90):
    pts = []
    for k in range(2 * n):
        r = ro if k % 2 == 0 else ri
        a = math.radians(rot + k * 180.0 / n)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return poly(pts)


def rot(pts, ox, oy, graus):
    a = math.radians(graus)
    c, s = math.cos(a), math.sin(a)
    return [(ox + x * c - y * s, oy + x * s + y * c) for x, y in pts]


# ─────────────────────────────── desenho ───────────────────────────────


class Svg:
    """Camadas: aura (fundo), sombras e pecas. A sombra de toda peca vai numa
    camada so', embaixo de tudo: sombra de peca posterior nao suja a anterior."""

    def __init__(self):
        self.defs, self.aura, self.sombras, self.corpo = [], [], [], []
        self.n = 0

    def _id(self, p):
        self.n += 1
        return "%s%d" % (p, self.n)

    def grad(self, cor, claro=0.42, escuro=0.58):
        i = self._id("g")
        self.defs.append(
            '<linearGradient id="%s" x1="0.15" y1="0" x2="0.85" y2="1">'
            '<stop offset="0" stop-color="%s"/><stop offset="0.5" stop-color="%s"/>'
            '<stop offset="1" stop-color="%s"/></linearGradient>'
            % (i, hx(mix(cor, BRANCO, claro)), hx(cor), hx(mul(cor, escuro))))
        return i

    def peca(self, d, cor, contorno=2.2, sombra=True, claro=0.42, escuro=0.58):
        if sombra:
            self.sombras.append('<path d="%s" transform="translate(1.3,2)" fill="#000" stroke="#000" '
                                'stroke-width="%s" stroke-linejoin="round" fill-rule="evenodd"/>' % (d, f(contorno)))
        self.corpo.append('<path d="%s" fill="url(#%s)" stroke="%s" stroke-width="%s" stroke-linejoin="round" '
                          'fill-rule="evenodd"/>' % (d, self.grad(cor, claro, escuro), hx(mul(cor, 0.3)), f(contorno)))

    def chapado(self, d, cor, op=1.0):
        self.corpo.append('<path d="%s" fill="%s" fill-opacity="%s" fill-rule="evenodd"/>' % (d, hx(cor), f(op)))

    def traco(self, d, cor, w, contorno=True, sombra=True, op=1.0):
        if sombra:
            self.sombras.append('<path d="%s" transform="translate(1.3,2)" fill="none" stroke="#000" '
                                'stroke-width="%s" stroke-linecap="round" stroke-linejoin="round"/>' % (d, f(w + 2.2)))
        if contorno:
            self.corpo.append('<path d="%s" fill="none" stroke="%s" stroke-width="%s" stroke-linecap="round" '
                              'stroke-linejoin="round"/>' % (d, hx(mul(cor, 0.3)), f(w + 2.2)))
        self.corpo.append('<path d="%s" fill="none" stroke="%s" stroke-opacity="%s" stroke-width="%s" '
                          'stroke-linecap="round" stroke-linejoin="round"/>' % (d, hx(cor), f(op), f(w)))

    def brilho(self, cx, cy, rx, ry, op=0.85, graus=-30):
        self.corpo.append('<ellipse cx="%s" cy="%s" rx="%s" ry="%s" fill="#fff" fill-opacity="%s" '
                          'transform="rotate(%s %s %s)"/>' % (f(cx), f(cy), f(rx), f(ry), f(op), f(graus), f(cx), f(cy)))

    def faisca(self, cx, cy, r, cor=BRANCO, op=0.95):
        self.corpo.append('<path d="%s" fill="%s" fill-opacity="%s"/>' % (estrela(cx, cy, r, r * 0.28, 4, -90), hx(cor), f(op)))

    def halo(self, cx, cy, r, cor, op):
        i = self._id("h")
        self.defs.append('<radialGradient id="%s" cx="%s" cy="%s" r="%s" gradientUnits="userSpaceOnUse">'
                         '<stop offset="0" stop-color="%s" stop-opacity="%s"/>'
                         '<stop offset="1" stop-color="%s" stop-opacity="0"/></radialGradient>'
                         % (i, f(cx), f(cy), f(r), hx(cor), f(op), hx(cor)))
        self.aura.append('<circle cx="%s" cy="%s" r="%s" fill="url(#%s)"/>' % (f(cx), f(cy), f(r), i))

    def recorte(self, d_clip, conteudo):
        """`conteudo` (lista de elementos SVG) recortado pela forma `d_clip`."""
        i = self._id("c")
        self.defs.append('<clipPath id="%s"><path d="%s"/></clipPath>' % (i, d_clip))
        self.corpo.append('<g clip-path="url(#%s)">%s</g>' % (i, "".join(conteudo)))

    def texto(self):
        return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
                '<defs>%s</defs><g>%s</g><g fill-opacity="0.32" stroke-opacity="0.32">%s</g><g>%s</g></svg>'
                % ("".join(self.defs), "".join(self.aura), "".join(self.sombras), "".join(self.corpo)))


def aura_do_tier(s, tier, cx=32, cy=33, r=30):
    if tier >= 3:
        s.halo(cx, cy, r, TIER[tier], 0.42 if tier == 3 else 0.6)


# ─────────────────────────────── moedas e metal ───────────────────────────────


def moeda(s, cx, cy, r, cor, marca=True):
    s.peca(elipse(cx, cy + r * 0.28, r, r * 0.55), mul(cor, 0.72))
    s.peca(elipse(cx, cy, r, r * 0.55), cor, sombra=False)
    if marca:
        s.traco(elipse(cx, cy, r * 0.62, r * 0.33), mul(cor, 0.78), 1.4, contorno=False, sombra=False)
    s.brilho(cx - r * 0.35, cy - r * 0.18, r * 0.28, r * 0.1, 0.8, -12)


def ouro(s):
    for cx, cy, r in ((22, 45, 13), (41, 44, 13), (31, 34, 13), (30, 22, 11)):
        moeda(s, cx, cy, r, OURO)
    s.faisca(47, 16, 6)
    s.faisca(14, 26, 3.5)


def cobre(s):
    d = circ(32, 33, 22) + rrect(26, 27, 12, 12, 2)
    s.peca(d, COBRE, contorno=2.6)
    s.traco(circ(32, 33, 16.5), mul(COBRE, 0.72), 1.6, contorno=False, sombra=False)
    for a in range(0, 360, 90):
        x, y = 32 + 12 * math.cos(math.radians(a + 45)), 33 + 12 * math.sin(math.radians(a + 45))
        s.chapado(circ(x, y, 1.6), mul(COBRE, 0.6))
    s.brilho(22, 20, 7, 2.6, 0.7)


def lingote(s, cor, brilho=True):
    topo = poly([(10, 31), (37, 22), (55, 29), (28, 39)])
    frente = poly([(10, 31), (28, 39), (28, 51), (10, 43)])
    lado = poly([(28, 39), (55, 29), (55, 41), (28, 51)])
    s.peca(frente, mul(cor, 0.78))
    s.peca(lado, mul(cor, 0.6))
    s.peca(topo, mix(cor, BRANCO, 0.12), sombra=False)
    if brilho:
        s.traco(linha((18, 30), (35, 25)), BRANCO, 1.8, contorno=False, sombra=False, op=0.7)


def darksteel(s):
    s.halo(34, 36, 28, (120, 110, 200), 0.35)
    lingote(s, (86, 88, 118))
    s.traco(linha((32, 45), (38, 36), (46, 36)), (160, 150, 255), 1.4, contorno=False, sombra=False, op=0.85)


def po_cintilante(s):
    saco = "M20,26C12,34 12,52 32,54C52,52 52,34 44,26Z"
    s.peca(saco, (196, 150, 220))
    s.peca(rrect(22, 18, 20, 9, 4), (170, 124, 200), sombra=False)
    s.traco(linha((21, 27), (43, 27)), (250, 212, 90), 2.2, sombra=False)
    s.brilho(24, 36, 3.2, 7, 0.45, 15)
    for x, y, r in ((46, 14, 6), (14, 18, 4.5), (52, 38, 3.5), (30, 9, 3)):
        s.faisca(x, y, r, (255, 240, 200))


# ─────────────────────────────── pocoes ───────────────────────────────


def frasco(s, cor, forma, marca=None):
    if forma == "redondo":
        corpo, nivel, (px, py, pw, ph) = circ(32, 40, 16), 36, (27, 15, 10, 13)
    elif forma == "grande":
        corpo, nivel, (px, py, pw, ph) = circ(32, 40, 19), 33, (26, 12, 12, 12)
    elif forma == "alto":
        corpo, nivel, (px, py, pw, ph) = rrect(20, 22, 24, 36, 9), 30, (26, 12, 12, 12)
    elif forma == "quadrado":
        corpo, nivel, (px, py, pw, ph) = rrect(15, 26, 34, 31, 7), 34, (26, 15, 12, 13)
    else:  # coracao
        corpo = "M32,58C16,48 10,40 10,31C10,23 16,19 22,19C27,19 30,22 32,25C34,22 37,19 42,19C48,19 54,23 54,31C54,40 48,48 32,58Z"
        nivel, (px, py, pw, ph) = 32, (27, 11, 10, 11)
    pesc = rrect(px, py, pw, ph, 2)
    boca = rrect(px - 3, py - 3, pw + 6, 5, 2)
    rolha = rrect(px + 1, py - 9, pw - 2, 8, 2.5)
    s.peca(pesc, VIDRO, contorno=2.0, claro=0.2, escuro=0.8)
    s.peca(corpo, VIDRO, contorno=2.4, claro=0.2, escuro=0.8)
    liquido = s._id("l")
    s.defs.append('<linearGradient id="%s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="%s"/>'
                  '<stop offset="1" stop-color="%s"/></linearGradient>'
                  % (liquido, hx(mix(cor, BRANCO, 0.22)), hx(mul(cor, 0.55))))
    s.recorte(corpo, [
        '<rect x="0" y="%s" width="64" height="64" fill="url(#%s)"/>' % (f(nivel), liquido),
        '<ellipse cx="32" cy="%s" rx="22" ry="2.6" fill="%s"/>' % (f(nivel), hx(mix(cor, BRANCO, 0.45))),
    ])
    s.traco(corpo, mul(VIDRO, 0.32), 2.4, contorno=False, sombra=False)
    s.peca(boca, VIDRO, contorno=1.8, sombra=False, claro=0.3, escuro=0.75)
    s.peca(rolha, (170, 118, 70), contorno=1.8, sombra=False)
    s.brilho(24, nivel - 2, 3, 7.5, 0.7, 18)
    if marca == "mais":
        s.traco(linha((32, 38), (32, 50)), BRANCO, 3.2, sombra=False)
        s.traco(linha((26, 44), (38, 44)), BRANCO, 3.2, sombra=False)
    elif marca == "estrela":
        s.peca(estrela(33, 45, 8, 3.6), (255, 246, 196), contorno=1.4, sombra=False)
    elif marca == "moeda":
        moeda(s, 33, 45, 8, OURO)
    elif marca == "trevo":
        for dx, dy in ((0, -4), (-4.2, 2), (4.2, 2)):
            s.peca(circ(32 + dx, 44 + dy, 3.8), (110, 220, 120), contorno=1.2, sombra=False)
        s.traco(linha((32, 46), (34, 52)), (80, 160, 90), 1.4, contorno=False, sombra=False)


# ─────────────────────────────── madeira, peixe, barco ───────────────────────────────


def madeira(s, tier):
    casca = mix(MADEIRA, TIER[tier], 0.3)
    miolo = mix(MADEIRA_CLARA, TIER[tier], 0.22)
    aura_do_tier(s, tier)
    for (x, y) in ((8, 36), (26, 42), (17, 22)):
        s.peca(rrect(x, y, 34, 15, 7.5), casca, contorno=2.2)
        s.traco(linha((x + 8, y + 5), (x + 26, y + 5)), mul(casca, 0.72), 1.2, contorno=False, sombra=False)
        s.peca(elipse(x + 34, y + 7.5, 5.5, 7.5), miolo, contorno=2.0, sombra=False)
        s.traco(elipse(x + 34, y + 7.5, 2.8, 4), mul(miolo, 0.72), 1.1, contorno=False, sombra=False)


def peixe(s, k):
    if k == 0:  # anchova: comprida, prata
        cor, corpo = (176, 196, 212), elipse(30, 32, 21, 7.5)
        cauda = poly([(49, 32), (60, 23), (57, 32), (60, 41)])
    elif k == 1:  # palhaco
        cor, corpo = (246, 122, 40), elipse(29, 32, 18, 12)
        cauda = poly([(45, 32), (58, 22), (55, 32), (58, 42)])
    elif k == 2:  # cirurgiao
        cor, corpo = (52, 94, 214), "M8,32C14,18 36,14 46,26L46,38C36,50 14,46 8,32Z"
        cauda = poly([(45, 32), (58, 21), (55, 32), (58, 43)])
    else:  # baiacu
        cor, corpo = (224, 190, 96), circ(30, 33, 17)
        cauda = poly([(45, 33), (56, 26), (54, 33), (56, 40)])
    cor_cauda = (250, 212, 60) if k == 2 else mul(cor, 0.9)
    s.peca(cauda, cor_cauda)
    if k == 3:
        for a in range(0, 360, 30):
            x0, y0 = 30 + 16 * math.cos(math.radians(a)), 33 + 16 * math.sin(math.radians(a))
            x1, y1 = 30 + 22 * math.cos(math.radians(a)), 33 + 22 * math.sin(math.radians(a))
            s.traco(linha((x0, y0), (x1, y1)), (240, 226, 170), 2.0)
    s.peca(corpo, cor, contorno=2.4)
    if k == 1:
        s.recorte(corpo, ['<rect x="%s" y="0" width="5" height="64" fill="#fff"/>' % f(x) for x in (16, 29)])
        s.traco(corpo, mul(cor, 0.3), 2.4, contorno=False, sombra=False)
    s.peca(poly([(22, 24), (32, 14), (36, 25)]) if k != 3 else poly([(26, 17), (32, 10), (36, 18)]), mul(cor, 0.85), contorno=1.8, sombra=False)
    olho = (16, 29) if k != 3 else (20, 29)
    s.chapado(circ(*olho, 3.2), BRANCO)
    s.chapado(circ(olho[0] - 0.6, olho[1], 1.7), ESCURO)
    s.brilho(26, 26, 6, 1.8, 0.55, -10)


def barco(s, esquife):
    casco = "M6,40L58,40C54,50 46,55 32,55C18,55 10,50 6,40Z"
    if esquife:
        s.traco(linha((14, 22), (46, 50)), (176, 128, 80), 2.6)
        s.peca(poly([(42, 46), (50, 53), (46, 57), (38, 50)]), (176, 128, 80), contorno=1.6)
        s.peca(casco, (150, 98, 58))
        s.traco(linha((9, 44), (55, 44)), (110, 70, 40), 1.4, contorno=False, sombra=False)
        s.peca(rrect(24, 36, 16, 5, 1.5), (188, 138, 88), contorno=1.6, sombra=False)
    else:
        s.traco(linha((32, 8), (32, 42)), (120, 80, 48), 2.6)
        s.peca("M34,10C48,16 52,28 50,38L34,38Z", (246, 240, 226), contorno=1.8)
        s.peca("M30,14C20,20 16,30 18,38L30,38Z", (232, 226, 210), contorno=1.8)
        s.peca(poly([(32, 8), (42, 5), (32, 2)]), VERMELHO, contorno=1.4, sombra=False)
        s.peca(casco, (126, 84, 52))
        s.traco(linha((9, 45), (55, 45)), (240, 200, 90), 1.6, contorno=False, sombra=False)


# ─────────────────────────────── equipamento ───────────────────────────────


def lamina(s, pts_base, ox, oy, graus, cor=ACO):
    s.peca(poly(rot(pts_base, ox, oy, graus)), cor, contorno=2.0)


def espada_e_escudo(s):
    escudo = "M14,14L40,10L46,30C44,44 34,52 27,56C20,52 10,44 8,30Z"
    s.peca(escudo, (54, 96, 190), contorno=2.6)
    s.traco(escudo, (232, 190, 80), 1.6, contorno=False, sombra=False)
    s.peca(estrela(27, 31, 8, 3.5), (240, 196, 80), contorno=1.4, sombra=False)
    # espada diagonal por cima
    s.peca(poly(rot([(-3, 0), (3, 0), (2.5, -36), (0, -42), (-2.5, -36)], 38, 50, 38)), ACO, contorno=2.0)
    s.peca(poly(rot([(-10, -1.8), (10, -1.8), (10, 1.8), (-10, 1.8)], 38, 50, 38)), OURO, contorno=1.8)
    s.traco(linha(*rot([(0, 2), (0, 11)], 38, 50, 38)), (90, 58, 40), 3.6)
    s.peca(circ(*rot([(0, 12.5)], 38, 50, 38)[0], 2.6), OURO, contorno=1.5, sombra=False)
    s.brilho(*rot([(-0.8, -24)], 38, 50, 38)[0], 1, 8, 0.7, 38)


def katana(s):
    lam = "M10,54C24,40 40,22 56,8L58,10C44,26 28,44 14,58Z"
    s.peca(lam, (220, 228, 236), contorno=2.0)
    s.traco("M13,54C27,41 42,24 55,11", BRANCO, 1.0, contorno=False, sombra=False, op=0.8)
    s.peca(elipse(19, 49, 6.5, 3.2), (60, 56, 70), contorno=1.8, sombra=False)
    s.recorte("M4,64L16,52L24,58L10,64Z", [])
    s.traco(linha((8, 60), (17, 51)), (40, 36, 52), 5.2)
    for t in (0.2, 0.5, 0.8):
        x, y = 8 + 9 * t, 60 - 9 * t
        s.traco(linha((x - 1.8, y - 1.8), (x + 1.8, y + 1.8)), (214, 60, 70), 1.2, contorno=False, sombra=False)


def pistola(s, ox, oy, graus, espelho=False):
    sx = -1 if espelho else 1
    cano = [(sx * -2, -3), (sx * 30, -3), (sx * 30, 3), (sx * -2, 3)]
    cabo = [(sx * -2, -3), (sx * 6, 2), (sx * 2, 18), (sx * -8, 16), (sx * -6, 2)]
    s.peca(poly(rot(cabo, ox, oy, graus)), (138, 86, 48), contorno=2.0)
    s.peca(poly(rot(cano, ox, oy, graus)), (150, 156, 170), contorno=2.0)
    s.peca(circ(*rot([(sx * 2, 6)], ox, oy, graus)[0], 2.4), OURO, contorno=1.4, sombra=False)


def pistolas(s):
    pistola(s, 16, 40, -35)
    pistola(s, 48, 40, 35, espelho=True)


def anel_magico(s):
    s.halo(32, 22, 22, (120, 240, 160), 0.5)
    s.peca(elipse(32, 40, 19, 13) + elipse(32, 40, 13.5, 8), OURO, contorno=2.4)
    s.peca(poly([(24, 28), (32, 12), (40, 28), (32, 34)]), (70, 214, 130), contorno=2.0)
    s.traco(linha((32, 12), (32, 34)), BRANCO, 1, contorno=False, sombra=False, op=0.5)
    s.brilho(28, 22, 2, 5, 0.8, 20)
    s.faisca(47, 14, 4.5, (200, 255, 220))


def manto(s, cor, detalhe, gola):
    d = "M22,10L42,10C46,24 54,44 56,56C46,52 40,58 32,54C24,58 18,52 8,56C10,44 18,24 22,10Z"
    s.peca(d, cor, contorno=2.4)
    s.traco("M24,14C22,30 18,44 16,52", mul(cor, 0.7), 1.4, contorno=False, sombra=False)
    s.traco("M40,14C42,30 46,44 48,52", mul(cor, 0.7), 1.4, contorno=False, sombra=False)
    s.traco("M32,14C32,30 32,42 32,52", mul(cor, 0.75), 1.2, contorno=False, sombra=False)
    s.peca(gola, detalhe, contorno=1.8, sombra=False)


def manto_guerreiro(s):
    manto(s, (176, 38, 48), OURO, circ(32, 13, 5))
    s.chapado(circ(32, 13, 2), (140, 30, 40))


def manto_mago(s):
    d = "M20,16C20,6 44,6 44,16L50,56C40,52 36,58 32,55C28,58 24,52 14,56Z"
    s.peca(d, (58, 76, 176), contorno=2.4)
    s.peca("M24,16C24,9 40,9 40,16L38,22L26,22Z", (40, 52, 130), contorno=1.8, sombra=False)
    s.traco("M16,54C24,50 40,50 48,54", (240, 200, 90), 2.0, contorno=False, sombra=False)
    s.traco(linha((32, 24), (32, 52)), (240, 200, 90), 1.6, contorno=False, sombra=False)
    s.peca(estrela(26, 36, 4.5, 2), (255, 236, 150), contorno=1.0, sombra=False)
    s.faisca(40, 44, 3, (255, 236, 150))


def bainha(s):
    d = poly(rot([(-4, -26), (4, -26), (4.5, 22), (0, 27), (-4.5, 22)], 32, 32, 40))
    s.peca(d, (40, 34, 44), contorno=2.2)
    for t in (-18, 2, 20):
        s.peca(poly(rot([(-5.4, t - 2), (5.4, t - 2), (5.4, t + 2), (-5.4, t + 2)], 32, 32, 40)), OURO, contorno=1.4, sombra=False)
    s.traco("M16,20C10,30 12,40 18,44", (206, 60, 70), 2.2)
    s.peca(elipse(18, 46, 3, 5), (206, 60, 70), contorno=1.4)


def coldre(s):
    s.peca(rrect(8, 12, 48, 9, 3), (110, 72, 42), contorno=2.0)
    s.peca(rrect(28, 11, 10, 11, 2), (206, 206, 214), contorno=1.8, sombra=False)
    s.peca("M20,20L42,20L46,42C44,52 34,58 26,56C22,52 18,40 20,20Z", COURO, contorno=2.4)
    s.traco("M24,26L40,26", mul(COURO, 0.6), 1.2, contorno=False, sombra=False)
    for y in (32, 40, 48):
        s.chapado(circ(24 + (y - 32) * 0.15, y, 1.2), (236, 214, 170))
        s.chapado(circ(41 - (y - 32) * 0.1, y, 1.2), (236, 214, 170))


def colete(s, cor, borda):
    d = "M18,10L26,10C27,16 37,16 38,10L46,10L54,22L48,26L48,56L16,56L16,26L10,22Z"
    s.peca(d, cor, contorno=2.4)
    s.traco("M26,11C27,18 37,18 38,11", borda, 2.0, contorno=False, sombra=False)
    return d


def armadura_leve(s):
    d = colete(s, (156, 104, 62), (110, 70, 40))
    s.traco(linha((32, 20), (32, 55)), (100, 62, 36), 1.8, contorno=False, sombra=False)
    for y in (26, 34, 42):
        s.traco(linha((28, y), (36, y + 4)), (236, 214, 170), 1.2, contorno=False, sombra=False)
        s.traco(linha((36, y), (28, y + 4)), (236, 214, 170), 1.2, contorno=False, sombra=False)
    s.peca(rrect(16, 46, 32, 5, 1.5), (100, 62, 36), contorno=1.4, sombra=False)


def armadura_media(s):
    d = colete(s, (150, 156, 168), (98, 64, 40))
    aneis = []
    for y in range(18, 56, 5):
        for x in range(14 + (y // 5) % 2 * 2, 52, 5):
            aneis.append('<circle cx="%s" cy="%s" r="2" fill="none" stroke="#4a4f5c" stroke-width="0.9"/>' % (x, y))
    s.recorte(d, aneis)
    s.traco(d, (46, 48, 56), 2.4, contorno=False, sombra=False)
    s.peca(rrect(15, 44, 34, 6, 2), (120, 78, 46), contorno=1.6, sombra=False)
    s.peca(rrect(29, 43, 6, 8, 1.5), OURO, contorno=1.2, sombra=False)


def armadura_pesada(s):
    peito = "M18,18L46,18L48,40C46,50 38,56 32,57C26,56 18,50 16,40Z"
    s.peca(elipse(14, 22, 10, 8), ACO, contorno=2.2)
    s.peca(elipse(50, 22, 10, 8), ACO, contorno=2.2)
    s.peca(peito, (178, 186, 200), contorno=2.6)
    s.traco(linha((32, 20), (32, 55)), (120, 128, 142), 1.6, contorno=False, sombra=False)
    s.traco("M20,36C26,40 38,40 44,36", (120, 128, 142), 1.4, contorno=False, sombra=False)
    for x, y in ((21, 22), (43, 22), (22, 44), (42, 44), (14, 22), (50, 22)):
        s.chapado(circ(x, y, 1.6), OURO)
    s.brilho(24, 26, 3, 7, 0.6, 15)


def joia(s, cx, cy, r, cor):
    s.peca(poly([(cx, cy - r), (cx + r * 0.85, cy - r * 0.2), (cx, cy + r), (cx - r * 0.85, cy - r * 0.2)]), cor, contorno=1.6)
    s.brilho(cx - r * 0.25, cy - r * 0.35, r * 0.18, r * 0.36, 0.85, 25)


def brinco(s):
    s.traco("M32,8C22,8 22,20 30,22", OURO, 2.6)
    s.peca(circ(31, 24, 3.4), OURO, contorno=1.4)
    s.traco(linha((31, 27), (31, 33)), OURO, 2.0)
    joia(s, 31, 45, 12, (70, 206, 230))
    s.faisca(46, 36, 3.5, (200, 244, 255))


def amuleto(s):
    s.traco("M14,8C16,26 24,34 32,36C40,34 48,26 50,8", OURO, 2.2)
    for t in range(1, 8):
        a = t / 8.0
        x = 14 + 36 * a
        y = 8 + 28 * math.sin(math.pi * a)
        s.chapado(circ(x, y, 1.1), mul(OURO, 0.7))
    s.peca(circ(32, 44, 13), OURO, contorno=2.4)
    s.peca(circ(32, 44, 7.5), (210, 44, 60), contorno=1.8, sombra=False)
    s.brilho(29, 41, 2.2, 3.4, 0.85, 20)


def bracelete(s):
    s.peca(elipse(32, 36, 24, 15) + elipse(32, 36, 17, 9), OURO, contorno=2.4)
    for x, y, cor in ((14, 38, (220, 60, 70)), (32, 50, (70, 180, 240)), (50, 38, (80, 210, 110))):
        s.peca(circ(x, y, 3.8), cor, contorno=1.4, sombra=False)
        s.brilho(x - 1, y - 1.3, 0.9, 1.6, 0.8, 20)


def cinto(s):
    s.peca("M4,26C20,22 44,22 60,26L60,38C44,34 20,34 4,38Z", COURO, contorno=2.4)
    for x in (12, 50):
        s.chapado(circ(x, 32, 1.4), (236, 214, 170))
    s.peca(rrect(22, 20, 20, 24, 4) + rrect(27, 25, 10, 14, 2), OURO, contorno=2.2)
    s.traco(linha((32, 25), (32, 39)), (120, 124, 136), 2.2, contorno=False, sombra=False)
    s.brilho(25, 24, 1.4, 4, 0.8, 0)


# ─────────────────────────────── materiais coloridos ───────────────────────────────


def material_colorido(s, base, tier):
    c = TIER[tier]
    aura_do_tier(s, tier)
    if base == 300:  # Aco: lingote
        lingote(s, mix(ACO, c, 0.45))
    elif base == 304:  # Coracao Negro
        d = "M32,56C16,46 8,36 8,26C8,18 14,12 22,12C27,12 30,15 32,19C34,15 37,12 42,12C50,12 56,18 56,26C56,36 48,46 32,56Z"
        s.peca(d, mix((58, 46, 70), c, 0.2), contorno=2.6)
        s.traco(linha((32, 22), (28, 32), (35, 38), (31, 48)), c, 2.2, contorno=False, sombra=False)
        s.traco(linha((20, 24), (24, 30)), c, 1.6, contorno=False, sombra=False)
        s.brilho(20, 20, 3, 5, 0.45, 30)
    elif base == 308:  # Sombra-da-Lua: crescente
        d = circ(32, 32, 22) + circ(42, 26, 17)
        s.peca("M32,10A22,22 0 1,0 54,34A17,17 0 1,1 32,10Z", mix((112, 122, 158), c, 0.45), contorno=2.6)
        s.faisca(44, 18, 4.5, mix(BRANCO, c, 0.3))
        s.faisca(52, 44, 3, mix(BRANCO, c, 0.3))
        s.brilho(18, 26, 2.6, 7, 0.5, 10)
    elif base == 312:  # Quintessencia: orbe
        s.halo(32, 33, 28, c, 0.55)
        s.peca(circ(32, 33, 19), mix(c, BRANCO, 0.12), contorno=2.4, claro=0.6, escuro=0.45)
        s.traco("M20,38C24,26 40,24 44,32", mix(c, BRANCO, 0.6), 2.0, contorno=False, sombra=False, op=0.8)
        s.traco("M22,44C30,40 40,42 44,38", mix(c, BRANCO, 0.5), 1.4, contorno=False, sombra=False, op=0.6)
        s.brilho(25, 24, 4, 2.4, 0.9, -30)
    elif base == 316:  # Berloque de Exorcismo: talisma de papel
        s.traco("M32,6C28,10 28,14 32,16", (200, 60, 60), 1.8)
        s.peca(rrect(20, 14, 24, 44, 3), (238, 222, 180), contorno=2.2)
        s.peca(rrect(23, 17, 18, 38, 2), mix((232, 214, 168), c, 0.18), contorno=0.8, sombra=False)
        tinta = mix((190, 40, 40), c, 0.5)
        s.traco(linha((32, 21), (32, 50)), tinta, 2.0, contorno=False, sombra=False)
        for y in (27, 36, 45):
            s.traco(linha((26, y), (38, y - 3)), tinta, 1.8, contorno=False, sombra=False)
        s.chapado(circ(32, 32, 3.2), tinta)
    elif base == 320:  # Platina: pepitas
        cor = mix((226, 230, 238), c, 0.3)
        for cx, cy, r in ((22, 42, 11), (41, 43, 10), (32, 28, 10), (46, 26, 6.5)):
            pts = [(cx + r * math.cos(math.radians(a)) * (0.8 + 0.2 * ((a // 45) % 2)),
                    cy + r * 0.8 * math.sin(math.radians(a)) * (0.85 + 0.15 * ((a // 60) % 2))) for a in range(0, 360, 45)]
            s.peca(poly(pts), cor, contorno=2.0)
            s.brilho(cx - r * 0.3, cy - r * 0.3, r * 0.2, r * 0.1, 0.85)
    elif base == 324:  # Fragmento Iluminante: cristais
        s.halo(32, 34, 28, c, 0.5)
        for pts, k in (([(24, 58), (18, 26), (26, 6), (34, 26), (32, 58)], 0.25),
                       ([(14, 58), (8, 38), (14, 24), (22, 38), (22, 58)], 0.0),
                       ([(34, 58), (36, 34), (44, 20), (52, 38), (46, 58)], -0.1)):
            s.peca(poly(pts), mix(c, BRANCO, max(0.0, k)) if k >= 0 else mul(c, 0.92), contorno=2.0, claro=0.55, escuro=0.5)
        s.traco(linha((26, 8), (27, 50)), BRANCO, 1.2, contorno=False, sombra=False, op=0.7)
        s.faisca(50, 14, 4, mix(BRANCO, c, 0.2))
    elif base == 328:  # Pedra de Anima: runa
        s.peca(poly([(12, 18), (30, 8), (50, 14), (54, 40), (36, 56), (12, 46)]), (126, 132, 128), contorno=2.6)
        s.halo(32, 32, 16, c, 0.55)
        for d in (linha((32, 16), (32, 48)), linha((24, 20), (32, 30), (24, 40)), linha((40, 20), (32, 30), (40, 40))):
            s.traco(d, mix(c, BRANCO, 0.25), 2.4, contorno=False, sombra=False)
        s.brilho(20, 20, 3, 5, 0.35, 30)
    elif base == 332:  # Escama
        cor = mix((70, 150, 150), c, 0.5)
        d = "M32,58C18,48 10,36 10,24C10,14 20,8 32,8C44,8 54,14 54,24C54,36 46,48 32,58Z"
        s.peca(d, cor, contorno=2.6)
        s.traco(linha((32, 12), (32, 52)), mul(cor, 0.62), 1.6, contorno=False, sombra=False)
        s.traco("M14,26C22,32 42,32 50,26", mul(cor, 0.66), 1.4, contorno=False, sombra=False)
        s.traco("M18,40C24,44 40,44 46,40", mul(cor, 0.66), 1.2, contorno=False, sombra=False)
        s.brilho(22, 18, 5, 2.6, 0.7)
    elif base == 336:  # Garras
        for dx in (-13, 0, 13):
            d = "M%s,56C%s,40 %s,24 %s,10C%s,24 %s,40 %s,56Z" % (
                f(32 + dx - 5), f(32 + dx - 5), f(32 + dx + 2), f(32 + dx + 10),
                f(32 + dx + 6), f(32 + dx + 5), f(32 + dx + 4))
            s.peca(d, (232, 222, 196), contorno=2.0)
            s.recorte(d, ['<rect x="0" y="0" width="64" height="24" fill="%s"/>' % hx(c)])
            s.traco(d, (70, 62, 50), 2.0, contorno=False, sombra=False)
    elif base == 340:  # Chifre
        pts = []
        for i in range(15):
            t = i / 14.0
            a = math.pi * (1.05 - 1.25 * t)
            r = 22 - 12 * t
            pts.append((30 + r * math.cos(a), 34 - r * math.sin(a) + 10 * t))
        for i in range(14):
            w = 11.0 - 8.0 * i / 13.0
            cor = c if i % 4 == 2 else (224, 206, 172)
            s.traco(linha(pts[i], pts[i + 1]), cor, w, contorno=(i == 0), sombra=(i < 3))
        s.traco(linha(*pts), (90, 74, 56), 0.8, contorno=False, sombra=False, op=0.6)
    elif base == 64:  # Couro
        cor = mix(COURO, c, 0.35)
        d = "M20,8L44,8L50,18L58,20L52,32L58,46L46,48L40,58L24,58L18,48L6,46L12,32L6,20L14,18Z"
        s.peca(d, cor, contorno=2.4)
        s.traco("M18,18L46,18L50,32L46,46L18,46L14,32Z", (240, 220, 180), 1.2, contorno=False, sombra=False, op=0.8)
        s.brilho(24, 22, 5, 2.4, 0.4)


# ─────────────────────────────── catalogo ───────────────────────────────

MATERIAIS_COLORIDOS = [300, 304, 308, 312, 316, 320, 324, 328, 332, 336, 340, 64]


def catalogo():
    itens = {
        1: ouro,
        2: lambda s: frasco(s, VERMELHO, "redondo"),
        8: lambda s: frasco(s, AZUL, "redondo"),
        9: lambda s: (s.halo(32, 36, 30, VERMELHO, 0.35), frasco(s, VERMELHO, "grande", "mais")),
        10: lambda s: (s.halo(32, 36, 30, AZUL, 0.35), frasco(s, AZUL, "grande", "mais")),
        11: lambda s: frasco(s, VERDE, "alto"),
        350: lambda s: (s.halo(32, 36, 30, (255, 220, 90), 0.45), frasco(s, (250, 202, 56), "redondo", "estrela")),
        351: lambda s: frasco(s, (244, 140, 36), "quadrado", "moeda"),
        352: lambda s: frasco(s, (170, 110, 240), "coracao", "trevo"),
        # Dungeons: Marcas da Tempestade (moeda de raio) e Selo da Tempestade.
        357: lambda s: (s.halo(32, 34, 26, (120, 180, 255), 0.35), moeda(s, 32, 34, 20, (96, 150, 230))),
        358: lambda s: (s.halo(32, 32, 30, (170, 120, 255), 0.5), moeda(s, 32, 32, 22, (140, 96, 220)), s.faisca(44, 18, 6, (230, 210, 255))),
        60: lambda s: madeira(s, 1),
        61: lambda s: madeira(s, 2),
        62: lambda s: madeira(s, 3),
        63: lambda s: madeira(s, 4),
        96: lambda s: peixe(s, 0),
        97: lambda s: peixe(s, 1),
        98: lambda s: peixe(s, 2),
        99: lambda s: peixe(s, 3),
        100: lambda s: barco(s, True),
        101: lambda s: barco(s, False),
        344: cobre,
        345: darksteel,
        346: po_cintilante,
        400: espada_e_escudo,
        401: katana,
        402: pistolas,
        403: anel_magico,
        404: manto_guerreiro,
        405: bainha,
        406: coldre,
        407: manto_mago,
        408: armadura_leve,
        409: armadura_media,
        410: armadura_pesada,
        411: brinco,
        412: amuleto,
        413: bracelete,
        414: cinto,
    }
    for base in MATERIAIS_COLORIDOS:
        for tier in range(1, 5):
            itens[base + tier - 1] = (lambda b, tr: lambda s: material_colorido(s, b, tr))(base, tier)
    # Chaves lendarias (cor 5): ids proprios, 353..356 (`item_id::*_LENDARIA`).
    for k, base in enumerate([332, 336, 340, 64]):
        itens[353 + k] = (lambda b: lambda s: material_colorido(s, b, 5))(base)
    return dict(sorted(itens.items()))


# ─────────────────────────────── raster e atlas ───────────────────────────────


def rasteriza(svg, tmp):
    grande = LADO * 3
    caminho_svg = os.path.join(tmp, "i.svg")
    caminho_png = os.path.join(tmp, "i.png")
    with open(caminho_svg, "w") as fh:
        fh.write(svg)
    subprocess.run(["rsvg-convert", "-w", str(grande), "-h", str(grande), caminho_svg, "-o", caminho_png], check=True)
    img = Image.open(caminho_png).convert("RGBA")
    img.load()
    return img.convert("RGBa").resize((LADO, LADO), Image.LANCZOS).convert("RGBA")


def main():
    itens = catalogo()
    linhas = (len(itens) + COLUNAS - 1) // COLUNAS
    largura, altura = COLUNAS * LADO, linhas * LADO
    atlas = Image.new("RGBA", (largura, altura), (0, 0, 0, 0))
    indice = []
    with tempfile.TemporaryDirectory() as tmp:
        for k, (item_id, desenho) in enumerate(itens.items()):
            s = Svg()
            desenho(s)
            atlas.paste(rasteriza(s.texto(), tmp), ((k % COLUNAS) * LADO, (k // COLUNAS) * LADO))
            indice.append((item_id, k))
    atlas.save(os.path.join(RAIZ, "assets/icones/itens.png"), format="PNG", optimize=False)
    rs = [
        "// GERADO por tools/icones/gerar_icones.py — nao edite a mao.",
        "// Rode `python3 tools/icones/gerar_icones.py` pra refazer atlas e indice.",
        "",
        "/// Lado de uma celula do atlas, em pixels.",
        f"pub const LADO: u32 = {LADO};",
        "/// Celulas por linha do atlas.",
        f"pub const COLUNAS: u32 = {COLUNAS};",
        "/// Dimensoes do atlas, em pixels.",
        f"pub const LARGURA: u32 = {largura};",
        f"pub const ALTURA: u32 = {altura};",
        "/// (item_id, celula), ordenado por item_id.",
        "pub const ICONES: &[(u16, u16)] = &[",
    ]
    rs += [f"    ({i}, {k})," for i, k in indice]
    rs += ["];", ""]
    with open(os.path.join(RAIZ, "crates/client/src/icones_indice.rs"), "w") as fh:
        fh.write("\n".join(rs))
    print(f"{len(indice)} icones -> assets/icones/itens.png ({largura}x{altura})")


if __name__ == "__main__":
    main()
