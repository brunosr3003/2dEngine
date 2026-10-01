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

def pecas(cabeca):
    return [
        ("cabeca", cabeca), ("torso", P.torso()),
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
}

if __name__ == "__main__":
    raiz = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    rig = os.path.join(raiz, "assets", "vox", "humanoides"); os.makedirs(rig, exist_ok=True)
    for nome, (pal, cab, arma) in MOBS.items():
        ps = pecas(cab())
        open(os.path.join(rig, f"{nome}.vox"), "wb").write(M.arquivo_cena(ps, pal, camada=nome))
        plano = ps + [("arma", arma())]
        open(os.path.join(raiz, "assets", "vox", f"{nome}.vox"), "wb").write(M.arquivo_plano(plano, pal))
        print(f"{nome:11} rig + plano  ({sum(len(v) for _, v in plano)} voxels)")
