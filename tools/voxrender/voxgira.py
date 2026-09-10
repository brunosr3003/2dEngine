#!/usr/bin/env python3
"""Gira um modelo MagicaVoxel meia volta em torno do eixo vertical.

Existe por causa de uma convenção: no Tempest a FRENTE de todo modelo é o
`+Y` do voxel, que a malha manda pro `+Z` do mundo. Modelo feito olhando pro
`-Y` — que é pra onde a câmera padrão do MagicaVoxel olha — anda de costas no
jogo, virando o rosto pra câmera justamente quando corre pra longe dela.

Corrigir isso no cliente seria uma lista de exceções que ninguém lembra de
atualizar. Melhor consertar o arquivo.

A edição é cirúrgica: reescreve só as coordenadas dentro dos chunks XYZI,
byte a byte, e não toca em paleta, materiais nem grafo de cena. O arquivo sai
do mesmo tamanho.

    voxgira.py assets/vox/player.vox            # gira no lugar
    voxgira.py entrada.vox -o saida.vox
"""

import argparse
import struct
import sys


def gira(bytes_do_arquivo: bytes) -> bytes:
    dados = bytearray(bytes_do_arquivo)
    if bytes(dados[:4]) != b"VOX ":
        raise SystemExit("não é um .vox")

    # Pilha de (início, fim) dos conteúdos a percorrer: o MAIN tem filhos.
    pilha = [(8, len(dados))]
    tamanho_atual = None
    girados = 0
    while pilha:
        i, fim = pilha.pop()
        while i + 12 <= fim:
            marca = bytes(dados[i : i + 4])
            n, m = struct.unpack_from("<II", dados, i + 4)
            corpo = i + 12
            if marca == b"SIZE":
                tamanho_atual = struct.unpack_from("<III", dados, corpo)
            elif marca == b"XYZI":
                if tamanho_atual is None:
                    raise SystemExit("XYZI sem SIZE antes — arquivo estranho")
                sx, sy, _sz = tamanho_atual
                (quantos,) = struct.unpack_from("<I", dados, corpo)
                for k in range(quantos):
                    o = corpo + 4 + k * 4
                    x, y = dados[o], dados[o + 1]
                    # Meia volta: nega os dois eixos horizontais em torno do
                    # centro da caixa. Negar UM só seria espelho, e espelho
                    # troca a mão do modelo.
                    dados[o] = sx - 1 - x
                    dados[o + 1] = sy - 1 - y
                girados += quantos
            if m > 0:
                pilha.append((corpo + n, corpo + n + m))
            i = corpo + n + m
    if girados == 0:
        raise SystemExit("nenhum voxel encontrado")
    print(f"{girados} voxels girados")
    return bytes(dados)


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("entrada")
    ap.add_argument("-o", "--out", help="saída (padrão: sobrescreve a entrada)")
    a = ap.parse_args()
    with open(a.entrada, "rb") as f:
        saida = gira(f.read())
    destino = a.out or a.entrada
    with open(destino, "wb") as f:
        f.write(saida)
    print(f"-> {destino}")


if __name__ == "__main__":
    main()
