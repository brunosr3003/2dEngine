#!/usr/bin/env python3
"""Renderiza modelos MagicaVoxel (.vox) em folhas de sprite pro cliente.

O pipeline e' o do Dead Cells: a FONTE da arte e' o modelo 3D, o sprite 2D e'
saida de build. Isso resolve duas coisas de uma vez —

  * arte 100% autoral, que e' o que a Apple cobra numa rejeicao 4.3;
  * as 8 direcoes saem de graca girando o modelo, que e' justamente a parte
    cara de manter num paper doll desenhado a mao.

Os `.vox` entram no git (sao pequenos); os PNGs gerados NAO — sao build output.

Uso:
    voxrender.py modelo.vox -o saida.png            # 8 direcoes, escala 4
    voxrender.py modelo.vox -o saida.png -d 4 -s 6  # 4 direcoes, escala 6
    voxrender.py pasta/ -o assets/mobs/             # lote

O formato .vox e' RIFF-like: chunks SIZE (dimensoes), XYZI (voxels) e RGBA
(paleta de 256 cores). Arquivos com cena (nTRN/nGRP/nSHP) tem varios modelos;
aqui os modelos sao renderizados separadamente.
"""

import argparse
import math
import os
import struct
import sys

from PIL import Image, ImageDraw

# Paleta default do MagicaVoxel quando o arquivo nao traz chunk RGBA.
DEFAULT_PALETTE_HINT = (200, 200, 200, 255)

# Brilho por face. O topo pega a luz cheia; as laterais escurecem pra dar
# volume sem precisar de luz de verdade.
FACE_SHADE = {
    (0, 0, 1): 1.00,    # topo
    (0, 0, -1): 0.35,   # base (raramente visivel)
    (1, 0, 0): 0.78,
    (-1, 0, 0): 0.62,
    (0, 1, 0): 0.70,
    (0, -1, 0): 0.86,
}

# Cantos de cada face de um cubo unitario, em ordem de poligono.
FACE_CORNERS = {
    (0, 0, 1): [(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)],
    (0, 0, -1): [(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)],
    (1, 0, 0): [(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)],
    (-1, 0, 0): [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
    (0, 1, 0): [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
    (0, -1, 0): [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
}


class Model:
    def __init__(self, size, voxels, palette):
        self.size = size          # (sx, sy, sz)
        self.voxels = voxels      # {(x, y, z): color_index}
        self.palette = palette    # lista de 256 RGBA


def parse_vox(path):
    """Le um .vox e devolve a lista de modelos que ele contem."""
    data = open(path, "rb").read()
    if data[:4] != b"VOX ":
        raise ValueError(f"{path}: nao e' um .vox")

    models = []
    palette = None
    pending_size = None

    def walk(i, end):
        nonlocal palette, pending_size
        while i < end:
            cid = data[i:i + 4]
            n, m = struct.unpack("<ii", data[i + 4:i + 12])
            body = i + 12
            if cid == b"SIZE":
                pending_size = struct.unpack("<iii", data[body:body + 12])
            elif cid == b"XYZI":
                count = struct.unpack("<i", data[body:body + 4])[0]
                voxels = {}
                off = body + 4
                for _ in range(count):
                    x, y, z, c = data[off:off + 4]
                    voxels[(x, y, z)] = c
                    off += 4
                models.append(Model(pending_size, voxels, None))
            elif cid == b"RGBA":
                # O chunk guarda as cores 1..255 no inicio; o indice 0 e' vazio.
                pal = [(0, 0, 0, 0)]
                for k in range(255):
                    off = body + k * 4
                    pal.append(tuple(data[off:off + 4]))
                palette = pal
            if m:
                walk(body + n, body + n + m)
            i = body + n + m

    walk(8, len(data))
    if palette is None:
        palette = [(0, 0, 0, 0)] + [DEFAULT_PALETTE_HINT] * 255
    for mdl in models:
        mdl.palette = palette
    return models


def close_z_gaps(model):
    """Junta as pecas que o modelo guarda separadas.

    Os modelos do zone14 sao autorados com as partes SOLTAS pra poder animar:
    o `lobo.vox`, por exemplo, tem as patas em z 0..12 e o corpo so' a partir
    de z 31 — 18 camadas vazias no meio. Renderizado cru, o bicho sai partido.

    Aqui as camadas vazias somem e as pecas se encostam. A separacao original
    continua util: e' ela que da os grupos pra animar depois (mover so' o grupo
    das patas por quadro), entao isto e' colapso de espaco, nao perda de dado.
    """
    zs = sorted({z for (_, _, z) in model.voxels})
    if not zs:
        return model
    remap = {}
    novo_z = zs[0]
    anterior = None
    for z in zs:
        if anterior is not None and z > anterior + 1:
            novo_z += 1          # encosta a peca de cima na de baixo
        else:
            novo_z = novo_z + 1 if anterior is not None else z
        remap[z] = novo_z
        anterior = z
    voxels = {(x, y, remap[z]): c for (x, y, z), c in model.voxels.items()}
    sx, sy, _ = model.size
    return Model((sx, sy, max(remap.values()) + 1), voxels, model.palette)


def surface_faces(model):
    """Faces visiveis: as que fazem fronteira com vazio.

    Sem isto um modelo solido gera dezenas de milhares de poligonos internos
    que nunca aparecem — o `lobo.vox` tem 71 mil voxels e so' a casca importa.
    """
    vox = model.voxels
    faces = []
    for (x, y, z), c in vox.items():
        for nrm in FACE_CORNERS:
            nx, ny, nz = x + nrm[0], y + nrm[1], z + nrm[2]
            if (nx, ny, nz) not in vox:
                faces.append(((x, y, z), nrm, c))
    return faces


def render(model, angle_deg, scale, elevation_deg, cell=None, bg=(0, 0, 0, 0)):
    """Projeta o modelo num quadro.

    Projecao ortografica: gira em torno de Z (a direcao do personagem) e
    inclina a camera pela elevacao. Elevacao 90 seria topo puro; o padrao 35
    da a vista 3/4 que um top-down 2D pede — da pra ver a frente do bicho e
    ainda entender a posicao dele no chao.
    """
    a = math.radians(angle_deg)
    ca, sa = math.cos(a), math.sin(a)
    e = math.radians(elevation_deg)
    ce, se = math.cos(e), math.sin(e)

    sx, sy, sz = model.size
    cx, cy = sx / 2.0, sy / 2.0

    def project(px, py, pz):
        rx = (px - cx) * ca - (py - cy) * sa
        ry = (px - cx) * sa + (py - cy) * ca
        # +Y entra na tela, +Z sobe. Y na tela cresce pra baixo.
        return rx * scale, (ry * se - pz * ce) * scale

    def depth(px, py, pz):
        ry = (px - cx) * sa + (py - cy) * ca
        return ry * ce + pz * se

    faces = surface_faces(model)
    # Pintor: desenha do mais longe pro mais perto.
    faces.sort(key=lambda f: depth(f[0][0] + 0.5, f[0][1] + 0.5, f[0][2] + 0.5))

    polys = []
    minx = miny = 1e9
    maxx = maxy = -1e9
    for (vx, vy, vz), nrm, c in faces:
        # Descarta face que aponta pra longe da camera.
        wn = (nrm[0] * ca - nrm[1] * sa, nrm[0] * sa + nrm[1] * ca, nrm[2])
        if wn[1] * ce + wn[2] * se >= 0:
            continue
        pts = []
        for dx, dy, dz in FACE_CORNERS[nrm]:
            px, py = project(vx + dx, vy + dy, vz + dz)
            pts.append((px, py))
            minx = min(minx, px); maxx = max(maxx, px)
            miny = min(miny, py); maxy = max(maxy, py)
        r, g, b, al = model.palette[c]
        k = FACE_SHADE[nrm]
        polys.append((pts, (int(r * k), int(g * k), int(b * k), al)))

    if not polys:
        raise ValueError("modelo vazio")

    w = cell or int(math.ceil(maxx - minx)) + 2
    h = cell or int(math.ceil(maxy - miny)) + 2
    img = Image.new("RGBA", (w, h), bg)
    d = ImageDraw.Draw(img)
    # Centraliza horizontalmente e encosta os pes na base do quadro: o cliente
    # ancora o sprite pelo pe, entao todos os quadros precisam do mesmo chao.
    ox = w / 2.0 - (minx + maxx) / 2.0
    oy = h - 1 - maxy
    for pts, color in polys:
        d.polygon([(x + ox, y + oy) for x, y in pts], fill=color)
    return img


def sheet(model, dirs, scale, elevation, start_angle=0.0):
    """Uma folha com as direcoes lado a lado, todas do mesmo tamanho."""
    frames = [
        render(model, start_angle + i * (360.0 / dirs), scale, elevation)
        for i in range(dirs)
    ]
    cw = max(f.width for f in frames)
    ch = max(f.height for f in frames)
    out = Image.new("RGBA", (cw * dirs, ch), (0, 0, 0, 0))
    for i, f in enumerate(frames):
        # Centraliza na celula mantendo o pe na base.
        out.paste(f, (i * cw + (cw - f.width) // 2, ch - f.height), f)
    return out, cw, ch


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("input", help="arquivo .vox ou pasta")
    ap.add_argument("-o", "--out", required=True, help="PNG de saida ou pasta")
    ap.add_argument("-d", "--dirs", type=int, default=8, help="direcoes (default 8)")
    ap.add_argument("-s", "--scale", type=float, default=4.0, help="pixels por voxel")
    ap.add_argument("-e", "--elevation", type=float, default=35.0,
                    help="elevacao da camera em graus (90 = topo puro)")
    ap.add_argument("--angle", type=float, default=0.0, help="angulo inicial")
    ap.add_argument("--keep-gaps", action="store_true",
                    help="nao juntar as pecas separadas do modelo (ver close_z_gaps)")
    args = ap.parse_args()

    inputs = []
    if os.path.isdir(args.input):
        inputs = [os.path.join(args.input, f) for f in sorted(os.listdir(args.input))
                  if f.endswith(".vox")]
        os.makedirs(args.out, exist_ok=True)
    else:
        inputs = [args.input]

    for path in inputs:
        try:
            models = parse_vox(path)
        except Exception as exc:
            print(f"[skip] {path}: {exc}", file=sys.stderr)
            continue
        if not models:
            print(f"[skip] {path}: sem modelo", file=sys.stderr)
            continue
        # Arquivo com varios modelos: usa o maior (os outros costumam ser
        # pecas soltas, tipo as barbatanas do tubarao).
        mdl = max(models, key=lambda m: len(m.voxels))
        if not args.keep_gaps:
            mdl = close_z_gaps(mdl)
        img, cw, ch = sheet(mdl, args.dirs, args.scale, args.elevation, args.angle)
        if os.path.isdir(args.out):
            name = os.path.splitext(os.path.basename(path))[0] + ".png"
            dst = os.path.join(args.out, name)
        else:
            dst = args.out
        img.save(dst)
        print(f"{os.path.basename(path):<24} {len(mdl.voxels):>7} voxels  "
              f"-> {os.path.basename(dst)}  celula {cw}x{ch}  {args.dirs} dirs")


if __name__ == "__main__":
    main()
