#!/usr/bin/env python3
"""Icones dos itens: pixel art procedural, um icone UNICO por item.

Gera:
  assets/icones/itens.png            atlas RGBA, celulas de LADO x LADO
  crates/client/src/icones_indice.rs tabela item_id -> celula (ordenada por id)

Python puro (so' stdlib: zlib/struct). Deterministico: rodar de novo gera
os mesmos bytes — nada de aleatorio sem semente, e o PNG sai com o mesmo zlib.

    python3 tools/icones/gerar_icones.py

Estilo: silhueta por categoria, 4 tons por peca com a luz vindo de cima e da
esquerda, contorno escuro de 1 px tirado da propria cor, sombra curta embaixo
e a direita. A cor do TIER (cinza/verde/azul/roxo, a mesma de
`shared::items::tier_color_hex`) entra nos materiais coloridos e na madeira e
no couro. A moldura de raridade NAO e' assada: o cliente desenha em volta
(`icones::icone`), porque a raridade de uma peca e' da instancia e nao do id.
"""
import math
import os
import struct
import zlib

LADO = 48
COLUNAS = 10
RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TIER = {1: (191, 191, 191), 2: (95, 211, 95), 3: (85, 119, 255), 4: (170, 85, 255), 5: (255, 119, 51)}

ACO = (200, 206, 216)
ACO_ESC = (120, 128, 140)
OURO = (236, 190, 76)
OURO_ESC = (176, 124, 44)
COURO = (146, 94, 54)
COURO_ESC = (104, 64, 36)
MADEIRA = (134, 90, 52)
VIDRO = (206, 228, 236)
BRANCO = (250, 250, 250)
VERMELHO = (212, 48, 56)
AZUL = (62, 108, 232)
VERDE = (74, 192, 92)
ROXO = (150, 90, 222)
ESCURO = (40, 36, 48)


def mul(c, k):
    return tuple(max(0, min(255, int(round(v * k)))) for v in c[:3])


def mix(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


# ─────────────────────────────── formas ───────────────────────────────
# Toda forma e' um predicado f(x, y) no centro do pixel.

def poligono(pts):
    def f(px, py):
        dentro = False
        n = len(pts)
        for i in range(n):
            x1, y1 = pts[i]
            x2, y2 = pts[(i + 1) % n]
            if (y1 > py) != (y2 > py) and px < (x2 - x1) * (py - y1) / (y2 - y1) + x1:
                dentro = not dentro
        return dentro
    return f


def elipse(cx, cy, rx, ry=None):
    ry = rx if ry is None else ry
    return lambda x, y: ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1.0


def ret(x0, y0, x1, y1):
    return lambda x, y: x0 <= x <= x1 and y0 <= y <= y1


def capsula(x0, y0, x1, y1, w):
    dx, dy = x1 - x0, y1 - y0
    l2 = dx * dx + dy * dy or 1e-9
    r2 = (w / 2.0) ** 2

    def f(x, y):
        t = max(0.0, min(1.0, ((x - x0) * dx + (y - y0) * dy) / l2))
        px, py = x0 + dx * t, y0 + dy * t
        return (x - px) ** 2 + (y - py) ** 2 <= r2
    return f


def anel(cx, cy, rx, ry, esp):
    fora = elipse(cx, cy, rx, ry)
    dentro = elipse(cx, cy, max(0.5, rx - esp), max(0.5, ry - esp))
    return lambda x, y: fora(x, y) and not dentro(x, y)


def uniao(*fs):
    return lambda x, y: any(f(x, y) for f in fs)


def menos(a, b):
    return lambda x, y: a(x, y) and not b(x, y)


def e(a, b):
    return lambda x, y: a(x, y) and b(x, y)


def arco(pontos, w):
    return uniao(*[capsula(*pontos[i], *pontos[i + 1], w) for i in range(len(pontos) - 1)])


def rodar(pts, ox, oy, ang, espelho=False):
    c, s = math.cos(ang), math.sin(ang)
    out = []
    for x, y in pts:
        if espelho:
            y = -y
        out.append((ox + x * c - y * s, oy + x * s + y * c))
    return out


# ─────────────────────────────── tela ───────────────────────────────

class Tela:
    def __init__(self):
        self.px = [[(0, 0, 0, 0)] * LADO for _ in range(LADO)]

    def pontos(self, f):
        return [(x, y) for y in range(LADO) for x in range(LADO) if f(x + 0.5, y + 0.5)]

    def pinta(self, f, cor, luz=True):
        """Preenche com 4 tons: claro em cima/esquerda, escuro embaixo/direita."""
        pts = self.pontos(f)
        if not pts:
            return
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        w, h = max(1, x1 - x0), max(1, y1 - y0)
        for x, y in pts:
            if luz:
                u, v = (x - x0) / w, (y - y0) / h
                q = u * 0.42 + v * 0.58
                k = (1.22, 1.06, 0.90, 0.74)[min(3, int(q * 4))]
            else:
                k = 1.0
            self.px[y][x] = mul(cor, k) + (255,)

    def chapado(self, f, cor, alfa=255):
        for x, y in self.pontos(f):
            self.px[y][x] = tuple(cor[:3]) + (alfa,)

    def aura(self, cx, cy, r, cor, forca=150):
        """Brilho suave atras (so' onde ainda esta' vazio)."""
        for y in range(LADO):
            for x in range(LADO):
                d = math.hypot(x + 0.5 - cx, y + 0.5 - cy) / r
                if d < 1.0 and self.px[y][x][3] < 40:
                    a = int(forca * (1.0 - d) ** 1.6)
                    if a > self.px[y][x][3]:
                        self.px[y][x] = tuple(cor[:3]) + (a,)

    def ponto(self, x, y, cor):
        if 0 <= x < LADO and 0 <= y < LADO:
            self.px[y][x] = tuple(cor[:3]) + (255,)

    def brilho(self, x, y, tam=1, cor=BRANCO):
        self.ponto(x, y, cor)
        if tam > 0:
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                self.ponto(x + dx, y + dy, mix(cor, (255, 255, 200), 0.3))

    def acabamento(self):
        solido = lambda x, y: 0 <= x < LADO and 0 <= y < LADO and self.px[y][x][3] >= 200
        novos = []
        for y in range(LADO):
            for x in range(LADO):
                if solido(x, y):
                    continue
                viz = [self.px[y + dy][x + dx] for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)) if solido(x + dx, y + dy)]
                if viz:
                    m = tuple(sum(c[i] for c in viz) / len(viz) for i in range(3))
                    novos.append((x, y, mul(m, 0.26) + (255,)))
        for x, y, c in novos:
            self.px[y][x] = c
        sombra = []
        for y in range(LADO - 1):
            for x in range(LADO - 1):
                if solido(x, y) and not solido(x + 1, y + 1) and self.px[y + 1][x + 1][3] < 90:
                    sombra.append((x + 1, y + 1))
        for x, y in sombra:
            self.px[y][x] = (0, 0, 0, 90)


# ─────────────────────────────── desenhos ───────────────────────────────

def espada_e_escudo(t):
    t.pinta(poligono([(8, 9), (30, 9), (31, 24), (19, 41), (7, 24)]), (58, 88, 160))
    t.pinta(poligono([(11, 12), (27, 12), (28, 23), (19, 36), (10, 23)]), (86, 122, 196))
    t.pinta(elipse(19, 21, 3.2), OURO)
    t.pinta(capsula(43, 5, 22, 26, 4.2), ACO)
    t.chapado(capsula(41, 7, 24, 24, 1.0), mul(ACO, 0.7))
    t.pinta(capsula(17, 22, 26, 31, 3.2), OURO)
    t.pinta(capsula(21, 27, 14, 34, 3.4), COURO)
    t.pinta(elipse(13, 35, 2.4), OURO)


def katana(t):
    pts = []
    for i in range(13):
        s = i / 12.0
        x, y = 41 - 24 * s, 5 + 24 * s
        off = 3.2 * math.sin(math.pi * s)
        pts.append((x + off * 0.7, y + off * 0.7))
    t.pinta(arco(pts, 3.6), ACO)
    t.chapado(arco([(x - 1.0, y + 0.2) for x, y in pts[1:-1]], 0.9), BRANCO)
    t.pinta(elipse(16, 30, 3.6), (58, 58, 70))
    t.chapado(anel(16, 30, 3.6, 3.6, 1.0), OURO)
    t.pinta(capsula(15, 31, 7, 39, 3.8), (126, 32, 44))
    for k in range(3):
        t.ponto(13 - 2 * k, 33 + 2 * k, (236, 220, 190))
    t.pinta(elipse(6, 40, 1.8), OURO)


def pistola(t, ox, oy, ang, espelho):
    cano = rodar([(0, 0), (20, 0), (20, 4), (0, 4)], ox, oy, ang, espelho)
    corpo = rodar([(-1, -1), (8, -1), (9, 6), (-1, 6)], ox, oy, ang, espelho)
    cabo = rodar([(1, 5), (8, 5), (4, 15), (-2, 14)], ox, oy, ang, espelho)
    t.pinta(poligono(cabo), (112, 70, 40))
    t.pinta(poligono(corpo), MADEIRA)
    t.pinta(poligono(cano), ACO)
    gat = rodar([(6, 8)], ox, oy, ang, espelho)[0]
    t.chapado(anel(gat[0], gat[1], 2.2, 2.2, 0.9), OURO)


def pistolas(t):
    pistola(t, 7, 30, math.radians(-38), False)
    pistola(t, 41, 30, math.radians(218), True)


def anel_magico(t):
    t.aura(24, 19, 15, (120, 220, 255), 120)
    t.pinta(anel(24, 29, 12, 8, 3.2), OURO)
    t.pinta(ret(19, 19, 21, 24), OURO_ESC)
    t.pinta(ret(27, 19, 29, 24), OURO_ESC)
    t.pinta(elipse(24, 17, 6.2), (70, 196, 255))
    t.pinta(poligono([(24, 12), (28, 17), (24, 22), (20, 17)]), (150, 232, 255))
    t.brilho(21, 14, 0)
    t.brilho(36, 9, 1, (200, 245, 255))
    t.brilho(11, 13, 1, (200, 245, 255))


def manto_guerreiro(t):
    t.pinta(poligono([(16, 8), (32, 8), (41, 41), (31, 38), (24, 42), (17, 38), (7, 41)]), (186, 40, 46))
    t.chapado(capsula(20, 14, 15, 37, 1.4), (120, 24, 30))
    t.chapado(capsula(28, 14, 33, 37, 1.4), (120, 24, 30))
    t.pinta(ret(15, 6, 33, 11), OURO)
    t.pinta(elipse(24, 9, 3.2), OURO_ESC)
    t.pinta(elipse(24, 9, 1.8), VERMELHO)


def bainha(t):
    t.pinta(capsula(44, 4, 38, 10, 4.2), (54, 44, 44))
    t.pinta(capsula(11, 38, 38, 10, 6.4), (132, 30, 42))
    t.pinta(capsula(33, 15, 38, 10, 7.2), OURO)
    t.pinta(capsula(11, 38, 15, 34, 7.2), OURO)
    t.pinta(capsula(23, 26, 26, 23, 7.2), OURO_ESC)
    t.brilho(35, 11, 0)


def coldre(t):
    t.pinta(capsula(5, 9, 43, 9, 4.2), COURO_ESC)
    t.pinta(poligono([(22, 4), (29, 4), (30, 12), (21, 12)]), (112, 70, 40))
    t.pinta(poligono([(15, 12), (33, 12), (35, 30), (27, 43), (20, 41), (14, 28)]), COURO)
    t.pinta(ret(15, 10, 33, 17), COURO_ESC)
    t.pinta(ret(22, 13, 26, 17), OURO)
    for k in range(5):
        t.ponto(17 + k * 4, 37 - (k % 2), (220, 190, 150))


def manto_mago(t):
    t.pinta(poligono([(16, 14), (32, 14), (39, 42), (9, 42)]), (50, 70, 164))
    t.pinta(elipse(24, 12, 8.5, 7.5), (58, 80, 180))
    t.chapado(elipse(24, 13, 4.2, 4.5), (18, 18, 36))
    t.pinta(ret(9, 39, 39, 42), OURO)
    t.pinta(capsula(24, 20, 24, 39, 1.6), OURO_ESC)
    for x, y in ((15, 28), (32, 25), (20, 35), (30, 34)):
        t.brilho(x, y, 1, (255, 236, 150))


def armadura_leve(t):
    t.pinta(poligono([(13, 13), (20, 9), (28, 9), (35, 13), (37, 41), (11, 41)]), (156, 102, 60))
    t.chapado(poligono([(20, 9), (28, 9), (24, 19)]), (70, 46, 28))
    for k in range(4):
        y = 21 + k * 3
        t.chapado(capsula(22, y, 26, y + 2, 1.0), (236, 214, 170))
        t.chapado(capsula(26, y, 22, y + 2, 1.0), (236, 214, 170))
    t.pinta(ret(11, 34, 37, 37), COURO_ESC)
    t.pinta(ret(22, 33, 26, 38), OURO)


def armadura_media(t):
    corpo = poligono([(13, 13), (20, 9), (28, 9), (35, 13), (37, 41), (11, 41)])
    t.pinta(corpo, (150, 158, 172))
    for x, y in t.pontos(corpo):
        if (x + y) % 3 == 0:
            t.ponto(x, y, mul(t.px[y][x], 0.78))
    t.chapado(elipse(24, 11, 5, 3), (60, 60, 70))
    t.pinta(elipse(12, 15, 6.5, 4.5), COURO)
    t.pinta(elipse(36, 15, 6.5, 4.5), COURO)
    t.pinta(ret(11, 35, 37, 38), COURO_ESC)


def armadura_pesada(t):
    t.pinta(ret(18, 8, 30, 14), ACO_ESC)
    t.pinta(poligono([(14, 13), (34, 13), (33, 36), (24, 43), (15, 36)]), ACO)
    t.pinta(elipse(11, 16, 7.5, 6.2), mul(ACO, 0.92))
    t.pinta(elipse(37, 16, 7.5, 6.2), mul(ACO, 0.92))
    t.chapado(capsula(24, 15, 24, 40, 1.0), BRANCO)
    for x, y in ((17, 17), (31, 17), (17, 31), (31, 31)):
        t.ponto(x, y, OURO)


def brinco(t):
    for (hx, hy, gx, gy, cor) in ((15, 8, 15, 24, (70, 200, 240)), (33, 14, 33, 31, (236, 90, 170))):
        t.chapado(anel(hx, hy, 3.2, 3.2, 1.1), OURO)
        t.pinta(capsula(hx, hy + 3, gx, gy - 6, 1.4), OURO_ESC)
        t.pinta(uniao(elipse(gx, gy, 4.2, 5.2), poligono([(gx - 3.4, gy - 2), (gx + 3.4, gy - 2), (gx, gy - 9)])), cor)
        t.brilho(gx - 1, gy - 2, 0)


def amuleto(t):
    t.chapado(anel(24, 18, 13, 11, 1.6), OURO_ESC)
    t.pinta(poligono([(24, 22), (33, 32), (24, 44), (15, 32)]), OURO)
    t.pinta(poligono([(24, 25), (30, 32), (24, 40), (18, 32)]), (150, 70, 220))
    t.brilho(22, 29, 0)


def bracelete(t):
    t.pinta(anel(24, 27, 16, 10, 4.2), OURO)
    for k in range(7):
        a = math.pi * (0.15 + 0.7 * k / 6.0)
        t.ponto(int(24 - 14 * math.cos(a)), int(27 + 8 * math.sin(a)), OURO_ESC)
    t.pinta(elipse(24, 18, 4.2), VERMELHO)
    t.pinta(elipse(12, 22, 2.4), AZUL)
    t.pinta(elipse(36, 22, 2.4), AZUL)
    t.brilho(23, 16, 0)


def cinto(t):
    t.pinta(capsula(4, 27, 44, 27, 9), COURO)
    for x in (34, 38, 42):
        t.ponto(x, 27, COURO_ESC)
    t.pinta(menos(ret(17, 19, 31, 35), ret(20, 22, 28, 32)), OURO)
    t.pinta(capsula(22, 27, 31, 27, 1.8), ACO)


def frasco(t, liq, forma, marca=None):
    if forma == "redondo":
        t.pinta(ret(21, 11, 27, 22), VIDRO)
        t.pinta(elipse(24, 30, 10.5), VIDRO)
        t.pinta(elipse(24, 32, 8.4, 7.4), liq)
        t.pinta(ret(20, 7, 28, 12), COURO)
    elif forma == "grande":
        t.pinta(ret(20, 9, 28, 21), VIDRO)
        t.pinta(elipse(24, 31, 13.2, 12.2), VIDRO)
        t.pinta(elipse(24, 33, 11, 10), liq)
        t.pinta(capsula(19, 20, 29, 20, 2.4), OURO)
        t.pinta(ret(19, 5, 29, 10), OURO_ESC)
    elif forma == "alto":
        t.pinta(ret(20, 8, 28, 16), VIDRO)
        t.pinta(poligono([(15, 16), (33, 16), (34, 43), (14, 43)]), VIDRO)
        t.pinta(poligono([(17, 22), (31, 22), (32, 41), (16, 41)]), liq)
        t.pinta(ret(19, 4, 29, 9), COURO)
    elif forma == "quadrado":
        t.pinta(ret(20, 8, 28, 15), VIDRO)
        t.pinta(ret(12, 15, 36, 42), VIDRO)
        t.pinta(ret(14, 22, 34, 40), liq)
        t.pinta(ret(19, 4, 29, 9), COURO_ESC)
    elif forma == "coracao":
        t.pinta(ret(21, 8, 27, 16), VIDRO)
        t.pinta(uniao(elipse(18, 24, 8), elipse(30, 24, 8), poligono([(10, 26), (38, 26), (24, 43)])), VIDRO)
        t.pinta(uniao(elipse(18, 26, 6), elipse(30, 26, 6), poligono([(12, 28), (36, 28), (24, 40)])), liq)
        t.pinta(ret(20, 4, 28, 9), COURO)
    t.brilho(17, 26, 0)
    t.ponto(18, 27, BRANCO)
    if marca == "mais":
        t.chapado(ret(23, 28, 25, 38), BRANCO)
        t.chapado(ret(19, 32, 29, 34), BRANCO)
    elif marca == "estrela":
        t.pinta(poligono([(24, 25), (26, 30), (31, 31), (27, 34), (28, 39), (24, 36), (20, 39), (21, 34), (17, 31), (22, 30)]), (255, 248, 190))
    elif marca == "moeda":
        t.pinta(elipse(24, 31, 5.5), OURO)
        t.chapado(ret(23, 27, 25, 35), OURO_ESC)
    elif marca == "trevo":
        for cx, cy in ((24, 28), (20.5, 32), (27.5, 32)):
            t.pinta(elipse(cx, cy, 3), VERDE)
        t.chapado(capsula(24, 33, 25, 38, 1.2), mul(VERDE, 0.6))


def moedas(t, cor, borda):
    for cx, cy in ((16, 35), (32, 35), (24, 30), (18, 24), (30, 22)):
        t.pinta(elipse(cx, cy, 8, 5), cor)
        t.chapado(anel(cx, cy, 5.5, 3.2, 1.0), borda)
    t.brilho(28, 20, 1)


def ouro(t):
    moedas(t, OURO, OURO_ESC)


def cobre(t):
    t.pinta(poligono([(8, 30), (16, 22), (24, 26), (22, 38), (10, 38)]), (178, 102, 60))
    moedas(t, (206, 124, 72), (140, 76, 40))


def darksteel(t):
    t.aura(24, 28, 20, (170, 80, 255), 110)
    t.pinta(poligono([(8, 36), (40, 36), (34, 20), (14, 20)]), (64, 58, 82))
    t.pinta(poligono([(14, 20), (34, 20), (32, 24), (16, 24)]), (104, 94, 130))
    t.chapado(arco([(17, 28), (22, 25), (25, 30), (31, 27)], 1.2), (220, 130, 255))


def po_cintilante(t):
    t.pinta(poligono([(14, 14), (32, 14), (37, 36), (11, 36)]), (196, 160, 110))
    t.pinta(capsula(15, 15, 31, 15, 3), COURO_ESC)
    t.pinta(elipse(24, 38, 17, 5), (246, 196, 230))
    for x, y in ((16, 37), (30, 36), (24, 40), (36, 31), (9, 30)):
        t.brilho(x, y, 1, (255, 250, 255))


def madeira(t, tier):
    cor = TIER[tier]
    for y, x0, x1 in ((31, 7, 37), (20, 11, 35)):
        t.pinta(capsula(x0, y, x1, y, 10.5), MADEIRA)
        t.pinta(elipse(x1 + 1, y, 4.5, 5.2), (208, 164, 108))
        t.chapado(anel(x1 + 1, y, 2.6, 3.0, 0.9), (150, 104, 60))
    t.pinta(capsula(20, 13, 22, 38, 2.6), cor)


def couro(t, tier):
    base = mix((176, 124, 80), TIER[tier], 0.28)
    t.pinta(poligono([(10, 12), (18, 8), (30, 8), (38, 12), (41, 24), (36, 38), (28, 43), (20, 43), (12, 38), (7, 24)]), base)
    t.pinta(elipse(24, 25, 9, 11), mul(base, 1.12))
    for k in range(10):
        a = k / 10.0 * math.tau
        t.ponto(int(24 + 14 * math.cos(a)), int(25 + 15 * math.sin(a)), TIER[tier])


def peixe(t, especie):
    if especie == 0:
        t.pinta(poligono([(36, 24), (44, 17), (44, 31)]), (140, 160, 190))
        t.pinta(elipse(22, 24, 15, 5.5), (176, 196, 214))
        t.chapado(capsula(10, 23, 34, 23, 1.0), (110, 130, 170))
    elif especie == 1:
        t.pinta(poligono([(34, 24), (43, 16), (43, 32)]), (236, 110, 40))
        corpo = elipse(22, 24, 13, 8.5)
        t.pinta(corpo, (242, 124, 42))
        for x0 in (16, 26):
            t.chapado(e(corpo, ret(x0, 0, x0 + 3, 48)), BRANCO)
    elif especie == 2:
        t.pinta(poligono([(34, 24), (44, 15), (44, 33)]), (246, 206, 60))
        t.pinta(elipse(22, 24, 14, 9.5), (54, 92, 212))
        t.chapado(arco([(12, 20), (20, 17), (30, 21)], 1.6), ESCURO)
    else:
        for k in range(10):
            a = k / 10.0 * math.tau
            cx, cy = 23 + 13 * math.cos(a), 25 + 13 * math.sin(a)
            t.pinta(capsula(23 + 10 * math.cos(a), 25 + 10 * math.sin(a), cx, cy, 1.4), (200, 180, 100))
        t.pinta(elipse(23, 25, 11.5), (226, 206, 118))
        for x, y in ((19, 29), (27, 30), (23, 33)):
            t.ponto(x, y, (150, 120, 60))
    t.ponto(14, 22, ESCURO)


def barco(t, grande):
    if grande:
        t.pinta(capsula(23, 6, 23, 32, 1.8), (110, 76, 44))
        t.pinta(poligono([(24, 8), (40, 28), (24, 30)]), (240, 236, 220))
        t.pinta(poligono([(22, 12), (10, 28), (22, 29)]), (224, 218, 200))
        t.pinta(poligono([(24, 5), (31, 7), (24, 9)]), VERMELHO)
        t.pinta(poligono([(5, 32), (43, 32), (37, 42), (11, 42)]), (126, 82, 46))
    else:
        t.pinta(capsula(8, 22, 40, 40, 1.8), (170, 130, 80))
        t.pinta(poligono([(6, 28), (42, 28), (36, 38), (12, 38)]), (140, 92, 52))
        t.pinta(ret(10, 29, 38, 31), (176, 124, 72))


def material_colorido(t, base, tier):
    c = TIER[tier]
    if base == 300:  # Aco: lingote
        t.pinta(poligono([(8, 35), (40, 35), (34, 20), (14, 20)]), mix(ACO, c, 0.22))
        t.pinta(poligono([(14, 20), (34, 20), (32, 24), (16, 24)]), mix(BRANCO, c, 0.15))
        t.pinta(elipse(24, 29, 3.2), c)
    elif base == 304:  # Pedra do Coracao Negro
        t.aura(24, 28, 19, c, 90)
        t.pinta(uniao(elipse(18, 22, 8), elipse(30, 22, 8), poligono([(10, 24), (38, 24), (24, 42)])), (74, 22, 38))
        t.pinta(uniao(elipse(19, 24, 4), elipse(29, 24, 4), poligono([(15, 26), (33, 26), (24, 36)])), c)
        t.brilho(16, 19, 0)
    elif base == 308:  # Pedra Sombra-da-Lua
        t.aura(22, 26, 19, c, 90)
        t.pinta(menos(elipse(22, 26, 15), elipse(30, 20, 12)), (74, 80, 108))
        t.chapado(menos(elipse(22, 26, 15), elipse(22, 26, 13.5)), c)
        t.brilho(36, 34, 1, mix(BRANCO, c, 0.3))
    elif base == 312:  # Quintessencia: orbe
        t.aura(24, 24, 22, c, 140)
        t.pinta(elipse(24, 24, 13), mix(VIDRO, c, 0.2))
        t.pinta(elipse(24, 25, 9), c)
        t.chapado(arco([(19, 24), (22, 20), (27, 22), (27, 27), (22, 29)], 1.4), mix(BRANCO, c, 0.4))
        t.brilho(18, 17, 0)
    elif base == 316:  # Berloque de Exorcismo
        t.pinta(capsula(24, 3, 24, 13, 1.6), (214, 190, 150))
        t.pinta(ret(15, 12, 33, 38), (238, 224, 184))
        t.chapado(capsula(24, 16, 24, 33, 1.6), c)
        t.chapado(capsula(19, 22, 29, 22, 1.6), c)
        t.chapado(anel(24, 28, 4, 4, 1.2), c)
        t.pinta(poligono([(20, 38), (28, 38), (26, 45), (22, 45)]), c)
    elif base == 320:  # Platina: pepitas
        for cx, cy, r in ((17, 30, 8), (30, 32, 7), (24, 20, 7), (33, 21, 5)):
            t.pinta(elipse(cx, cy, r, r * 0.8), mix((226, 230, 238), c, 0.18))
        t.brilho(21, 17, 1, mix(BRANCO, c, 0.25))
        t.brilho(13, 27, 0)
    elif base == 324:  # Fragmento Iluminante: cristais
        t.aura(24, 26, 21, c, 130)
        t.pinta(poligono([(20, 42), (16, 18), (22, 4), (28, 18), (26, 42)]), mix(c, BRANCO, 0.25))
        t.pinta(poligono([(12, 42), (8, 26), (13, 16), (18, 28), (18, 42)]), c)
        t.pinta(poligono([(30, 42), (31, 24), (37, 14), (41, 28), (37, 42)]), mul(c, 0.9))
        t.brilho(21, 12, 0)
    elif base == 328:  # Pedra de Anima: runa
        t.pinta(poligono([(12, 14), (24, 7), (36, 13), (39, 34), (26, 42), (10, 35)]), (122, 128, 122))
        t.chapado(arco([(19, 16), (24, 24), (19, 32)], 2.0), c)
        t.chapado(arco([(29, 16), (24, 24), (29, 32)], 2.0), c)
        t.chapado(capsula(24, 13, 24, 36, 2.0), mix(c, BRANCO, 0.3))
    elif base == 332:  # Escama
        t.pinta(uniao(elipse(24, 20, 14, 13), poligono([(11, 22), (37, 22), (24, 44)])), mix((70, 150, 150), c, 0.5))
        t.chapado(capsula(24, 12, 24, 38, 1.4), mul(mix((70, 150, 150), c, 0.5), 0.6))
        t.chapado(arco([(14, 22), (24, 30), (34, 22)], 1.2), mul(mix((70, 150, 150), c, 0.5), 0.7))
        t.brilho(18, 14, 0)
    elif base == 336:  # Garra
        for dx in (-9, 0, 9):
            pts = [(24 + dx - 2, 42), (24 + dx, 30), (24 + dx + 4, 18), (24 + dx + 10, 10)]
            t.pinta(arco(pts[:3], 5.0 - abs(dx) * 0.12), (232, 222, 196))
            t.pinta(arco(pts[2:], 3.0), c)
    elif base == 340:  # Chifre
        pts = []
        for i in range(14):
            s = i / 13.0
            a = math.pi * (1.1 - 1.2 * s)
            r = 16 - 9 * s
            pts.append((26 + r * math.cos(a), 28 - r * math.sin(a) + 8 * s))
        for i in range(13):
            w = 9.0 - 6.5 * i / 12.0
            cor = c if i % 3 == 1 else (216, 200, 170)
            t.pinta(capsula(*pts[i], *pts[i + 1], w), cor)
    elif base == 64:  # Couro (HIDE)
        couro(t, tier)


# ─────────────────────────────── catalogo ───────────────────────────────

MATERIAIS_COLORIDOS = [300, 304, 308, 312, 316, 320, 324, 328, 332, 336, 340, 64]


def catalogo():
    itens = {
        1: ouro,
        2: lambda t: frasco(t, VERMELHO, "redondo"),
        8: lambda t: frasco(t, AZUL, "redondo"),
        9: lambda t: frasco(t, VERMELHO, "grande", "mais"),
        10: lambda t: frasco(t, AZUL, "grande", "mais"),
        11: lambda t: frasco(t, VERDE, "alto"),
        350: lambda t: (t.aura(24, 30, 20, (255, 220, 90), 110), frasco(t, (250, 202, 56), "redondo", "estrela")),
        351: lambda t: frasco(t, (244, 140, 36), "quadrado", "moeda"),
        352: lambda t: frasco(t, (170, 110, 240), "coracao", "trevo"),
        60: lambda t: madeira(t, 1),
        61: lambda t: madeira(t, 2),
        62: lambda t: madeira(t, 3),
        63: lambda t: madeira(t, 4),
        96: lambda t: peixe(t, 0),
        97: lambda t: peixe(t, 1),
        98: lambda t: peixe(t, 2),
        99: lambda t: peixe(t, 3),
        100: lambda t: barco(t, True),
        101: lambda t: barco(t, False),
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
            itens[base + tier - 1] = (lambda b, tr: lambda t: material_colorido(t, b, tr))(base, tier)
    return dict(sorted(itens.items()))


def png(caminho, largura, altura, linhas):
    cru = b"".join(b"\x00" + bytes(l) for l in linhas)

    def pedaco(tipo, dados):
        return struct.pack(">I", len(dados)) + tipo + dados + struct.pack(">I", zlib.crc32(tipo + dados) & 0xFFFFFFFF)

    dados = (b"\x89PNG\r\n\x1a\n"
             + pedaco(b"IHDR", struct.pack(">IIBBBBB", largura, altura, 8, 6, 0, 0, 0))
             + pedaco(b"IDAT", zlib.compress(cru, 9))
             + pedaco(b"IEND", b""))
    os.makedirs(os.path.dirname(caminho), exist_ok=True)
    with open(caminho, "wb") as f:
        f.write(dados)


def main():
    itens = catalogo()
    linhas_de_celulas = (len(itens) + COLUNAS - 1) // COLUNAS
    largura, altura = COLUNAS * LADO, linhas_de_celulas * LADO
    atlas = [[0, 0, 0, 0] * largura for _ in range(altura)]
    indice = []
    for k, (item_id, desenho) in enumerate(itens.items()):
        t = Tela()
        desenho(t)
        t.acabamento()
        cx, cy = (k % COLUNAS) * LADO, (k // COLUNAS) * LADO
        for y in range(LADO):
            linha = atlas[cy + y]
            for x in range(LADO):
                linha[(cx + x) * 4:(cx + x) * 4 + 4] = list(t.px[y][x])
        indice.append((item_id, k))
    png(os.path.join(RAIZ, "assets/icones/itens.png"), largura, altura, atlas)
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
    with open(os.path.join(RAIZ, "crates/client/src/icones_indice.rs"), "w") as f:
        f.write("\n".join(rs))
    print(f"{len(indice)} icones -> assets/icones/itens.png ({largura}x{altura})")


if __name__ == "__main__":
    main()
