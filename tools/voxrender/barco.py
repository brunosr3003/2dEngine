#!/usr/bin/env python3
"""A CHALUPA do Mar Aberto (docs/MAR_ABERTO.md).

Gera `assets/vox/barcos/chalupa.vox`.

O primeiro barco do jogo foi um `.vox` trazido do zone14. O dono testou e
recusou: aquele arquivo e' um casco SEM mastro, sem vela e sem canhao — o que
ele lembrava de bom era a chalupa que o zone14 monta em CODIGO, bloco a
bloco, e que nunca virou arquivo. Entao esta e' a mesma ideia, escrita aqui.

A forma que faz ler como barco de vela sao tres curvas, e nenhuma delas e'
enfeite:

- **meia-boca**: a largura por posicao ao longo da quilha. Afunila pra proa em
  ponta e pra popa num painel reto, que e' o que um barco de verdade tem.
- **tosamento**: a amurada SOBE pras duas pontas. Sem isso o casco e' uma
  caixa, e nenhuma quantidade de detalhe conserta.
- **fundo**: a quilha desce no meio e sobe nas pontas.

O eixo do arquivo: X e' a boca (bombordo-boreste), Y e' o comprimento (proa em
+Y) e Z e' a altura. E' a convencao do MagicaVoxel que o carregador do Tempest
ja' espera — ele mede a altura em Z.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from voxsimplify import escrever_vox  # noqa: E402

# ── tamanho, em voxels ────────────────────────────────────────────────────
COMP = 96          # proa a popa
BOCA = 34          # bombordo a boreste
ALTO = 64

MEIO_X = BOCA // 2
MEIO_Y = COMP // 2

# ── paleta ────────────────────────────────────────────────────────────────
# Indices sao 1..N; o escritor cuida do resto.
CORES = [
    (54, 33, 20, 255),      # 1 quilha e trave escura
    (86, 54, 32, 255),      # 2 costado
    (104, 66, 39, 255),     # 3 costado claro (da' leitura de regua)
    (150, 96, 54, 255),     # 4 friso
    (181, 140, 92, 255),    # 5 conves
    (160, 118, 74, 255),    # 6 conves escuro
    (36, 24, 15, 255),      # 7 amurada
    (198, 152, 66, 255),    # 8 latao
    (188, 30, 30, 255),     # 9 listra vermelha da linha d'agua
    (232, 228, 214, 255),   # 10 vela
    (206, 200, 184, 255),   # 11 vela na sombra
    (44, 48, 56, 255),      # 12 ferro do canhao
    (120, 84, 46, 255),     # 13 mastro
]
QUILHA, COSTADO, COSTADO2, FRISO, CONVES, CONVES2 = 1, 2, 3, 4, 5, 6
AMURADA, LATAO, LISTRA, VELA, VELA2, FERRO, MASTRO = 7, 8, 9, 10, 11, 12, 13

V = {}


def poe(x, y, z, c):
    x, y, z = int(round(x)), int(round(y)), int(round(z))
    if 0 <= x < BOCA and 0 <= y < COMP and 0 <= z < ALTO:
        V[(x, y, z)] = c


def caixa(x0, y0, z0, x1, y1, z1, c):
    for z in range(int(z0), int(z1) + 1):
        for y in range(int(y0), int(y1) + 1):
            for x in range(int(x0), int(x1) + 1):
                poe(x, y, z, c)


def meia_boca(t):
    """Meia-largura em voxels, por `t` = -1 (popa) .. +1 (proa)."""
    if t >= 0:
        # Proa em ponta: cai rapido perto da ponta.
        return (MEIO_X - 1) * max(0.0, 1.0 - t ** 2.6)
    # Popa: afunila pouco e fecha num painel reto.
    return (MEIO_X - 1) * max(0.0, 1.0 - 0.45 * (-t) ** 2.2)


def fundo(t):
    """Altura da quilha: desce no meio, sobe nas pontas."""
    return 6 + 5.0 * abs(t) ** 2.0


def conves(t):
    return 22 + 2.5 * max(0.0, t) ** 2


def borda(t):
    """Topo da amurada. O TOSAMENTO e' esta subida pras pontas."""
    return conves(t) + 5 + 5.0 * abs(t) ** 2.4


def casco():
    for iy in range(COMP):
        t = (iy - MEIO_Y) / float(MEIO_Y)
        w = meia_boca(t)
        if w < 1.0:
            continue
        z_fundo, z_conves, z_borda = fundo(t), conves(t), borda(t)
        painel = iy <= 2  # as tres primeiras fatias fecham a popa
        for ix in range(BOCA):
            dx = ix - MEIO_X + 0.5
            if abs(dx) > w:
                continue
            # A CASCA e' o anel externo: o que tem vizinho fora do casco.
            fora = abs(dx) + 1 > w
            for iz in range(int(z_fundo), int(z_borda) + 1):
                if iz <= z_fundo + 1:
                    poe(ix, iy, iz, QUILHA)          # quilha
                elif iz < z_conves:
                    if fora or painel:
                        # Listra na linha d'agua, e regua alternada acima.
                        c = LISTRA if abs(iz - (z_fundo + 6)) <= 1 else (
                            COSTADO if (iz // 3) % 2 == 0 else COSTADO2)
                        poe(ix, iy, iz, c)
                elif iz == int(z_conves):
                    poe(ix, iy, iz, CONVES if (iy // 4) % 2 == 0 else CONVES2)
                elif fora or painel:
                    c = AMURADA if iz >= z_borda - 1 else FRISO
                    poe(ix, iy, iz, c)


def mastro_e_vela():
    t = 0.08
    iy = int(MEIO_Y + t * MEIO_Y)
    base = int(conves(t)) + 1
    topo = base + 34
    caixa(MEIO_X - 1, iy - 1, base, MEIO_X + 1, iy + 1, topo, MASTRO)
    # Verga e vela: a vela ENCHE pra um lado, senao ela e' uma placa.
    verga = topo - 4
    caixa(2, iy, verga, BOCA - 3, iy, verga, MASTRO)
    for iz in range(base + 9, verga):
        u = (iz - (base + 9)) / float(verga - (base + 9))
        larg = (MEIO_X - 3) * (0.55 + 0.45 * math.sin(math.pi * u))
        barriga = 2.5 * math.sin(math.pi * u)
        for ix in range(BOCA):
            dx = ix - MEIO_X + 0.5
            if abs(dx) > larg:
                continue
            c = VELA if abs(dx) < larg * 0.75 else VELA2
            poe(ix, iy + barriga, iz, c)
    # Estai da proa: a linha que amarra o mastro na ponta.
    for k in range(20):
        u = k / 19.0
        poe(MEIO_X, iy + u * (COMP - 6 - iy), topo - u * (topo - base - 6), MASTRO)


def canhoes():
    """Dois de cada bordo, saindo por portinholas."""
    for t in (-0.34, 0.12):
        iy = int(MEIO_Y + t * MEIO_Y)
        z = int(conves(t)) + 2
        w = meia_boca(t)
        for lado in (-1, 1):
            x_borda = MEIO_X + lado * w
            # Portinhola: abre o buraco na amurada.
            for dz in range(3):
                for dy in range(-2, 3):
                    V.pop((int(x_borda), iy + dy, z + dz), None)
                    V.pop((int(x_borda - lado), iy + dy, z + dz), None)
            # Carreta e cano.
            caixa(x_borda - lado * 4, iy - 1, z, x_borda - lado * 2, iy + 1, z + 1, MASTRO)
            for k in range(6):
                poe(x_borda - lado * 3 + lado * k, iy, z + 2, FERRO)
                poe(x_borda - lado * 3 + lado * k, iy, z + 3, FERRO)
            poe(x_borda - lado * 3, iy, z + 4, LATAO)


def leme_e_detalhes():
    # Cana do leme na popa.
    iy = 6
    z = int(conves(-0.87))
    caixa(MEIO_X - 1, iy, z + 1, MEIO_X + 1, iy + 3, z + 4, MASTRO)
    poe(MEIO_X, iy + 1, z + 5, LATAO)
    # Lanterna de popa.
    poe(MEIO_X, 4, int(borda(-0.92)) + 1, LATAO)
    # Gurupes: a lanca que sai da proa.
    for k in range(10):
        poe(MEIO_X, COMP - 4 + k // 3, int(conves(0.9)) + 4 + k // 2, MASTRO)


def main():
    casco()
    canhoes()
    mastro_e_vela()
    leme_e_detalhes()
    destino = os.path.join(
        os.path.dirname(__file__), "..", "..", "assets", "vox", "barcos", "chalupa.vox"
    )
    destino = os.path.normpath(destino)
    os.makedirs(os.path.dirname(destino), exist_ok=True)
    # O escritor quer a paleta de 255 entradas, indexada por [i+1].
    paleta = list(CORES) + [(0, 0, 0, 255)] * (256 - len(CORES))
    escrever_vox(destino, V, (BOCA, COMP, ALTO), [(0, 0, 0, 0)] + paleta)
    print(f"{destino}: {len(V)} voxels, {BOCA}x{COMP}x{ALTO}")


if __name__ == "__main__":
    main()
