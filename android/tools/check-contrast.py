#!/usr/bin/env python3
"""Rapporto WCAG dei testi delle quattro tinte e delle tessere attività."""
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parents[2]
source = (root / 'src/ui/theme.rs').read_text()
def rgb(n):
    return (n >> 16, n >> 8 & 255, n & 255)
def luminance(c):
    v = [x / 255 for x in c]
    v = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in v]
    return sum(x * k for x, k in zip(v, (.2126, .7152, .0722)))
def contrast(a, b):
    a, b = sorted((luminance(a), luminance(b)))
    return (b + .05) / (a + .05)
failed = False
for name in ('MELANZANA', 'NOTTE', 'BOSCO', 'CARTA'):
    body = re.search(r'pub const TINT_' + name + r': Tint = Tint \{(.*?)\n\};', source, re.S)[1]
    values = {k: rgb(int(v, 16)) for k, v in re.findall(r'(\w+): hex\(0x([0-9A-Fa-f]+)\)', body)}
    results = [(ink + '/' + bg, contrast(values[ink], values[bg])) for ink in ('ink', 'muted', 'brand') for bg in ('frame', 'frame_hi')]
    for pair, value in results:
        print(f'{name} {pair}: {value:.2f}:1')
        failed |= value < 4.5
telefono = (root / 'android/src/ui/casa/telefono.rs').read_text()
# La grafica a barra di sei attività non ha tessere colorate con testo: il controllo vale solo se ci sono.
activities = telefono.split('const ATTIVITA:')[1].split('];', 1)[0] if 'const ATTIVITA:' in telefono else ''
for match in re.finditer(r'\("([^"]+)".*?\((\d+), (\d+), (\d+)\), \((\d+), (\d+), (\d+)\)\)', activities):
    name, *numbers = match.groups()
    values = list(map(int, numbers))
    value = contrast(values[:3], values[3:])
    print(f'Attività {name}: {value:.2f}:1')
    failed |= value < 4.5
sys.exit(1 if failed else 0)
