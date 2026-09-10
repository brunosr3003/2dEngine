#!/usr/bin/env python3
"""Reduz um modelo `.vox`: menos voxel, menos triangulo, menos custo no celular.

Existe por causa do orcamento de arte: **mob comum tem que ser barato**. O
`lobo.vox` do zone14 tem 71 mil voxels e da 14.112 triangulos depois do greedy
meshing — isso e' modelo de BOSS, do qual ha um na tela. Um mob comum, do qual
ha dezenas, precisa de uma fracao disso.

A reducao e' por blocos: cada cubo NxNxN vira um voxel so', com a cor que mais
aparece dentro dele (a moda). Isso preserva a silhueta e a paleta, e derruba a
contagem de superficie — que e' o que vira triangulo.

    voxsimplify.py lobo.vox -o lobo_pequeno.vox -f 2
    voxsimplify.py lobo.vox -o lobo_pequeno.vox -f 2 --escala 0.7
"""

import argparse
import struct
import sys
from collections import Counter

sys.path.insert(0, __import__("os").path.dirname(__file__))
from voxrender import parse_vox  # noqa: E402


def reduzir(model, fator):
    """Junta blocos de `fator`^3 num voxel, pela cor mais frequente."""
    blocos = {}
    for (x, y, z), c in model.voxels.items():
        chave = (x // fator, y // fator, z // fator)
        blocos.setdefault(chave, Counter())[c] += 1
    return {k: v.most_common(1)[0][0] for k, v in blocos.items()}


def escrever_vox(path, voxels, size, palette):
    """Serializa no formato 150 do MagicaVoxel: SIZE + XYZI + RGBA."""
    def chunk(cid, conteudo, filhos=b""):
        return cid + struct.pack("<ii", len(conteudo), len(filhos)) + conteudo + filhos

    size_c = chunk(b"SIZE", struct.pack("<iii", *size))
    xyzi = struct.pack("<i", len(voxels))
    for (x, y, z), c in voxels.items():
        xyzi += bytes((x, y, z, c))
    xyzi_c = chunk(b"XYZI", xyzi)
    rgba = b"".join(bytes(palette[i + 1]) for i in range(255)) + bytes(4)
    rgba_c = chunk(b"RGBA", rgba)
    main = chunk(b"MAIN", b"", size_c + xyzi_c + rgba_c)
    with open(path, "wb") as f:
        f.write(b"VOX " + struct.pack("<i", 150) + main)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("input")
    ap.add_argument("-o", "--out", required=True)
    ap.add_argument("-f", "--fator", type=int, default=2,
                    help="lado do bloco que vira 1 voxel (default 2)")
    ap.add_argument("--escala", type=float, default=1.0,
                    help="encolhe o modelo depois de reduzir (0.7 = 70%% do tamanho)")
    args = ap.parse_args()

    models = parse_vox(args.input)
    if not models:
        sys.exit(f"{args.input}: sem modelo")
    m = max(models, key=lambda x: len(x.voxels))
    antes = len(m.voxels)

    voxels = reduzir(m, args.fator)
    if args.escala != 1.0:
        # Encolher e' outra reducao por blocos, agora com fator fracionario.
        inv = 1.0 / args.escala
        blocos = {}
        for (x, y, z), c in voxels.items():
            chave = (int(x / inv), int(y / inv), int(z / inv))
            blocos.setdefault(chave, Counter())[c] += 1
        voxels = {k: v.most_common(1)[0][0] for k, v in blocos.items()}

    # Encosta no canto e dimensiona a caixa pelo que sobrou.
    mx = min(k[0] for k in voxels); my = min(k[1] for k in voxels); mz = min(k[2] for k in voxels)
    voxels = {(x - mx, y - my, z - mz): c for (x, y, z), c in voxels.items()}
    size = tuple(max(k[i] for k in voxels) + 1 for i in range(3))
    if max(size) > 255:
        sys.exit(f"modelo reduzido ainda tem {size} — aumente o fator")

    escrever_vox(args.out, voxels, size, m.palette)
    print(f"{antes} -> {len(voxels)} voxels ({len(voxels)*100//antes}%)  caixa {size}  -> {args.out}")


if __name__ == "__main__":
    main()
