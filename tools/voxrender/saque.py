#!/usr/bin/env python3
"""O saquinho do saque: `assets/vox/saque.vox`.

O que o bicho deixa no chao. Ate' aqui o saque nao tinha modelo e o cliente
desenhava o cubo reserva, verde — que o jogador leu como "o urso virou um
quadrado verde".

Saco de estopa amarrado na boca, com uma FAIXA em volta da barriga pintada
nos indices reservados do tier (241-244). O cliente troca essa faixa pela cor
da raridade do item — cinza, verde, azul, roxo, as mesmas do cristal da
pedra; dourada pra ouro e pocao, que nao tem tier. A faixa fica na barriga e
nao embaixo porque e' ali que a camera de cima enxerga.

Uso:  python3 tools/voxrender/saque.py [--previa pasta]
"""
import os, sys, math
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M

ESTOPA, ESTOPA_ESC, CORDA, FUNDO = 1, 2, 3, 4
TIER = M.TIER
paleta = {
    ESTOPA: (176, 140, 92), ESTOPA_ESC: (148, 114, 72), CORDA: (104, 76, 44), FUNDO: (122, 94, 60),
    TIER[0]: (214, 220, 228), TIER[1]: (176, 182, 190), TIER[2]: (138, 144, 152), TIER[3]: (100, 104, 112),
}
CX, CY = 16, 12            # centro na tela comum (a malha recentra pela caixa)
# raio do saco por altura: largo embaixo, apertado na boca amarrada
PERFIL = [3.0, 3.8, 4.2, 4.3, 4.1, 3.6, 2.8, 1.6, 1.9, 1.2]

def saco():
    v = {}
    for z, r in enumerate(PERFIL):
        for x in range(CX - 5, CX + 6):
            for y in range(CY - 5, CY + 6):
                if (x - CX + 0.5) ** 2 + (y - CY + 0.5) ** 2 > r * r:
                    continue
                if z == 0:
                    c = FUNDO
                elif z == 7:
                    c = CORDA                       # a amarra
                else:
                    c = ESTOPA if (x + y + z) % 3 else ESTOPA_ESC   # trama da estopa
                v[(x, y, z)] = c
    # faixa do tier na barriga (z 3), so' na casca de fora
    for (x, y, z), c in list(v.items()):
        if z == 3 and (x - CX + 0.5) ** 2 + (y - CY + 0.5) ** 2 > 3.1 ** 2:
            v[(x, y, z)] = TIER[1] if (x + y) % 2 else TIER[0]
    # pontas da boca franzida e o no' da corda caindo na frente (+Y)
    for p in [(CX, CY, 10), (CX - 1, CY, 10), (CX, CY - 1, 10)]:
        v[p] = ESTOPA
    for p in [(CX, CY + 2, 6), (CX, CY + 3, 5), (CX + 1, CY + 3, 4)]:
        v[p] = CORDA
    return v

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    s = saco()
    open(os.path.join(raiz, "assets", "vox", "saque.vox"), "wb").write(M.arquivo_plano([("saque", s)], paleta))
    zs = [p[2] for p in s]
    print(f"saque.vox: {len(s)} voxels, {max(zs) - min(zs) + 1} de altura")
    if "--previa" in sys.argv:
        pasta = sys.argv[sys.argv.index("--previa") + 1]
        # as cinco cores que o cliente usa (ver render3d::variantes_do_saque)
        claro = {"0": (235, 190, 50), "1": (214, 220, 228), "2": (96, 226, 138), "3": (92, 176, 250), "4": (186, 112, 246)}
        for nome, c in claro.items():
            p = dict(paleta)
            for k, f in enumerate((1.0, 0.82, 0.66, 0.5)):
                p[TIER[k]] = tuple(int(x * f) for x in c)
            open(os.path.join(pasta, f"saque_{nome}.vox"), "wb").write(M.arquivo_plano([("saque", s)], p))
