#!/usr/bin/env python3
"""Bakes Kōgen-tō's map from an OpenStreetMap extract of Shinjuku + Shibuya.

Map data © OpenStreetMap contributors, available under the ODbL
(https://www.openstreetmap.org/copyright). The game credits it.

    python rasterizar.py <osm.json> <assets/kogen_mapa.bin> [previa.png]

`osm.json` comes from `query.overpass` (Overpass API, `out geom tags`).

One CELL is one terrain column (BLOCO = 0.5 units). The projection and the
constants below are mirrored in `crates/shared/src/kogen.rs` — change both.
"""
import json
import math
import struct
import sys
import zlib

import numpy as np
from PIL import Image, ImageDraw

# ── projection (mirror of kogen.rs) ──
METROS_POR_UNIDADE = 4.0        # 4 m of Tokyo per game unit
BLOCO = 0.5                     # units per cell
METROS_POR_CELULA = METROS_POR_UNIDADE * BLOCO
BLOCOS_POR_METRO = 0.5          # a 200 m tower stands 100 blocks tall
LAT0, LON0 = 35.6754, 139.7000  # the cell (0, 0)
M_POR_GRAU_LAT = 110_540.0
M_POR_GRAU_LON = 111_320.0 * math.cos(math.radians(LAT0))

# ── the baked window, in cells round (0, 0) ──
X0, Z0, W, H = -660, -1300, 1320, 2600

# ── the island (mirror of kogen.rs) ──
COSTA_A, COSTA_B, COSTA_P = 585.0, 1270.0, 3.2  # superellipse half-axes, in cells
ORLA = 20                                       # cells of shore slope
NIVEL_CHAO = 20
DOCAS_DE = 1030                                 # the Docks: rows south of this
AVENIDA_DO_CAIS = 14                            # the waterfront avenue between them

# ── cell kinds (mirror of `kogen::Tipo`) ──
(MAR, ORLA_T, RUA, AVENIDA, FAIXA, ZEBRA, CALCADA, PRACA, PARQUE, BOSQUE, TRILHO, PREDIO, DOCAS) = range(13)
# ── facade styles (mirror of `kogen::Estilo` order) ──
CONCRETO, VIDRO, TIJOLO, BRANCO, LETREIROS = range(5)

# Districts: 0 Docks, 1 Shibuya, 2 Shrine Forest, 3 Kabukicho, 4 Nishi-Shinjuku, 5 Tocho.
TOCHO = (35.68955, 139.69175)
CRUZAMENTO = (35.65950, 139.70050)  # the Shibuya scramble


def para_celula(lat, lon):
    return (lon - LON0) * M_POR_GRAU_LON / METROS_POR_CELULA - X0, -(lat - LAT0) * M_POR_GRAU_LAT / METROS_POR_CELULA - Z0


def de_celula(c, r):
    x, z = c + X0, r + Z0
    return LAT0 - z * METROS_POR_CELULA / M_POR_GRAU_LAT, LON0 + x * METROS_POR_CELULA / M_POR_GRAU_LON


def anel(geom):
    return [para_celula(p["lat"], p["lon"]) for p in geom]


def aneis_externos(el):
    if el["type"] == "way":
        return [anel(el["geometry"])] if "geometry" in el else []
    return [anel(m["geometry"]) for m in el.get("members", []) if m.get("role") == "outer" and "geometry" in m]


def altura_m(tags):
    for chave in ("height", "building:height"):
        if chave in tags:
            try:
                return float(tags[chave].split()[0].replace("m", ""))
            except ValueError:
                pass
    if "building:levels" in tags:
        try:
            return float(tags["building:levels"].split(";")[0]) * 3.4 + 2.0
        except ValueError:
            pass
    return None


def hash01(i):
    i = (i * 2654435761) & 0xFFFFFFFF
    i ^= i >> 15
    return (i * 2246822519 & 0xFFFFFFFF) / 0xFFFFFFFF


LARGURA_M = {
    "motorway": 18, "trunk": 24, "primary": 22, "secondary": 16, "tertiary": 12,
    "motorway_link": 9, "trunk_link": 9, "primary_link": 9, "secondary_link": 8, "tertiary_link": 8,
    "residential": 6, "unclassified": 6, "living_street": 5, "service": 4,
    "pedestrian": 6, "footway": 3, "steps": 3, "path": 2.5,
}


def px(m):
    return max(1, round(m / METROS_POR_CELULA))


def tracejado(pts, ligado=3.0, desligado=3.0):
    """Splits a polyline (cells) into dashes of `ligado` m every `ligado+desligado` m."""
    passo_on, passo = ligado / METROS_POR_CELULA, (ligado + desligado) / METROS_POR_CELULA
    s, tracos = 0.0, []
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        L = math.hypot(x1 - x0, y1 - y0)
        t = 0.0
        while t < L:
            fase = (s + t) % passo
            if fase < passo_on:
                fim = min(L, t + passo_on - fase)
                a, b = t / L, fim / L
                tracos.append([(x0 + (x1 - x0) * a, y0 + (y1 - y0) * a), (x0 + (x1 - x0) * b, y0 + (y1 - y0) * b)])
                t = fim
            else:
                t += passo - fase
        s += L
    return tracos


def distrito(lat, lon, linha):
    if linha + Z0 >= DOCAS_DE:
        return 0
    if lat < 35.6650:
        return 1
    if lat < 35.6800:
        return 2
    dy = (lat - TOCHO[0]) * M_POR_GRAU_LAT
    dx = (lon - TOCHO[1]) * M_POR_GRAU_LON
    if math.hypot(dx, dy) < 300:
        return 5
    return 4 if lon < 139.6990 else 3


def estilo(d, blocos, tags, ident, perto_do_cruzamento):
    r = hash01(ident + 7)
    if perto_do_cruzamento or (d == 3 and r < 0.5) or (d == 1 and r < 0.3):
        return LETREIROS
    if blocos >= 60:
        return VIDRO if r < 0.75 else BRANCO
    if d == 1:
        return TIJOLO if r < 0.6 else BRANCO
    if d == 2:
        return BRANCO if r < 0.5 else CONCRETO
    if d == 3:
        return VIDRO if r < 0.75 else CONCRETO
    if d == 4:
        return VIDRO if r < 0.5 else (BRANCO if r < 0.8 else CONCRETO)
    if d == 5:
        return VIDRO if r < 0.6 else BRANCO
    return CONCRETO


def main():
    osm = json.load(open(sys.argv[1]))
    saida = sys.argv[2]
    els = osm["elements"]

    tipo = Image.new("L", (W, H), PRACA)
    alt = Image.new("I", (W, H), 0)
    est = Image.new("L", (W, H), 0)
    dt, da, de = ImageDraw.Draw(tipo), ImageDraw.Draw(alt), ImageDraw.Draw(est)

    # 1. Ground cover.
    for el in els:
        t = el.get("tags", {})
        k = None
        if t.get("natural") == "wood" or t.get("landuse") in ("forest", "religious"):
            k = BOSQUE
        elif t.get("leisure") in ("park", "garden") or t.get("landuse") == "grass" or t.get("natural") == "water":
            k = PARQUE
        elif t.get("landuse") == "railway":
            k = TRILHO
        if k is not None:
            for a in aneis_externos(el):
                if len(a) >= 3:
                    dt.polygon(a, fill=k)

    # 2. Streets (sidewalk band first), narrow to wide; then rails.
    vias = [el for el in els if el.get("tags", {}).get("highway") in LARGURA_M and "geometry" in el]
    vias.sort(key=lambda el: LARGURA_M[el["tags"]["highway"]])
    faixas = []
    for el in vias:
        t = el["tags"]
        if t.get("tunnel") in ("yes", "building_passage") or t.get("layer", "0").startswith("-") or t.get("indoor") == "yes":
            continue
        h = t["highway"]
        if h.startswith("motorway") and t.get("bridge") == "yes":
            continue  # elevated: on the deck layer
        pts = anel(el["geometry"])
        w = px(LARGURA_M[h])
        if h in ("footway", "steps", "path", "pedestrian"):
            dt.line(pts, fill=CALCADA, width=w, joint="curve")
            continue
        larga = h.split("_")[0] in ("motorway", "trunk", "primary", "secondary")
        dt.line(pts, fill=CALCADA, width=w + 2 * px(2.5), joint="curve")
        dt.line(pts, fill=AVENIDA if larga else RUA, width=w, joint="curve")
        if larga and not h.endswith("link"):
            faixas.append(pts)
    for pts in faixas:
        for traco in tracejado(pts):
            dt.line(traco, fill=FAIXA, width=1)
    for el in els:
        t = el.get("tags", {})
        if t.get("railway") in ("rail", "light_rail") and t.get("tunnel") != "yes" and "geometry" in el \
                and not t.get("layer", "0").startswith("-"):
            dt.line(anel(el["geometry"]), fill=TRILHO, width=px(6))

    # 3. Buildings, low to tall.
    predios = []
    for el in els:
        t = el.get("tags", {})
        if "building" not in t or t.get("building") in ("roof", "no", "construction") or t.get("layer", "0").startswith("-"):
            continue
        h = altura_m(t)
        if h is None:
            r = hash01(el["id"])
            h = {"house": 7, "detached": 7, "apartments": 14 + 20 * r, "commercial": 18 + 30 * r,
                 "office": 25 + 40 * r, "retail": 12 + 14 * r}.get(t["building"], 9 + 16 * r)
        predios.append((h, el))
    predios.sort(key=lambda x: x[0])
    cx, cz = para_celula(*CRUZAMENTO)
    for h, el in predios:
        blocos = max(2, min(250, round(h * BLOCOS_POR_METRO)))
        for a in aneis_externos(el):
            if len(a) < 3:
                continue
            mx = sum(p[0] for p in a) / len(a)
            mz = sum(p[1] for p in a) / len(a)
            lat, lon = de_celula(mx, mz)
            d = distrito(lat, lon, mz)
            perto = math.hypot(mx - cx, mz - cz) < 70
            dt.polygon(a, fill=PREDIO)
            da.polygon(a, fill=blocos)
            de.polygon(a, fill=estilo(d, blocos, el["tags"], el["id"], perto))

    # 4. The elevated expressways (Shuto): the deck layer, height in blocks above the street.
    deck = Image.new("I", (W, H), 0)
    dd = ImageDraw.Draw(deck)
    pontes = [el for el in vias if el["tags"]["highway"].startswith("motorway") and el["tags"].get("bridge") == "yes"]
    nivel_de = lambda t: 14 + 6 * max(0, int(t.get("layer", "1")) - 1)
    for el in pontes:
        t = el["tags"]
        dd.line(anel(el["geometry"]), fill=nivel_de(t), width=px(LARGURA_M[t["highway"]]), joint="curve")
    # RAMPS: an elevated way's end that joins no other elevated way is where
    # it comes down to the street. The last stretch slopes down to one
    # block (`VAO_LIVRE` and the step rule in terreno.rs do the rest).
    chave = lambda p: (round(p["lat"], 7), round(p["lon"], 7))
    pontas = {}
    for el in pontes:
        g = el["geometry"]
        for p in (g[0], g[-1]):
            pontas[chave(p)] = pontas.get(chave(p), 0) + 1
    M_POR_BLOCO_DE_RAMPA = 6.0
    rampas = 0
    for el in pontes:
        t = el["tags"]
        g = el["geometry"]
        nivel = nivel_de(t)
        w = px(LARGURA_M[t["highway"]])
        for pts in (g, g[::-1]):
            if pontas[chave(pts[0])] != 1:
                continue
            rampas += 1
            cel = anel(pts)
            comprimento = nivel * M_POR_BLOCO_DE_RAMPA / METROS_POR_CELULA  # in cells
            s = 0.0
            for (x0, y0), (x1, y1) in zip(cel, cel[1:]):
                L = math.hypot(x1 - x0, y1 - y0)
                k = max(1, int(L))
                for i in range(k):
                    a, b = i / k, (i + 1) / k
                    meio = s + L * (a + b) / 2
                    if meio > comprimento:
                        break
                    h = max(1, round(nivel * meio / comprimento))
                    pa = (x0 + (x1 - x0) * a, y0 + (y1 - y0) * a)
                    pb = (x0 + (x1 - x0) * b, y0 + (y1 - y0) * b)
                    dd.line([pa, pb], fill=h, width=w)
                    # Round caps: piece by piece, a bend would leave a notch.
                    r = w / 2
                    dd.ellipse([pb[0] - r, pb[1] - r, pb[0] + r, pb[1] + r], fill=h)
                s += L
                if s > comprimento:
                    break

    T = np.array(tipo, dtype=np.uint8)
    A = np.array(alt, dtype=np.int32).clip(0, 255).astype(np.uint8)
    E = np.array(est, dtype=np.uint8)
    D = np.array(deck, dtype=np.int32).clip(0, 255).astype(np.uint8)
    A[T != PREDIO] = 0
    E[T != PREDIO] = 0

    # 5. The scramble crossing: diagonal zebra over the road round it.
    rr, cc = np.mgrid[0:H, 0:W]
    perto = np.hypot(cc - cx, rr - cz) < 22
    zebra = ((cc + rr) % 5 < 2) | ((cc - rr) % 5 < 2)
    T[perto & np.isin(T, (RUA, AVENIDA, FAIXA)) & zebra] = ZEBRA

    # 6. The Docks and the waterfront avenue.
    z = rr + Z0
    T[(z >= DOCAS_DE - AVENIDA_DO_CAIS) & (z < DOCAS_DE)] = AVENIDA
    T[(z >= DOCAS_DE - AVENIDA_DO_CAIS - 3) & (z < DOCAS_DE - AVENIDA_DO_CAIS)] = CALCADA
    T[z >= DOCAS_DE] = DOCAS
    D[z >= DOCAS_DE - AVENIDA_DO_CAIS - 3] = 0

    # 7. The coast: a superellipse with a ragged edge; a sloping shore band.
    x = cc + X0
    ang = np.arctan2(z, x)
    # Bays and capes: low harmonics for the big shape, high ones for the rag.
    ruido = (1.0 + 0.045 * np.sin(ang * 5 + 0.3) + 0.035 * np.sin(ang * 9 - 1.0) + 0.02 * np.sin(ang * 17 + 2.0)
             + 0.012 * np.sin(ang * 37))
    s = ((np.abs(x) / COSTA_A) ** COSTA_P + (np.abs(z) / COSTA_B) ** COSTA_P) ** (1 / COSTA_P) / ruido
    r = np.hypot(x, z)
    ate_a_costa = r * (1 / np.maximum(s, 1e-6) - 1)  # cells to the shore, along the ray
    mar = s >= 1
    orla = ~mar & (ate_a_costa < ORLA)
    T[mar], A[mar], E[mar], D[mar] = MAR, 0, 0, 0
    T[orla] = ORLA_T
    A[orla] = (1 + (NIVEL_CHAO - 1) * (ate_a_costa[orla] / ORLA)).round().clip(1, NIVEL_CHAO).astype(np.uint8)
    E[orla] = 0
    D[orla] = 0

    # 8. Districts.
    lat = LAT0 - z * METROS_POR_CELULA / M_POR_GRAU_LAT
    lon = LON0 + x * METROS_POR_CELULA / M_POR_GRAU_LON
    dist = np.where(lat < 35.6650, 1, np.where(lat < 35.6800, 2, np.where(lon < 139.6990, 4, 3))).astype(np.uint8)
    dx = (lon - TOCHO[1]) * M_POR_GRAU_LON
    dy = (lat - TOCHO[0]) * M_POR_GRAU_LAT
    dist[(lat >= 35.6800) & (np.hypot(dx, dy) < 300)] = 5
    dist[z >= DOCAS_DE - AVENIDA_DO_CAIS - 3] = 0

    # Format: b"KOG2", i32 x0, z0, u32 w, h, then zlib(tipo|distrito<<4 ‖ altura ‖ estilo ‖ deck), row-major.
    tipo_d = (T | (dist << 4)).astype(np.uint8)
    with open(saida, "wb") as f:
        f.write(b"KOG2" + struct.pack("<iiII", X0, Z0, W, H))
        f.write(zlib.compress(tipo_d.tobytes() + A.tobytes() + E.tobytes() + D.tobytes(), 9))

    if len(sys.argv) > 3:
        cores = np.array([
            (20, 30, 60), (150, 150, 150), (60, 60, 70), (80, 80, 92), (230, 200, 40), (240, 240, 240),
            (125, 125, 135), (105, 100, 110), (70, 140, 70), (40, 100, 50), (110, 85, 65), (0, 0, 0),
            (150, 90, 70)], dtype=np.uint8)
        img = cores[T].astype(np.float32)
        luz = (A.astype(np.float32) / 110.0).clip(0, 1)
        pr = T == PREDIO
        img[pr] = np.stack([90 + 165 * luz, 95 + 140 * luz, 130 + 110 * luz], -1)[pr]
        img[D > 0] = (230, 140, 40)
        tons = np.array([(0, 0, 0), (40, 0, 0), (0, 40, 0), (40, 0, 40), (0, 0, 50), (50, 40, 0)], np.float32)
        img[~mar] += tons[dist[~mar]] * 0.6
        Image.fromarray(img.clip(0, 255).astype(np.uint8)).save(sys.argv[3])
    print(f"{rampas} ramps; {len(predios)} buildings; {int((T == PREDIO).sum())} building cells; {int((D > 0).sum())} deck cells;"
          f" tallest {int(A[T == PREDIO].max())} blocks; {W}x{H} cells")


if __name__ == "__main__":
    main()
