#!/usr/bin/env bash
# Aggiunge alle note di una release la riga dell'installatore Windows on ARM e il consiglio su quale
# scegliere. Non fa nulla se ci sono già. Uso: tools/note-arm.sh note.md <versione> <owner/repo>
set -euo pipefail
f="$1"; v="$2"; repo="$3"
grep -q 'ARM64-Setup-' "$f" && exit 0
row="| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-$v.exe**](https://github.com/$repo/releases/download/v$v/Cedro-ARM64-Setup-$v.exe) |"
hint="Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro."
awk -v row="$row" -v hint="$hint" '
  { print }
  /^\| \*\*Windows\*\* \(quasi tutti i PC\)/ && !r { print row; r=1 }
  /^Scarica il file e fai doppio clic/ && !h { print hint; h=1 }
' "$f" > "$f.tmp"
mv "$f.tmp" "$f"
grep -q 'ARM64-Setup-' "$f" || { echo "note-arm: riga «Windows» non trovata in $f"; exit 1; }
