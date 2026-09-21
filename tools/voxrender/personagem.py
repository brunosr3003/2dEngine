#!/usr/bin/env python3
"""ROSTOS, CABELOS e CHAPEUS do jogador, cada um no seu arquivo.

O `npcs.py` monta a cabeca inteira numa peca so': pele, olhos, cabelo, barba e
chapeu saem fundidos em `cabeca`. Isso serve pro NPC, que nunca muda. Nao serve
pro jogador, que escolhe cada parte.

Aqui a cabeca sai em TRES arquivos que o cliente empilha (`render3d::Vestimenta`):

    rostos/rosto_NN.vox    objeto `cabeca`  — substitui a cabeca do corpo
    cabelos/cabelo_NN.vox  objeto `cabelo`  — vai POR CIMA, na junta da cabeca
    chapeus/<nome>.vox     objeto `cabelo`  — o mesmo slot; chapeu ganha

# A regra que faz isso funcionar

**O rosto nao tem couro cabeludo.** Se `rosto_02.vox` trouxesse cabeleira
junto, escolher `cabelo_03` poria DOIS cabelos na cabeca. O rosto e' pele,
olhos, nariz, boca, orelha e (no maximo) barba; o cabelo inteiro mora no
arquivo de cabelo.

**A cor nao esta' aqui.** O rosto escreve nos indices 249-252 e o cabelo nos
245-248 — as faixas que o SHADER tinge (`vox::faixa_de`). O que este gerador
escreve na paleta e' so' rascunho: quem manda na cor final e' o tom de pele e
a cor de cabelo que o jogador escolheu. E' por isso que ha' 3 rostos e nao 12.

Uso:  python3 tools/voxrender/personagem.py
"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import molde_corpo as M        # noqa: E402
import npcs as N               # noqa: E402

RAIZ = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
DEST = os.path.join(RAIZ, "assets", "vox", "personagem")
PELE, CAB, F = M.PELE, M.CABELO, N.F
cx = N.cx


def rosto(o):
    """So' a cabeca: pele, olhos, nariz, boca, orelha e barba. SEM cabelo."""
    h = cx(14, 17, 10, 13, 33, 33, PELE[2])            # pescoco
    h.update(cx(12, 19, 8, 15, 34, 41, PELE[1]))       # a cabeca, 8x8x8
    # olhos
    h[(13, F, 38)] = N.OLHO_BR; h[(14, F, 38)] = N.OLHO
    h[(17, F, 38)] = N.OLHO;    h[(18, F, 38)] = N.OLHO_BR
    # sobrancelha: entra na faixa do CABELO, pra acompanhar a cor escolhida
    for x in (13, 14, 17, 18): h[(x, F, 39)] = CAB[2]
    h[(15, 16, 37)] = PELE[1]; h[(16, 16, 37)] = PELE[1]   # nariz
    h[(15, F, 35)] = N.BOCA; h[(16, F, 35)] = N.BOCA
    for y in (11, 12):                                      # orelhas
        for z in (37, 38): h[(12, y, z)] = PELE[2]; h[(19, y, z)] = PELE[2]
    if o.get("rugas"):
        for (x, z) in ((13, 37), (18, 37), (14, 40), (17, 40)): h[(x, F, z)] = PELE[2]
    if o.get("barba"):
        for x in range(13, 19): h[(x, F, 36)] = CAB[1]      # bigode
        h[(15, F, 36)] = PELE[2]; h[(16, F, 36)] = PELE[2]
        for x in range(12, 20): h[(x, F, 34)] = CAB[1]; h[(x, F, 35)] = CAB[1]
        h[(15, F, 35)] = N.BOCA; h[(16, F, 35)] = N.BOCA
        for y in range(11, 15):
            for z in (34, 35, 36): h[(12, y, z)] = CAB[1]; h[(19, y, z)] = CAB[1]
    elif o.get("bigode"):
        for x in range(13, 19): h[(x, F, 36)] = CAB[1]
        h[(12, F, 35)] = CAB[1]; h[(19, F, 35)] = CAB[1]
    return h


def cabelo(o):
    """So' o cabelo — uma CASCA UM VOXEL FORA do cranio.

    A primeira versao escrevia o cabelo NA superficie da cabeca (topo em
    z=41, nuca em y=8, laterais em x=12/19) — que sao exatamente as camadas
    que a cabeca ja' ocupa. Duas malhas no mesmo plano: a placa de video nao
    tem como decidir qual fica na frente, e o dono viu o cabelo piscando
    entre a cor do cabelo e a da pele.

    A regra que conserta, e que o `docs/ARTE_DO_PERSONAGEM.md` ja' pedia pros
    chapeus: **a casca fica um voxel mais larga que a cabeca em toda volta,
    pra as faces dela nunca pousarem nas faces da cabeca.**

    Cabeca: x 12-19, y 8-15, z 34-41. A casca: x 11/20, y 7, z 42.
    """
    c = {}
    if o.get("careca"):
        # Careca nao e' ausencia de cabelo: e' uma coroa baixa, por fora.
        c.update(cx(11, 20, 7, 7, 35, 38, CAB[2]))
        for z in range(35, 39):
            c[(11, 8, z)] = CAB[2]; c[(20, 8, z)] = CAB[2]
        return c
    # topo: uma camada ACIMA do crânio
    c.update(cx(11, 20, 7, 16, 42, 42, CAB[1]))
    # nuca: uma camada ATRAS
    c.update(cx(11, 20, 7, 7, 35, 42, CAB[1]))
    # laterais: uma coluna de cada lado, por fora
    for y in range(7, 14):
        c[(11, y, 41)] = CAB[1]; c[(20, y, 41)] = CAB[1]
    for y in range(7, 12):
        for z in (38, 39, 40):
            c[(11, y, z)] = CAB[2]; c[(20, y, z)] = CAB[2]
    if o.get("coque"):
        # Atras da nuca, mais pra fora ainda — nao encosta no cranio.
        c.update(cx(14, 17, 5, 6, 39, 42, CAB[1]))
        c.update(cx(15, 16, 4, 4, 40, 41, CAB[2]))
    if o.get("trancas"):
        for z in range(30, 39):
            c[(11, 8, z)] = CAB[2]; c[(20, 8, z)] = CAB[2]
    return c


def sem_cabeca(pecas, cabeca):
    """Tira da peca todo voxel que a CABECA ja' ocupa.

    A cabeca e' macica, entao voxel de chapeu dentro dela e' invisivel — mas
    NAO e' inofensivo: quando os dois estao na mesma malha nao ha' conflito,
    e quando estao em malhas separadas (que e' o caso agora) as faces
    coincidentes brigam. Tirar e' de graca visualmente e acaba com a briga.

    Os chapeus vem do `npcs.py`, onde cabeca e chapeu saiam no MESMO dicionario
    e um simplesmente sobrescrevia o outro. Separados, eles precisam disto.
    """
    return {k: v for k, v in pecas.items() if k not in cabeca}


# (arquivo, opcoes). Tres de cada, como o dono pediu — "tres variacoes mais
# simples de graca, depois criamos coisas mais legais pagas".
ROSTOS = [
    ("rosto_01", {}),                          # liso
    ("rosto_02", {"barba": True}),             # barbado
    ("rosto_03", {"rugas": True, "bigode": True}),  # veterano
]
CABELOS = [
    ("cabelo_01", {}),                # solto
    ("cabelo_02", {"coque": True}),   # coque
    ("cabelo_03", {"trancas": True}), # trancas
]


def grava(pasta, nome, objeto, voxels, paleta):
    if not voxels:
        raise SystemExit(f"{nome}: vazio — o gerador nao escreveu voxel nenhum")
    fora = [p for p in voxels if not (0 <= p[0] < M.W and 0 <= p[1] < M.D and 0 <= p[2] < M.H)]
    if fora:
        raise SystemExit(f"{nome}: {len(fora)} voxels fora da tela {M.W}x{M.D}x{M.H}: {fora[:3]}")
    d = os.path.join(DEST, pasta)
    os.makedirs(d, exist_ok=True)
    caminho = os.path.join(d, f"{nome}.vox")
    with open(caminho, "wb") as f:
        f.write(M.arquivo_cena([(objeto, voxels)], paleta=paleta, camada=objeto))
    print(f"{pasta}/{nome}.vox: {len(voxels)} voxels, objeto '{objeto}'")


# ── as ROUPAS ────────────────────────────────────────────────────────────
#
# Uma skin de roupa sao as DEZ pecas do corpo: cada uma SUBSTITUI a do
# piratinha (`render3d::Vestimenta`). Sai do `npcs.py`, que ja' monta torso,
# bracos e pernas com casaco, colete, ombreira e bota — e' o mesmo corpo, com
# outra roupa.
#
# A pele (maos, pescoco) fica na faixa 249-252 pra o tom escolhido continuar
# valendo: uma roupa que assasse a cor da mao daria mao branca em personagem
# de pele escura.
ROUPAS = [
    ("aventureiro", {"colete": True}),
    ("mercenario", {"casaco": True, "ombreiras": True}),
    ("andarilho", {"tunica": True, "capa": True}),
    # A paga: casaco, ombreiras, peitoral e capa — a mais carregada das quatro.
    ("capitao", {"casaco": True, "ombreiras": True, "peitoral": True, "capa": True}),
]


def roupa(o):
    """As dez pecas, com a roupa por cima. So' o que o rig desenha."""
    pecas = {
        "torso": N.torso(o),
        "braco_d": N.braco(20, 23, o), "antebraco_d": N.antebraco(20, 23, o, "d"),
        "braco_e": N.braco(8, 11, o),  "antebraco_e": N.antebraco(8, 11, o, "e"),
        "coxa_d": N.coxa(17, 20, o),   "canela_d": N.canela(17, 20, o),
        "coxa_e": N.coxa(11, 14, o),   "canela_e": N.canela(11, 14, o),
    }
    return {k: v for k, v in pecas.items() if v}


def confere_sem_sobreposicao(cabecas, pecas, nome):
    for rn, cab in cabecas.items():
        ov = set(pecas) & set(cab)
        if ov:
            raise SystemExit(
                f"{nome} sobrepoe {len(ov)} voxels de {rn} (ex.: {sorted(ov)[:3]}) — "
                "isso pisca na tela; a casca tem que ficar um voxel por fora"
            )


def main():
    # A paleta e' RASCUNHO: o shader troca 245-252 no desenho. Mas ela tem que
    # existir e ser legivel, pra o arquivo abrir no MagicaVoxel pra retoque.
    p = N.paleta()
    cabecas = {}
    for nome, o in ROSTOS:
        v = rosto(o)
        cabecas[nome] = v
        grava("rostos", nome, "cabeca", v, p)
    for nome, o in CABELOS:
        v = cabelo(o)
        confere_sem_sobreposicao(cabecas, v, f"cabelos/{nome}")
        grava("cabelos", nome, "cabelo", v, p)
    # Os chapeus ja' estao prontos no `npcs.py` — oito formas, cor propria.
    # Os chapeus ja' existem no `npcs.py`, mas foram desenhados pra sair na
    # MESMA malha da cabeca, onde sobrepor nao custa nada. Separados, o que
    # afunda no cranio tem que sair.
    uma_cabeca = next(iter(cabecas.values()))
    for nome, fn in N.CHAPEUS.items():
        v = sem_cabeca(fn(), uma_cabeca)
        confere_sem_sobreposicao(cabecas, v, f"chapeus/{nome}")
        grava("chapeus", nome, "cabelo", v, p)
    # As roupas: um arquivo com as NOVE pecas do corpo (a cabeca vem do rosto).
    for nome, o in ROUPAS:
        pecas = roupa(o)
        for k, v in pecas.items():
            fora = [q for q in v if not (0 <= q[0] < M.W and 0 <= q[1] < M.D and 0 <= q[2] < M.H)]
            if fora:
                raise SystemExit(f"{nome}/{k}: {len(fora)} voxels fora da tela")
        d = os.path.join(DEST, "skins")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, f"{nome}.vox"), "wb") as f:
            f.write(M.arquivo_cena(list(pecas.items()), paleta=p, camada="corpo"))
        print(f"skins/{nome}.vox: {len(pecas)} pecas, {sum(len(v) for v in pecas.values())} voxels")


if __name__ == "__main__":
    main()
