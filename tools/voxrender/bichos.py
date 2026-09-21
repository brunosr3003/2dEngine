#!/usr/bin/env python3
"""As CRIATURAS em PECAS, pra andar como no zone14.

O `mobs.py` entrega o bicho inteiro, parado: um bloco so', que desliza pelo
chao. Aqui sai o mesmo bicho fatiado — tronco, cabeca, pescoco, cauda e as
quatro patas —, cada fatia num objeto nomeado do grafo de cena, todas na MESMA
tela. E' isso que deixa o cliente mover a pata sem o resto (`bicho.rs`).

As fatias vem prontas do zone14 (`models/<bicho>_<peca>.vox`), no grid de
128 do modelo inteiro. Duas regras pra elas continuarem encaixando:

1. **o mesmo fator pra todas.** Reduzir cada peca pelo proprio orcamento
   daria resolucoes diferentes e a pata nao casaria com o tronco;
2. **o mesmo grid.** A reducao junta blocos de fator^3 contados da origem da
   tela de 128, igual pra todas; so' depois tudo desce pro canto.

`head_neck` fica de fora, como no conversor do zone14: e' a cabeca e o
pescoco num arquivo so', e as duas soltas ja' estao aqui.

Uso:  python3 tools/voxrender/bichos.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from voxrender import parse_vox  # noqa: E402
from voxsimplify import reduzir  # noqa: E402
import molde_corpo  # noqa: E402

MODELOS = os.path.expanduser("~/zone14/models")

# (arquivo no jogo, prefixo no zone14, faces no maximo)
#
# Mob comum aparece as dezenas; o chefe e' um so' e leva o detalhe
# (docs/PIPELINE_ARTE.md). O orcamento e' a SOMA das pecas: fatiar cria faces
# nas emendas, entao o mesmo bicho fatiado custa um pouco mais que inteiro.
BICHOS = [
    ("lobo_pequeno", "wolf",    2600),
    ("urso",         "bear",    2600),
    ("tigre",        "tiger",   2600),
    ("owlbear",      "owlbear", 2600),
    ("lobo",         "wolf",   12000),   # o chefe
    # A escada de pet e montaria: uma CRIATURA por cor, e nao a mesma
    # tingida (docs/PETS.md, docs/MONTARIAS.md). Orcamento de mob comum —
    # o laranja aparece pouco, mas aparece ao lado de outros jogadores.
    ("cervo",        "deer",       2600),
    ("hipogrifo",    "hippogriff", 3400),   # tem asa
    ("dragao",       "dragon",     3800),   # tem asa
    ("porco",        "pig",        2200),
    # ── O BESTIARIO POR ILHA (docs/MUNDO.md) ──
    #
    # Ate' 21/09/2026 as quatro ilhas sorteavam da MESMA lista, e o dono viu
    # caranguejo na Geleira. Cada ilha passa a ter os bichos dela, e estes
    # sao os que faltavam. Todos QUADRUPEDES: e' o rig que o `bicho.rs` sabe
    # animar, e um modelo inteiro (o zone14 tem varios) entraria parado no
    # meio de bicho que anda.
    ("escaravelho",  "escaravelho", 2600),   # Ermo: o besouro gigante
    ("escaravelho_rainha", "rainha", 3400),  # Ermo: a rainha, com as placas
    ("morsa",        "morsa",       2600),   # Geleira: a morsa
    ("rochoso",      "rochoso",     2600),   # Planalto: a criatura de pedra
]

# arquivo do zone14 -> nome da peca no jogo (ver `bicho::junta_de`)
# Cada bicho do zone14 nomeia a pata do jeito dele — `paw` no lobo, `hoof` no
# veado e no hipogrifo, `trotter` no porco, `talon` nas garras da frente do
# hipogrifo. Todos caem nas MESMAS quatro juntas do cliente: o que muda e' o
# desenho, nao o esqueleto.
PECAS = {
    "body": "tronco", "head": "cabeca", "neck": "pescoco", "tail": "cauda",
    "paw_front_right": "pata_fd", "paw_front_left": "pata_fe",
    "paw_back_right": "pata_td", "paw_back_left": "pata_te",
    "hoof_front_right": "pata_fd", "hoof_front_left": "pata_fe",
    "hoof_back_right": "pata_td", "hoof_back_left": "pata_te",
    "trotter_front_right": "pata_fd", "trotter_front_left": "pata_fe",
    "trotter_back_right": "pata_td", "trotter_back_left": "pata_te",
    "talon_front_right": "pata_fd", "talon_front_left": "pata_fe",
    "wing_right": "asa_d", "wing_left": "asa_e",
    # Alguns so' tem cabeca e pescoco num arquivo so'.
    "head_snout": "cabeca", "head_antlers": "cabeca",
    # Os insetos e a criatura de pedra chamam a pata de `claw`, e nao tem
    # pescoco nem cauda — a junta some e o resto anima igual.
    "claw_front_right": "pata_fd", "claw_front_left": "pata_fe",
    "claw_back_right": "pata_td", "claw_back_left": "pata_te",
    # Pecas que nao tem junta propria entram na junta VIZINHA: o focinho e a
    # mandibula vao com a cabeca, as placas da rainha com o tronco. Elas so'
    # funcionam porque o pivo e' da JUNTA e nao da peca (`vox::load_bicho`) —
    # cada uma com o seu pivo abriria fresta na primeira passada.
    "snout": "cabeca",
    "mandibula_1": "cabeca",
    "placa_1": "tronco", "placa_2": "tronco", "placa_3": "tronco",
    "placa_4": "tronco", "placa_5": "tronco", "placa_6": "tronco",
}


def faces(v):
    s = set(v)
    return sum(1 for (x, y, z) in s for d in ((1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1))
               if (x + d[0], y + d[1], z + d[2]) not in s)


def carrega(prefixo):
    pecas, paleta = [], None
    for arquivo, nome in PECAS.items():
        caminho = os.path.join(MODELOS, f"{prefixo}_{arquivo}.vox")
        if not os.path.exists(caminho):
            continue
        m = max(parse_vox(caminho), key=lambda x: len(x.voxels))
        pecas.append((nome, m))
        paleta = paleta or m.palette
    return pecas, paleta


def monta(saida, prefixo, orcamento):
    pecas, paleta = carrega(prefixo)
    fator = 1
    while True:
        reduzidas = [(n, reduzir(m, fator)) for n, m in pecas]
        total = sum(faces(v) for _, v in reduzidas)
        if total <= orcamento:
            break
        fator += 1
    tudo = [k for _, v in reduzidas for k in v]
    lo = [min(k[i] for k in tudo) for i in range(3)]
    tam = [max(k[i] for k in tudo) - lo[i] + 1 for i in range(3)]
    finais = [(n, {(x - lo[0], y - lo[1], z - lo[2]): c for (x, y, z), c in v.items()})
              for n, v in reduzidas if v]

    # a tela do molde vira a deste bicho
    molde_corpo.W, molde_corpo.D, molde_corpo.H = tam
    molde_corpo.TRANSL = f"0 0 {tam[2] // 2}"
    cores = {i: tuple(paleta[i][:3]) for i in range(1, 256)}
    with open(saida, "wb") as f:
        f.write(molde_corpo.arquivo_cena(finais, cores, camada=prefixo))
    print(f"{os.path.basename(saida)}: fator {fator}, {total} faces, tela {tam[0]}x{tam[1]}x{tam[2]}, "
          + ", ".join(f"{n}={len(v)}" for n, v in finais))


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "bichos")
    os.makedirs(pasta, exist_ok=True)
    for nome, prefixo, orcamento in BICHOS:
        monta(os.path.join(pasta, f"{nome}.vox"), prefixo, orcamento)
