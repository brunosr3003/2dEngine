#!/usr/bin/env python3
"""Gera o MOLDE do corpo: `tools/moldes/corpo_molde.vox`.

Um arquivo do MagicaVoxel com as dez pecas do rig ja' NOMEADAS, cada uma num
objeto proprio, todas no mesmo quadro (mesmo tamanho de tela, mesma posicao).
E' o ponto de partida da arte: abre, esculpe por cima, salva. Nome, tela e
pivo ja' estao certos — e sao justamente as tres coisas que o importador do
jogo nao consegue adivinhar.

As medidas moram aqui e em `docs/ARTE_DO_PERSONAGEM.md`; se mudarem, mudam
nos dois.

Uso:  python3 tools/voxrender/molde_corpo.py [--plano saida.vox]
      (--plano grava tambem uma versao de UM objeto, so' pra render)
"""
import struct, sys, os

W, D, H = 32, 24, 48          # tela: largura (x), profundidade (y), altura (z)
TRANSL = "0 0 24"             # todos os objetos na MESMA posicao

# ── paleta ──────────────────────────────────────────────────────────────
# Indices reservados (1..255). O resto e' livre.
TIER  = (241, 242, 243, 244)  # cor do tier: claro -> escuro
CABELO = (245, 246, 247, 248)
PELE  = (249, 250, 251, 252)
MARCA = 255                   # marcador (pega de arma); nao e' desenhado
CAMISA, CAMISA_ESC, CALCA, BOTA, CINTO, MANGA, MANGA2, COXA, CANELA, OLHO = range(1, 11)
cores = {
    CAMISA: (206, 214, 224), CAMISA_ESC: (170, 180, 194), CALCA: (58, 72, 104),
    BOTA: (96, 66, 44), CINTO: (70, 52, 36), MANGA: (190, 200, 214),
    MANGA2: (150, 162, 180), COXA: (66, 82, 118), CANELA: (46, 58, 86),
    OLHO: (60, 110, 200),
    TIER[0]: (214, 220, 228), TIER[1]: (176, 182, 190), TIER[2]: (138, 144, 152), TIER[3]: (100, 104, 112),
    CABELO[0]: (122, 86, 54), CABELO[1]: (98, 68, 42), CABELO[2]: (76, 52, 32), CABELO[3]: (54, 36, 22),
    PELE[0]: (240, 204, 170), PELE[1]: (222, 180, 146), PELE[2]: (196, 152, 120), PELE[3]: (164, 122, 96),
    MARCA: (255, 0, 255),
}

def caixa(x0, x1, y0, y1, z0, z1, c):
    return {(x, y, z): c for x in range(x0, x1 + 1) for y in range(y0, y1 + 1) for z in range(z0, z1 + 1)}

def junta(*ds):
    out = {}
    for d in ds: out.update(d)
    return out

# ── as dez pecas ────────────────────────────────────────────────────────
# O personagem OLHA PRA +Y. O lado DIREITO dele e' o de X MAIOR.
# Chao em z=0; eixo do corpo no plano x=16, y=12.
cabeca = junta(
    caixa(14, 17, 10, 13, 33, 33, PELE[1]),          # pescoco
    caixa(12, 19, 8, 15, 34, 41, PELE[1]),           # cabeca 8x8x8
    caixa(13, 14, 15, 15, 37, 38, OLHO),             # olho esquerdo dele
    caixa(17, 18, 15, 15, 37, 38, OLHO),             # olho direito dele
    caixa(15, 16, 15, 15, 35, 35, PELE[2]),          # boca
)
torso = junta(
    caixa(11, 20, 9, 14, 20, 32, CAMISA),
    caixa(11, 20, 9, 14, 20, 22, CALCA),             # bacia
    caixa(11, 20, 9, 14, 23, 23, CINTO),
    caixa(11, 20, 9, 9, 24, 32, CAMISA_ESC),         # costas, pra ler a frente
)
def braco(x0, x1):   return caixa(x0, x1, 10, 13, 25, 32, MANGA)
# A TAMPA: a peca de baixo de cada junta que dobra (cotovelo, joelho) sobe
# dois voxels, recuada um de cada lado, pra DENTRO da peca de cima. Parado,
# ela fica escondida la' dentro e nao aparece nem briga com a face de fora;
# dobrando, e' ela que tampa o buraco que abriria na junta.
def tampa(x0, x1, z0, c): return caixa(x0 + 1, x1 - 1, 11, 12, z0, z0 + 1, c)
def antebraco(x0, x1):
    return junta(caixa(x0, x1, 10, 13, 20, 24, MANGA2), caixa(x0, x1, 10, 13, 16, 19, PELE[1]),
                 tampa(x0, x1, 25, MANGA2))
def coxa(x0, x1):    return caixa(x0, x1, 10, 13, 10, 19, COXA)
def canela(x0, x1):
    return junta(caixa(x0, x1, 10, 13, 2, 9, CANELA), caixa(x0, x1, 10, 15, 0, 1, BOTA),
                 tampa(x0, x1, 10, CANELA))

pecas = [
    ("cabeca", cabeca), ("torso", torso),
    ("braco_d", braco(21, 24)), ("antebraco_d", antebraco(21, 24)),
    ("braco_e", braco(7, 10)),  ("antebraco_e", antebraco(7, 10)),
    ("coxa_d", coxa(16, 19)),   ("canela_d", canela(16, 19)),
    ("coxa_e", coxa(12, 15)),   ("canela_e", canela(12, 15)),
]

# ── escrita .vox (com grafo de cena) ────────────────────────────────────
def s(t):  b = t.encode(); return struct.pack("<i", len(b)) + b
def dic(d):
    out = struct.pack("<i", len(d))
    for k, v in d.items(): out += s(k) + s(v)
    return out
def chunk(cid, conteudo, filhos=b""):
    return cid.encode() + struct.pack("<ii", len(conteudo), len(filhos)) + conteudo + filhos

def rgba(paleta=None):
    paleta = paleta or cores
    out = b""
    for i in range(1, 257):
        r, g, b = paleta.get(i, (0, 0, 0)) if i < 256 else (0, 0, 0)
        out += bytes((r, g, b, 255))
    return chunk("RGBA", out)

def modelo(vox):
    xyzi = struct.pack("<i", len(vox)) + b"".join(bytes((x, y, z, c)) for (x, y, z), c in sorted(vox.items()))
    return chunk("SIZE", struct.pack("<iii", W, D, H)) + chunk("XYZI", xyzi)

def arquivo_cena(pecas, paleta=None, camada="corpo"):
    filhos = b"".join(modelo(v) for _, v in pecas)
    n = len(pecas)
    filhos += chunk("nTRN", struct.pack("<i", 0) + dic({}) + struct.pack("<iiii", 1, -1, -1, 1) + dic({}))
    filhos += chunk("nGRP", struct.pack("<i", 1) + dic({}) + struct.pack("<i", n)
                    + b"".join(struct.pack("<i", 2 + 2 * k) for k in range(n)))
    for k, (nome, _) in enumerate(pecas):
        filhos += chunk("nTRN", struct.pack("<i", 2 + 2 * k) + dic({"_name": nome})
                        + struct.pack("<iiii", 3 + 2 * k, -1, 0, 1) + dic({"_t": TRANSL}))
        filhos += chunk("nSHP", struct.pack("<i", 3 + 2 * k) + dic({}) + struct.pack("<i", 1)
                        + struct.pack("<i", k) + dic({}))
    filhos += chunk("LAYR", struct.pack("<i", 0) + dic({"_name": camada}) + struct.pack("<i", -1))
    filhos += rgba(paleta)
    return b"VOX " + struct.pack("<i", 200) + chunk("MAIN", b"", filhos)

def arquivo_plano(pecas, paleta=None):
    tudo = {}
    for _, v in pecas: tudo.update(v)
    return b"VOX " + struct.pack("<i", 150) + chunk("MAIN", b"", modelo(tudo) + rgba(paleta))

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    saida = os.path.join(raiz, "moldes", "corpo_molde.vox")
    open(saida, "wb").write(arquivo_cena(pecas))
    print(f"molde: {saida}  ({len(pecas)} pecas, tela {W}x{D}x{H})")
    if "--plano" in sys.argv:
        p = sys.argv[sys.argv.index("--plano") + 1]
        open(p, "wb").write(arquivo_plano(pecas))
        print(f"plano: {p}")
