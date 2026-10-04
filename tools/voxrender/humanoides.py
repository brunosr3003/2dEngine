#!/usr/bin/env python3
"""Os mobs HUMANOIDES: pistoleiro, mago e arqueiro.

Mesmo corpo e mesma geometria do piratinha (`pirata.py`) — a regra e' "quem
atira e' gente", e gente usa o rig do jogador. O que muda e' a PALETA (a
mesma peca pintada de outra cor), a cabeca (sem tapa-olho, com bandana ou
capuz) e a arma.

Cada um sai em dois arquivos:

- `assets/vox/humanoides/<nome>.vox` — as dez pecas nomeadas, sem a arma
  (carregadas pelo cliente no rig; arma no encaixe da mao);
- `assets/vox/<nome>.vox` — UMA peca so', parada, com a arma na mao. E' o que
  o fallback do cliente se as pecas nao estiverem disponiveis.

Uso:  python3 tools/voxrender/humanoides.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M
import pirata as P

cx = M.caixa
F = 15          # face da frente
ARMA_METAL, ARMA_MADEIRA, CORDA = 13, 14, 15

def cabeca_sem_tapa():
    h = P.cabeca()
    for x in (17, 18):
        for z in (37, 38): h[(x, F, z)] = P.PELE[1]
    h[(17, F, 38)] = P.OLHO_AZUL; h[(18, F, 38)] = P.BRANCO     # o outro olho
    h[(16, F, 39)] = P.PELE[1]; h[(15, F, 40)] = P.PELE[1]      # sem a tira
    h[(17, F, 39)] = P.CAB[2]; h[(18, F, 39)] = P.CAB[2]         # sobrancelha
    return h

def capuz(cor):
    """Capuz: casca um voxel mais larga que a cabeca, aberta na frente do rosto."""
    c = {}
    for (x, y, z), _ in cx(11, 20, 7, 16, 36, 42, cor).items():
        dentro = 12 <= x <= 19 and 8 <= y <= 15 and z <= 41
        rosto = 13 <= x <= 18 and y >= 15 and 36 <= z <= 40
        if not dentro and not rosto: c[(x, y, z)] = cor
    for z in range(30, 36):                                        # cai nas costas
        for x in range(12, 20): c[(x, 7, z)] = cor
    return c

def bandana(cor):
    b = {p: cor for p in cx(11, 20, 7, 16, 39, 40, cor)}
    for z in (36, 37, 38): b[(15, 6, z)] = cor; b[(16, 6, z)] = cor   # no' atras
    return b

def pistola():
    """Na mao direita (x 21-24), cano pra frente."""
    a = cx(22, 23, 14, 19, 18, 18, ARMA_METAL)
    a.update(cx(22, 23, 12, 13, 16, 18, ARMA_MADEIRA))          # cabo dentro da mao
    a[(22, 14, 19)] = ARMA_METAL                                   # cao
    return a

def arco():
    """Na mao esquerda (x 7-10), em pe', na frente da mao."""
    a = {}
    for z in range(10, 31):
        d = abs(z - 20)
        y = 16 - (0 if d < 5 else 1 if d < 8 else 2)
        a[(8, y, z)] = ARMA_MADEIRA; a[(9, y, z)] = ARMA_MADEIRA
    for z in range(11, 30): a[(8, 13, z)] = CORDA                   # corda
    return a

# ── Skyreach: wings and halo (the owner: "even the mobs need some wings") ──
ASA, ASA_SOMBRA, ASA_OURO, HALO = 40, 41, 42, 43
CORES_CELESTES = {ASA: (250, 251, 255), ASA_SOMBRA: (214, 222, 238), ASA_OURO: (236, 196, 92),
                  HALO: (255, 226, 120)}


def asas():
    """Folded wings on the back, in the torso piece so they move with the
    body: from the shoulder blades (behind the torso, y 6-7) out to each side
    and up past the head, the lower edge stepping down in feathers."""
    a = {}
    for lado in (-1, 1):
        raiz = 15.5 + lado * 2.5
        for k in range(0, 13):
            x = int(raiz + lado * k)
            if not (0 <= x <= 31):
                continue
            # The upper edge rises to a peak two thirds out, then falls to the
            # tip; the lower edge comes up in steps — the primaries.
            topo = 32 + min(k, 8) * 1.5 - max(0, k - 8) * 1.2
            base = 26 - k * 0.5 if k < 9 else 22 + (k - 9) * 2.5
            for z in range(int(base), int(topo) + 1):
                c = ASA_OURO if z == int(topo) else (ASA_SOMBRA if (z - int(base)) % 4 == 0 else ASA)
                a[(x, 7, z)] = c
                if k < 7:
                    a[(x, 6, z)] = ASA_SOMBRA
    return a


def halo():
    """A gold ring floating above the head (head top is z 41)."""
    return {(x, y, 45): HALO for x in range(12, 20) for y in range(8, 16)
            if x in (12, 19) or y in (8, 15)}


def pecas(cabeca, aladas=False):
    torso = P.torso()
    if aladas:
        torso.update(asas())
    return [
        ("cabeca", cabeca), ("torso", torso),
        ("braco_d", P.braco(21, 24)), ("antebraco_d", P.antebraco(21, 24)),
        ("braco_e", P.braco(7, 10)),  ("antebraco_e", P.antebraco(7, 10)),
        ("coxa_d", P.coxa(16, 19)),   ("canela_d", P.canela(16, 19)),
        ("coxa_e", P.coxa(12, 15)),   ("canela_e", P.canela(12, 15)),
    ]

def paleta(**troca):
    p = dict(P.paleta)
    p.update({ARMA_METAL: (96, 96, 104), ARMA_MADEIRA: (110, 70, 36), CORDA: (225, 220, 200)})
    for nome, cor in troca.items():
        p[getattr(P, nome) if not nome.startswith("CAB") else P.CAB[int(nome[3])]] = cor
    return p

MOBS = {
    # Ranger: bandido de pistola. Couro e bandana vermelha.
    "pistoleiro": (
        paleta(BRANCO=(196, 164, 116), CINZA=(150, 118, 78), PRETO=(72, 52, 36),
               CARVAO=(56, 40, 28), BOTA=(44, 30, 20), FAIXA=(176, 32, 32)),
        lambda: {**cabeca_sem_tapa(), **bandana(P.FAIXA)}, pistola),
    # Mago: capuz e manto roxos, barba branca. O circulo no braco e' efeito de
    # codigo, nao modelo.
    "mago": (
        paleta(BRANCO=(112, 62, 164), CINZA=(84, 44, 128), PRETO=(64, 32, 96),
               CARVAO=(52, 26, 80), BOTA=(48, 30, 64), FAIXA=(222, 180, 60),
               CAB0=(222, 222, 226), CAB1=(200, 200, 206), CAB2=(170, 170, 178), CAB3=(140, 140, 150)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, dict),
    # Arqueiro: capuz verde, tunica verde, arco na mao esquerda.
    "arqueiro": (
        paleta(BRANCO=(92, 134, 70), CINZA=(66, 102, 52), PRETO=(84, 62, 40),
               CARVAO=(66, 48, 30), BOTA=(60, 40, 24), FAIXA=(124, 84, 40)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, arco),
    # ── Island variants (`shared::bestiary`) ──
    # Same body, same weapon as their species; the outfit says the island.
    # Glacier: fur-lined white and ice blue, pale wool hood.
    "frost_archer": (
        paleta(BRANCO=(214, 226, 238), CINZA=(150, 176, 204), PRETO=(92, 104, 122),
               CARVAO=(70, 80, 96), BOTA=(58, 64, 76), FAIXA=(96, 150, 210)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.BRANCO)}, arco),
    "frost_mage": (
        paleta(BRANCO=(120, 170, 222), CINZA=(70, 118, 178), PRETO=(44, 76, 128),
               CARVAO=(34, 58, 100), BOTA=(40, 52, 76), FAIXA=(226, 240, 252),
               CAB0=(232, 236, 242), CAB1=(212, 218, 228), CAB2=(184, 192, 206), CAB3=(150, 160, 178)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, dict),
    # Waste: sand linen and sun-bleached leather.
    "dune_raider": (
        paleta(BRANCO=(222, 196, 146), CINZA=(186, 150, 98), PRETO=(120, 86, 50),
               CARVAO=(92, 64, 38), BOTA=(78, 54, 32), FAIXA=(214, 120, 40)),
        lambda: {**cabeca_sem_tapa(), **bandana(P.BRANCO)}, pistola),
    "sand_archer": (
        paleta(BRANCO=(206, 170, 110), CINZA=(168, 128, 76), PRETO=(110, 78, 46),
               CARVAO=(86, 60, 34), BOTA=(72, 50, 30), FAIXA=(150, 54, 40)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, arco),
    "sun_mage": (
        paleta(BRANCO=(222, 150, 50), CINZA=(186, 96, 34), PRETO=(130, 56, 28),
               CARVAO=(100, 42, 22), BOTA=(84, 44, 26), FAIXA=(250, 214, 92),
               CAB0=(60, 44, 34), CAB1=(48, 36, 28), CAB2=(38, 28, 22), CAB3=(28, 20, 16)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, dict),
    # Plateau: slate grey and moss, storm violet for the mage.
    "cliff_archer": (
        paleta(BRANCO=(132, 140, 136), CINZA=(96, 106, 100), PRETO=(70, 66, 58),
               CARVAO=(54, 50, 44), BOTA=(46, 40, 34), FAIXA=(110, 142, 70)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, arco),
    "storm_mage": (
        paleta(BRANCO=(70, 74, 120), CINZA=(48, 50, 92), PRETO=(32, 32, 64),
               CARVAO=(24, 24, 50), BOTA=(30, 30, 46), FAIXA=(150, 220, 255),
               CAB0=(222, 222, 226), CAB1=(200, 200, 206), CAB2=(170, 170, 178), CAB3=(140, 140, 150)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, dict),
    # Skyreach: white and gold robes, a sky-blue sash, a halo; the wings come
    # from `ALADOS` below.
    "seraph_archer": (
        {**paleta(BRANCO=(242, 240, 232), CINZA=(212, 206, 194), PRETO=(226, 220, 204),
                  CARVAO=(196, 188, 170), BOTA=(214, 172, 70), FAIXA=(120, 180, 240)), **CORES_CELESTES},
        lambda: {**cabeca_sem_tapa(), **halo()}, arco),
    "seraph_mage": (
        {**paleta(BRANCO=(236, 240, 250), CINZA=(150, 190, 236), PRETO=(222, 226, 236),
                  CARVAO=(190, 196, 210), BOTA=(214, 172, 70), FAIXA=(255, 226, 120),
                  CAB0=(244, 236, 210), CAB1=(226, 214, 180), CAB2=(200, 186, 150), CAB3=(170, 156, 120)),
         **CORES_CELESTES},
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA), **halo()}, dict),
}

# ── Kōgen-tō: the robots ──
# Same rig and weapons; a gunmetal body, a helmet shell with a glowing visor
# and an antenna instead of a face, and neon where the sash was.
NEON, VISOR, ANTENA = 44, 45, 46
CORES_ROBO = {NEON: (60, 230, 255), VISOR: (255, 60, 170), ANTENA: (226, 230, 240)}


def capacete():
    """A robot head: a shell one voxel bigger than the head, closed in front,
    a visor band across the eyes and an antenna with a lit tip."""
    c = {}
    for (x, y, z), _ in cx(11, 20, 7, 16, 33, 42, 0).items():
        dentro = 12 <= x <= 19 and 8 <= y <= 15 and z <= 41
        if not dentro:
            c[(x, y, z)] = P.CARVAO
    for x in range(12, 20):
        for z in (37, 38):
            c[(x, 16, z)] = VISOR
    for z in range(43, 47):
        c[(16, 11, z)] = ANTENA
    c[(16, 11, 47)] = NEON
    return c


def paleta_robo(aco, escuro, faixa):
    p = paleta(BRANCO=aco, CINZA=escuro, PRETO=(36, 40, 50), CARVAO=(28, 30, 38), BOTA=(22, 24, 30), FAIXA=faixa)
    for i in P.PELE:
        p[i] = (150, 156, 168)  # hands and neck: bare metal
    p.update(CORES_ROBO)
    p[ARMA_METAL] = (70, 74, 86)
    p[ARMA_MADEIRA] = (40, 44, 54)
    p[CORDA] = (60, 230, 255)  # the energy bow's string
    return p


MOBS.update({
    "gunner_bot": (paleta_robo((120, 126, 140), (84, 90, 104), (255, 170, 40)), capacete, pistola),
    "laser_sentry": (paleta_robo((96, 104, 122), (66, 72, 88), (60, 230, 255)), capacete, arco),
    "tesla_unit": (paleta_robo((70, 78, 120), (48, 54, 92), (150, 120, 255)), capacete, dict),
    # Abyssia: sea-worn uniforms and luminous coral accents.
    "drowned_pirate": (
        paleta(BRANCO=(72, 114, 126), CINZA=(48, 82, 96), PRETO=(34, 58, 70),
               CARVAO=(26, 44, 56), BOTA=(28, 42, 48), FAIXA=(134, 208, 194)),
        lambda: {**cabeca_sem_tapa(), **bandana(P.FAIXA)}, pistola),
    "fishman_harpooner": (
        paleta(BRANCO=(76, 156, 158), CINZA=(44, 112, 126), PRETO=(30, 74, 92),
               CARVAO=(22, 56, 72), BOTA=(32, 74, 78), FAIXA=(248, 164, 116)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, arco),
    "merfolk_mage": (
        paleta(BRANCO=(124, 132, 190), CINZA=(74, 88, 152), PRETO=(42, 56, 110),
               CARVAO=(32, 40, 82), BOTA=(32, 48, 86), FAIXA=(98, 238, 218)),
        lambda: {**cabeca_sem_tapa(), **capuz(P.CINZA)}, dict),
})

# The robots' torso: a glowing core on the chest, plate seams, and shoulder
# plates that stand out over the arms.
ROBOS = {"gunner_bot", "laser_sentry", "tesla_unit"}


def torso_robo(torso):
    t = dict(torso)
    for (x, y, z), c in list(t.items()):
        if z in (22, 27) and y in (8, 14):
            t[(x, y, z)] = P.CARVAO          # plate seams round the body
    for x in (14, 15, 16, 17):
        for z in (27, 28, 29):
            t[(x, 15, z)] = NEON if 15 <= x <= 16 and z == 28 else P.CINZA  # the chest core
    for (x0, x1) in ((8, 11), (20, 23)):
        for x in range(x0, x1 + 1):
            for y in range(8, 15):
                t[(x, y, 33)] = P.CINZA       # shoulder plates
                t[(x, y, 32)] = P.CINZA if x in (x0, x1) or y in (8, 14) else t.get((x, y, 32), P.CINZA)
    return t


# Which mobs carry wings on the back.
ALADOS = {"seraph_archer", "seraph_mage"}

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    rig = os.path.join(raiz, "assets", "vox", "humanoides"); os.makedirs(rig, exist_ok=True)
    for nome, (pal, cab, arma) in MOBS.items():
        ps = pecas(cab(), nome in ALADOS)
        if nome in ROBOS:
            ps = [(n, torso_robo(v) if n == "torso" else v) for n, v in ps]
        open(os.path.join(rig, f"{nome}.vox"), "wb").write(M.arquivo_cena(ps, pal, camada=nome))
        plano = ps + [("arma", arma())]
        open(os.path.join(raiz, "assets", "vox", f"{nome}.vox"), "wb").write(M.arquivo_plano(plano, pal))
        print(f"{nome:11} rig + plano  ({sum(len(v) for _, v in plano)} voxels)")
