#!/usr/bin/env python3
"""Controllo statico delle chiavi letterali Android e delle cinque tabelle."""
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parents[2]
# lit() marca dati dell'utente/formati: non è una chiamata di traduzione.
call = re.compile(r'(?:\b(?:tr|tr_string)|\.(?:text|text_wrap))\s*\(\s*(?:&\s*)?"((?:[^"\\]|\\.)*)"')
helper = re.compile(r'\b(?:testo_a|testo_centrato_a)\s*\(\s*p\s*,\s*"((?:[^"\\]|\\.)*)"')
keys = {}
for path in (root / 'android/src').rglob('*.rs'):
    source = path.read_text()
    # Test unitari e fixture non entrano nell'APK.
    source = source.split('#[cfg(test)]')[0]
    for match in (*call.finditer(source), *helper.finditer(source)):
        key = match[1].replace(r'\n', '\n').replace(r'\"', '"').replace(r'\\', '\\')
        if key.strip() and re.search(r'[A-Za-zÀ-ÿ]', key):
            keys.setdefault(key, set()).add(str(path.relative_to(root)))
# Nomi di prodotto, formati e valori non linguistici invarianti.
invariants = {'Cedro', 'PDF', 'PDF/A', 'TXT', 'SRT', 'VTT', 'Word', 'YouTube', 'Àa', 'OCR', 'ZIP', 'PNG', 'JPG', 'HEIC', 'NDPI', 'DICOM'}
failed = False
for lang in ('en', 'de', 'es', 'fr', 'pt'):
    table = {}
    for line in (root / f'i18n/{lang}.tsv').read_text().splitlines():
        if '\t' in line and not line.startswith('#'):
            key, value = line.split('\t', 1)
            table[key.replace(r'\n', '\n')] = value
    missing = sorted(k for k in keys if k not in invariants and not table.get(k))
    print(f'{lang}: {len(keys)} chiavi letterali, {len(missing)} mancanti')
    for key in missing:
        print(f'  {key!r}: {", ".join(sorted(keys[key]))}')
    failed |= bool(missing)
sys.exit(1 if failed else 0)
