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
    # Os BRANCOS da Geleira. Mesma malha do urso e do tigre, PALETA outra —
    # ver `PELAGENS`.
    ("urso_polar",   "bear",        2600),
    ("tigre_branco", "tiger",       2600),
    # Island variants (`shared::bestiary`): the species' mesh in the island's
    # coat, swapped by palette index like the white ones.
    ("snow_owlbear",  "owlbear",    2600),   # Glacier
    ("storm_owlbear", "owlbear",    2600),   # Plateau
    ("crag_lynx",     "tiger",      2600),   # Plateau
    ("cave_bear",     "bear",       2600),   # Plateau
]

# ── A PELAGEM: trocar a COR DA PELE, indice a indice ──
#
# O urso branco e o tigre branco saiam do urso e do tigre com um
# multiplicador de cor por vertice: a malha inteira puxada pro branco. O dono
# olhou e disse o obvio — "urso normal pintado de branco, isso nao existe,
# fica muito feio". E era isso mesmo: multiplicar TUDO come o que nao e'
# pelo. A listra preta do tigre clareava junto (o tigre branco e' branco COM
# listra preta, senao e' um gato), o nariz rosa virava branco, a boca vermelha
# sumia e o olho perdia o brilho. Tinta por cima nao sabe o que e' pelo.
#
# Aqui a troca e' por INDICE de paleta: mexe na rampa do pelo e deixa nariz,
# boca, garra e olho em paz. Custa um `.vox` a mais por bicho (a malha e' a
# mesma geometria, so' que com outras cores assadas no vertice) e nenhum
# passe de render — e e' a unica forma de um animal de outra cor ser outro
# animal, e nao o mesmo com um filtro em cima.
#
# Os indices sairam de ler os modelos do zone14:
#   bear   2,3,4,5 = a rampa do pelo · 9 = o focinho · 8 = o nariz (preto)
#          1 = coxim · 10,13 = a garra · 11 = a boca · 7 = a pele do focinho
#   tiger  4,5 = a rampa do pelo · 1 = A LISTRA · 7 = a barriga/rosto branco
#          9 = o focinho · 8 = o nariz (rosa) · 6 = o pelo da pata
PELAGENS = {
    # Urso polar: a rampa marrom vira uma rampa FRIA (o branco puro achata a
    # silhueta contra a neve; o azul e' o que devolve o volume). Nariz preto,
    # boca e garra intocados.
    "urso_polar": {
        2: (140, 152, 172),   # sombra
        3: (196, 206, 222),   # o pelo do corpo
        4: (224, 232, 242),
        5: (243, 248, 255),   # luz
        9: (226, 224, 214),   # o focinho, creme — nao dourado
        7: (210, 206, 196),
    },
    # Tigre branco: o laranja vira branco-creme e A LISTRA CONTINUA PRETA —
    # e' ela que faz o bicho. O nariz rosa (8) fica, que e' assim no animal.
    "tigre_branco": {
        4: (226, 231, 240),   # o pelo do corpo
        5: (243, 247, 252),
        7: (250, 252, 255),   # barriga e rosto, branco puro
        9: (240, 244, 250),   # o focinho
    },
    # Indices read from zone14's owlbear: 1,2,3 = the feather ramp (dark to
    # light) · 4,5 = face and beak · 7 = the eye · 9 = claws · 10 = mouth.
    # Snow Owlbear: a cold grey-white ramp; beak, eye and claws untouched.
    "snow_owlbear": {
        1: (112, 124, 144),
        2: (172, 184, 204),
        3: (220, 228, 240),
        4: (194, 184, 168),
        5: (236, 228, 214),
    },
    # Storm Owlbear: slate and indigo, and an eye that glows like the storm.
    "storm_owlbear": {
        1: (28, 30, 48),
        2: (54, 58, 90),
        3: (92, 98, 138),
        4: (142, 150, 178),
        5: (198, 206, 226),
        7: (120, 220, 255),
    },
    # Crag Lynx: the tiger's orange becomes rock dust; the stripe stays dark,
    # which on grey reads as a lynx and not as a pale tiger.
    "crag_lynx": {
        4: (150, 132, 108),
        5: (184, 166, 140),
        7: (230, 224, 212),
        9: (214, 202, 180),
    },
    # Cave Bear: near-black fur, a dull muzzle instead of the gold one.
    "cave_bear": {
        2: (26, 24, 26),
        3: (46, 42, 44),
        4: (72, 66, 66),
        5: (102, 94, 90),
        9: (160, 142, 114),
    },
}

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


def monta(saida, prefixo, orcamento, pelagem=None, pecas_paleta=None, pintar=None):
    pecas, paleta = pecas_paleta or carrega(prefixo)
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
    # Painting by position (the robots' seams) happens on the REDUCED voxels:
    # a one-voxel line drawn before the reduction is averaged away.
    if pintar:
        finais = [(n, pintar(n, v)) for n, v in finais]

    # a tela do molde vira a deste bicho
    molde_corpo.W, molde_corpo.D, molde_corpo.H = tam
    molde_corpo.TRANSL = f"0 0 {tam[2] // 2}"
    cores = {i: tuple(paleta[i][:3]) for i in range(1, 256)}
    # A troca de pelagem e' aqui, na PALETA, e nao no vertice: o que nao esta'
    # no dicionario (nariz, boca, garra, listra) sai exatamente como veio.
    for i, rgb in (pelagem or {}).items():
        cores[i] = rgb
    with open(saida, "wb") as f:
        f.write(molde_corpo.arquivo_cena(finais, cores, camada=prefixo))
    print(f"{os.path.basename(saida)}: fator {fator}, {total} faces, tela {tam[0]}x{tam[1]}x{tam[2]}, "
          + ", ".join(f"{n}={len(v)}" for n, v in finais))


# ── MOUNT SKINS: a coat for every mount species ──
#
# `shared::aparencia::SKINS_DE_MONTARIA`. Same rule as `PELAGENS`: swap the
# FUR indices and leave eyes, nose, mouth, claws and horns alone — never a
# tint over the whole mesh. Here one coat has to fit five species, so each
# species names its roles (fur ramp, secondary, stripe) and each coat gives
# two colours per role: the role's indices are ordered by how light they are
# in the original and spread along the coat's ramp, so the model keeps its
# own shading.
#
# Indices read from the zone14 models (voxel counts in parentheses):
#   deer        3,4 brown fur · 8 cream belly/antler · 10 antler tips
#   wolf        1,2,3,4 slate fur · 6 pale chest
#   tiger       4,5 orange fur · 7 white belly/face · 1 the stripe
#   hippogriff  1,2,3 feather body · 4,11 white head feathers
#   dragon      1,2,3,4 red scales · 5,6,7,12 gold belly
PAPEIS_DE_MONTARIA = {
    "cervo":     {"pelo": [3, 4], "detalhe": [8, 10]},
    "lobo":      {"pelo": [1, 2, 3, 4], "detalhe": [6]},
    "tigre":     {"pelo": [4, 5], "detalhe": [7], "listra": [1]},
    "hipogrifo": {"pelo": [1, 2, 3], "detalhe": [4, 11]},
    "dragao":    {"pelo": [1, 2, 3, 4], "detalhe": [5, 6, 7, 12]},
}
# (sufixo) -> role -> (dark, light). Order and names match the Rust table.
PELES_DE_MONTARIA = {
    "dourada":   {"pelo": ((122, 84, 22), (250, 212, 96)), "detalhe": ((236, 226, 196), (255, 250, 236)),
                  "listra": ((96, 62, 18), (96, 62, 18))},
    "obsidiana": {"pelo": ((14, 12, 18), (66, 60, 80)), "detalhe": ((120, 60, 200), (196, 136, 255)),
                  "listra": ((150, 80, 230), (150, 80, 230))},
    "gelida":    {"pelo": ((104, 140, 186), (232, 244, 255)), "detalhe": ((70, 160, 230), (170, 226, 255)),
                  "listra": ((40, 90, 160), (40, 90, 160))},
    "brasa":     {"pelo": ((96, 20, 10), (246, 100, 30)), "detalhe": ((255, 170, 40), (255, 228, 120)),
                  "listra": ((30, 10, 8), (30, 10, 8))},
    "espectral": {"pelo": ((26, 80, 84), (150, 244, 222)), "detalhe": ((206, 255, 248), (255, 255, 255)),
                  "listra": ((12, 50, 62), (12, 50, 62))},
}


def pelagem_da_skin(paleta, papeis, pele):
    """The palette swap of one coat on one species."""
    troca = {}
    for papel, indices in papeis.items():
        escuro, claro = pele[papel]
        lum = lambda i: sum(c * w for c, w in zip(paleta[i][:3], (0.3, 0.59, 0.11)))
        ordem = sorted(indices, key=lum)
        for k, i in enumerate(ordem):
            t = k / (len(ordem) - 1) if len(ordem) > 1 else 0.6
            troca[i] = tuple(round(a + (b - a) * t) for a, b in zip(escuro, claro))
    return troca


# ── WINGED (Skyreach): the hippogriff's wings on another species ──
#
# The owner wants Skyreach's mobs angelic, "even the mobs need some wings".
# The rig already flaps any piece named `asa_d`/`asa_e` (`client::bicho`,
# `Junta::Asa`), so a winged creature is the species' own pieces plus the
# hippogriff's two wings, seated on its back at the same RELATIVE place
# (outside the body's side, from its top up) and scaled by body length.
#
# The wings use the hippogriff's palette indices, which mean other colours in
# the target's palette, so they are recoloured into four free slots: white
# and pale feathers, gold at the darkest (the hippogriff's orange) indices.

class _Modelo:
    def __init__(self, voxels):
        self.voxels = voxels


def _caixa(voxels):
    ks = list(voxels)
    return [min(k[i] for k in ks) for i in range(3)], [max(k[i] for k in ks) for i in range(3)]


ASA_CORES = [(206, 214, 230), (230, 236, 246), (250, 251, 255), (236, 196, 92)]


def alado(prefixo):
    """The species' pieces and palette, with white-and-gold wings added."""
    pecas, paleta = carrega(prefixo)
    hip, hpal = carrega("hippogriff")
    hip = dict(hip)
    corpo = dict(pecas)["tronco"].voxels
    (tlo, thi), (hlo, hhi) = _caixa(corpo), _caixa(hip["tronco"].voxels)
    s = (thi[1] - tlo[1]) / (hhi[1] - hlo[1])
    usados = {c for _, m in pecas for c in m.voxels.values()}
    livres = [i for i in range(250, 100, -1) if i not in usados][:4]
    # Hippogriff wing indices by how light they are; the orange ones (5, 6,
    # 10 in its palette) become gold.
    lum = lambda i: sum(c * w for c, w in zip(hpal[i][:3], (0.3, 0.59, 0.11)))
    def cor(i):
        r, g, b = hpal[i][:3]
        if r > 200 and g < 200 and b < 120:
            return livres[3]
        l = lum(i)
        return livres[0] if l < 110 else livres[1] if l < 200 else livres[2]
    for lado in ("asa_d", "asa_e"):
        asa = hip[lado].voxels
        alo, ahi = _caixa(asa)
        direita = lado == "asa_d"
        # The target box of the scaled wing, then each target voxel maps back
        # to the nearest source voxel (no holes when scaling up).
        # The wing root sits at the hippogriff's own spacing from the centre
        # line, scaled — capped at the body's half width. Seated at the outer
        # edge, a wide body (the owlbear) held its wings far out in the air.
        tcx, hcx = (tlo[0] + thi[0]) / 2, (hlo[0] + hhi[0]) / 2
        base = min((thi[0] - tlo[0]) / 2, (hhi[0] - hlo[0]) / 2 * s)
        hbase = (hhi[0] - hlo[0]) / 2
        def para_alvo(x, y, z):
            rx = (x - hcx - hbase) if direita else (hcx - hbase - x)
            nx = tcx + base + rx * s if direita else tcx - base - rx * s
            return nx, tlo[1] + (y - hlo[1]) * s, thi[2] + (z - hhi[2]) * s
        cantos = [para_alvo(x, y, z) for x in (alo[0], ahi[0]) for y in (alo[1], ahi[1]) for z in (alo[2], ahi[2])]
        mn = [int(min(c[i] for c in cantos)) for i in range(3)]
        mx = [int(max(c[i] for c in cantos)) + 1 for i in range(3)]
        nova = {}
        for X in range(mn[0], mx[0] + 1):
            rx = (X - tcx - base) / s if direita else (tcx - base - X) / s
            x = round(hcx + hbase + rx) if direita else round(hcx - hbase - rx)
            for Y in range(mn[1], mx[1] + 1):
                y = round(hlo[1] + (Y - tlo[1]) / s)
                for Z in range(mn[2], mx[2] + 1):
                    z = round(hhi[2] + (Z - thi[2]) / s)
                    c = asa.get((x, y, z))
                    if c:
                        nova[(X, Y, Z)] = cor(c)
        pecas.append((lado, _Modelo(nova)))
    paleta = list(paleta)
    for i, rgb in zip(livres, ASA_CORES):
        paleta[i] = (*rgb, 255)
    return pecas, paleta


# (file, species prefix, face budget, coat roles). The coat is "seraph":
# ivory to white fur, gold details.
BICHOS_ALADOS = [
    ("seraph_wolf", "wolf", 5200, {"pelo": [1, 2, 3, 4], "detalhe": [6]}),
    ("seraph_lynx", "tiger", 4200, {"pelo": [4, 5], "detalhe": [7], "listra": [1]}),
    ("seraph_bear", "bear", 4600, {"pelo": [2, 3, 4, 5], "detalhe": [9]}),
    ("seraph_owlbear", "owlbear", 5200, {"pelo": [1, 2, 3], "detalhe": [4, 5]}),
    ("pegasus_stag", "deer", 4200, {"pelo": [3, 4], "detalhe": [8, 10]}),
]
PELE_SERAFICA = {"pelo": ((214, 206, 192), (252, 250, 244)), "detalhe": ((222, 172, 66), (250, 218, 128)),
                 "listra": ((204, 154, 56), (204, 154, 56))}


# ── ROBOTS (Kōgen-tō): the species' mesh in gunmetal, plated and lit ──
#
# A grey animal is not a robot. Three things make it read as a machine: the
# coat becomes a steel ramp (by the same palette roles as the mount skins),
# PLATE SEAMS cut the body every few voxels in a dark line, and NEON strips
# run along its sides; the eyes glow. The seams and strips are painted by
# voxel position, in free palette slots, so the geometry stays the species'.
ROBO_PELE = {"pelo": ((52, 56, 66), (176, 184, 198)), "detalhe": ((40, 200, 240), (120, 240, 255)),
             "listra": ((30, 32, 40), (30, 32, 40))}
ROBO_CORES = [(24, 26, 32), (60, 230, 255), (255, 60, 170)]  # seam, neon, eye


def robo(prefixo, papeis, olhos):
    """The species' pieces and palette, as a robot."""
    pecas, paleta = carrega(prefixo)
    usados = {c for _, m in pecas for c in m.voxels.values()}
    livres = [i for i in range(250, 100, -1) if i not in usados][:3]
    paleta = list(paleta)
    troca = pelagem_da_skin(paleta, papeis, ROBO_PELE)
    for i, rgb in troca.items():
        paleta[i] = (*rgb, 255)
    for i in olhos:
        paleta[i] = (*ROBO_CORES[2], 255)
    # What is left of flesh — red mouths, pink noses, warm muzzles — would
    # read as an animal: reds glow pink, warm skin turns to dark metal.
    papel = {i for v in papeis.values() for i in v} | set(olhos)
    for i in usados - papel:
        r, g, b = paleta[i][:3]
        if r > 140 and g < 110:
            paleta[i] = (*ROBO_CORES[2], 255)
        elif r > g + 20 and r > b + 30:
            paleta[i] = (70, 74, 86, 255)
    for i, rgb in zip(livres, ROBO_CORES):
        paleta[i] = (*rgb, 255)
    costura, neon = livres[0], livres[1]
    pelo = set(papeis.get("pelo", []))
    vizinhos = ((1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1))

    def pintar(nome, vs):
        if nome not in ("tronco", "cabeca", "pescoco", "cauda"):
            return vs
        vs = dict(vs)
        xs = [k[0] for k in vs]
        meio, largura = (min(xs) + max(xs)) / 2, max(xs) - min(xs)
        for (x, y, z), c in list(vs.items()):
            if c not in pelo or all((x + d[0], y + d[1], z + d[2]) in vs for d in vizinhos):
                continue
            if y % 4 == 0:
                vs[(x, y, z)] = costura   # a plate seam across the body
            elif nome == "tronco" and abs(x - meio) >= largura * 0.38 and z % 3 == 1:
                vs[(x, y, z)] = neon      # neon strips on the flanks
        return vs
    return pecas, paleta, pintar


# (file, species prefix, face budget, coat roles, eye indices)
BICHOS_ROBO = [
    ("mech_hound", "wolf", 3200, {"pelo": [1, 2, 3, 4], "detalhe": [6]}, [5]),
    ("volt_panther", "tiger", 3200, {"pelo": [4, 5], "detalhe": [7], "listra": [1]}, [3]),
    ("iron_bear", "bear", 3200, {"pelo": [2, 3, 4, 5], "detalhe": [9]}, [6]),
    ("dynamo_owlbear", "owlbear", 3400, {"pelo": [1, 2, 3], "detalhe": [4, 5]}, [7]),
]


if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    pasta = os.path.join(raiz, "assets", "vox", "bichos")
    os.makedirs(pasta, exist_ok=True)
    # `--robos`: only Kōgen-tō's robots.
    if "--robos" in sys.argv:
        for nome, prefixo, orcamento, papeis, olhos in BICHOS_ROBO:
            pecas, paleta, pintar = robo(prefixo, papeis, olhos)
            monta(os.path.join(pasta, f"{nome}.vox"), prefixo, orcamento, None, (pecas, paleta), pintar)
        sys.exit(0)
    # `--alados`: only Skyreach's winged creatures.
    if "--alados" in sys.argv:
        for nome, prefixo, orcamento, papeis in BICHOS_ALADOS:
            pecas, paleta = alado(prefixo)
            monta(os.path.join(pasta, f"{nome}.vox"), prefixo, orcamento,
                  pelagem_da_skin(paleta, papeis, PELE_SERAFICA), (pecas, paleta))
        sys.exit(0)
    # `--skins`: only the mount coats, leaving every species file untouched.
    so_skins = "--skins" in sys.argv
    for nome, prefixo, orcamento in ([] if so_skins else BICHOS):
        monta(os.path.join(pasta, f"{nome}.vox"), prefixo, orcamento,
              PELAGENS.get(nome))
    for nome, prefixo, orcamento in BICHOS:
        papeis = PAPEIS_DE_MONTARIA.get(nome)
        if not papeis:
            continue
        _, paleta = carrega(prefixo)
        for sufixo, pele in PELES_DE_MONTARIA.items():
            monta(os.path.join(pasta, f"{nome}_{sufixo}.vox"), prefixo, orcamento,
                  pelagem_da_skin(paleta, papeis, pele))
