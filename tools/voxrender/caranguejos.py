#!/usr/bin/env python3
"""Os CARANGUEJOS em pecas, pra andar DE LADO na praia.

Nao ha' caranguejo no zone14, entao o voxel sai daqui, por codigo. As pecas
tem os MESMOS nomes dos bichos de quatro patas (`bicho::junta_de`), com outro
papel no corpo:

  * tronco   — casco, barriga, boca e os olhos em haste;
  * pata_fd / pata_fe — as PINCAS (direita / esquerda): e' a "pata da frente"
    que o golpe usa, entao a pincada sai do mesmo arco da patada;
  * pata_td / pata_te — as tres pernas de cada lado, num grupo so'.

O cliente sabe que e' caranguejo pelo nome do arquivo (`Anatomia::lateral`):
anda com o corpo girado 90 graus e o pe' plantado corre no X do corpo.

O bicho olha pro +Y do voxel; Z e' pra cima. Mob comum e' leve
(docs/PIPELINE_ARTE.md): o teto de faces e' o dos outros bichos comuns.

Uso:  python3 tools/voxrender/caranguejos.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo  # noqa: E402

# indices da paleta
TOPO, LADO, BARRIGA, MANCHA, PERNA, PINCA, PONTA, HASTE, OLHO, ESPINHO = range(1, 11)

PALETAS = {
    # laranja-avermelhado de praia
    "caranguejo": {
        TOPO: (222, 88, 48), LADO: (186, 64, 36), BARRIGA: (240, 196, 150),
        MANCHA: (150, 48, 30), PERNA: (206, 80, 44), PINCA: (232, 104, 56),
        PONTA: (90, 30, 20), HASTE: (236, 170, 120), OLHO: (18, 18, 22),
        ESPINHO: (250, 230, 210),
    },
    # casco verde-agua com pintas de coral e a pinca grande laranja
    "caranguejo_rei": {
        TOPO: (64, 150, 168), LADO: (44, 112, 130), BARRIGA: (230, 210, 180),
        MANCHA: (236, 120, 140), PERNA: (58, 132, 150), PINCA: (240, 134, 92),
        PONTA: (80, 40, 34), HASTE: (200, 230, 220), OLHO: (18, 18, 22),
        ESPINHO: (236, 120, 140),
    },
}

# (arquivo, escala, pinca direita maior, espinhos no casco)
CARANGUEJOS = [
    ("caranguejo", 1.0, 1.0, False),
    ("caranguejo_rei", 1.45, 1.4, True),
]

TETO_DE_FACES = 2600


def r(v):
    return int(round(v))


def caranguejo(s, pinca_grande, espinhos):
    W, D, H = r(30 * s), r(26 * s), r(10 * s) + 2
    cx, cy = W // 2, r(11 * s)
    rx, ry, alt = 8 * s, 6 * s, 3 * s
    tronco, fd, fe, td, te = {}, {}, {}, {}, {}

    def topo_em(x, y):
        e = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
        return None if e > 1 else (2 + r(alt * (1 - e) ** 0.5), e)

    # casco: cupula achatada, barriga clara embaixo, pintas no topo
    for x in range(W):
        for y in range(D):
            t = topo_em(x, y)
            if t is None:
                continue
            topo, e = t
            for z in range(2, topo + 1):
                if z == 2:
                    c = BARRIGA
                elif z == topo:
                    c = MANCHA if (x * 7 + y * 3) % 11 == 0 else TOPO
                else:
                    c = LADO
                tronco[(x, y, z)] = c
            if espinhos and 0.62 < e < 0.9 and (x + y) % 4 == 0:
                tronco[(x, y, topo + 1)] = ESPINHO

    # boca na frente do casco, olhos em haste
    for x in range(cx - 1, cx + 2):
        tronco[(x, cy + r(ry) - 1, 3)] = PONTA
    for dx in (-3, 3):
        ex, ey = cx + r(dx * s), cy + r(4 * s)
        base = topo_em(ex, ey)[0]
        for z in range(base + 1, base + 1 + max(2, r(2 * s))):
            tronco[(ex, ey, z)] = HASTE
        tronco[(ex, ey, base + 1 + max(2, r(2 * s)))] = OLHO

    # tres pernas de cada lado: sai reta da borda do casco e desce ate' a ponta
    for dy in (-3, 0, 3):
        y = cy + r(dy * s)
        for lado, alvo in ((-1, te), (1, td)):
            x0 = cx + lado * (int(rx) + 1)
            n = r(4 * s)
            for k in range(n):
                alvo[(x0 + lado * k, y, 3)] = PERNA
            xp = x0 + lado * n
            alvo[(xp, y, 2)] = PERNA
            alvo[(xp, y, 1)] = PERNA
            alvo[(xp + lado, y, 0)] = PONTA

    # pincas: braco pra frente e pra fora, depois a garra com a boca aberta
    for lado, alvo, g in ((1, fd, pinca_grande), (-1, fe, 1.0)):
        k = s * g
        bx, by = cx + lado * r(5 * s), cy + r(4 * s)
        for t in range(r(3 * k)):
            alvo[(bx + lado * t, by + t, 3)] = PERNA
        px, py = bx + lado * r(3 * k), by + r(3 * k)
        comp, larg = max(3, r(4 * k)), max(2, r(2 * k))
        xs = range(min(px, px + lado * (larg - 1)), max(px, px + lado * (larg - 1)) + 1)
        for y in range(py, py + comp):
            ponta = y == py + comp - 1
            for x in xs:
                for z in (2, 3):
                    alvo[(x, y, z)] = PONTA if ponta else PINCA
                if y < py + comp // 2:
                    alvo[(x, y, 4)] = PINCA
                if y > py and not ponta:
                    alvo[(x, y, 5)] = PINCA
                elif ponta:
                    alvo[(x, y, 5)] = PONTA

    pecas = [("tronco", tronco), ("pata_fd", fd), ("pata_fe", fe), ("pata_td", td), ("pata_te", te)]
    for nome, v in pecas:
        for (x, y, z) in v:
            assert 0 <= x < W and 0 <= y < D and 0 <= z < H, f"{nome}: ({x},{y},{z}) fora da tela {W}x{D}x{H}"
    return (W, D, H), pecas


def faces(v):
    s = set(v)
    return sum(1 for (x, y, z) in s for d in ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))
               if (x + d[0], y + d[1], z + d[2]) not in s)


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "bichos")
    os.makedirs(pasta, exist_ok=True)
    for nome, s, pinca, espinhos in CARANGUEJOS:
        (W, D, H), pecas = caranguejo(s, pinca, espinhos)
        total = sum(faces(v) for _, v in pecas)
        assert total <= TETO_DE_FACES, f"{nome}: {total} faces, acima de {TETO_DE_FACES}"
        molde_corpo.W, molde_corpo.D, molde_corpo.H = W, D, H
        molde_corpo.TRANSL = f"0 0 {H // 2}"
        with open(os.path.join(pasta, f"{nome}.vox"), "wb") as f:
            f.write(molde_corpo.arquivo_cena(pecas, PALETAS[nome], camada=nome))
        print(f"{nome}.vox: {total} faces, tela {W}x{D}x{H}, " + ", ".join(f"{n}={len(v)}" for n, v in pecas))
