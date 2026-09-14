#!/usr/bin/env python3
"""Icone 1024x1024 do app iOS (assets/ios/AppIcon-1024.png).

Opaco (a App Store recusa alfa), deterministico: rodar de novo gera o mesmo
PNG. Ceu de tempestade em gradiente, farol de pedra no centro e um raio.

    python3 tools/ios/gerar_icone.py
"""
import math
import os

from PIL import Image, ImageDraw, ImageFilter

LADO = 1024
RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SAIDA = os.path.join(RAIZ, "assets", "ios", "AppIcon-1024.png")


def mistura(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def main():
    img = Image.new("RGB", (LADO, LADO))
    px = img.load()
    topo, meio, base = (14, 20, 46), (28, 58, 104), (18, 104, 128)
    for y in range(LADO):
        t = y / (LADO - 1)
        cor = mistura(topo, meio, t / 0.6) if t < 0.6 else mistura(meio, base, (t - 0.6) / 0.4)
        for x in range(LADO):
            # vinheta suave
            d = math.hypot(x - LADO / 2, y - LADO * 0.42) / (LADO * 0.75)
            k = max(0.55, 1.0 - d * 0.55)
            px[x, y] = tuple(int(c * k) for c in cor)

    # halo do farol
    halo = Image.new("L", (LADO, LADO), 0)
    ImageDraw.Draw(halo).ellipse((262, 150, 762, 650), fill=150)
    halo = halo.filter(ImageFilter.GaussianBlur(90))
    img = Image.composite(Image.new("RGB", (LADO, LADO), (255, 214, 120)), img, halo)

    d = ImageDraw.Draw(img)
    # mar
    for i, y in enumerate(range(760, LADO, 26)):
        d.rectangle((0, y, LADO, y + 13), fill=(16, 72 + i * 4, 104 + i * 3))
    # rochedo
    d.polygon([(250, 800), (380, 700), (650, 690), (790, 800)], fill=(40, 46, 58))
    # torre do farol (trapezio listrado)
    base_y, topo_y = 720, 360
    for i in range(6):
        y0 = topo_y + (base_y - topo_y) * i / 6
        y1 = topo_y + (base_y - topo_y) * (i + 1) / 6
        w0 = 70 + 50 * i / 6
        w1 = 70 + 50 * (i + 1) / 6
        cor = (236, 232, 220) if i % 2 == 0 else (196, 58, 48)
        d.polygon([(512 - w0, y0), (512 + w0, y0), (512 + w1, y1), (512 - w1, y1)], fill=cor)
    # lanterna e cupula
    d.rectangle((442, 300, 582, 362), fill=(255, 226, 140))
    d.rectangle((430, 290, 594, 304), fill=(40, 46, 58))
    d.polygon([(430, 290), (512, 220), (594, 290)], fill=(40, 46, 58))
    # feixes de luz
    feixe = Image.new("L", (LADO, LADO), 0)
    fd = ImageDraw.Draw(feixe)
    fd.polygon([(512, 330), (0, 250), (0, 410)], fill=110)
    fd.polygon([(512, 330), (LADO, 250), (LADO, 410)], fill=110)
    feixe = feixe.filter(ImageFilter.GaussianBlur(18))
    img = Image.composite(Image.new("RGB", (LADO, LADO), (255, 236, 170)), img, feixe)

    d = ImageDraw.Draw(img)
    # raio
    raio = [(770, 70), (690, 250), (745, 250), (660, 440), (820, 210), (760, 210), (840, 70)]
    d.polygon(raio, fill=(255, 244, 160))

    os.makedirs(os.path.dirname(SAIDA), exist_ok=True)
    img.save(SAIDA, "PNG", optimize=False)
    print(SAIDA)


if __name__ == "__main__":
    main()
