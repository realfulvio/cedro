#!/usr/bin/env bash
# Prova vera dei motori del riassunto: ogni motore riassume una riunione breve (prova.json) con Qwen3.5-4B.
# Uso:  riassumi/prova.sh <cartella dei motori> <modello .gguf>
# Passa se ogni motore arriva in fondo («end») con i quattro titoli che Cedro legge.
set -euo pipefail
dir=$1; model=$2
here=$(cd "$(dirname "$0")" && pwd)
fail=()
for exe in "$dir"/cedro-riassumi-*; do
  echo "== $(basename "$exe")"
  start=$(date +%s)
  out=$(timeout 1200 "$exe" --model "$model" < "$here/prova.json" || true)
  grep -v '"e":"text"' <<<"$out" | grep -v '"e":"progress"' | cut -c1-600
  end=$(grep '"e":"end"' <<<"$out" || true)
  echo "   $(( $(date +%s) - start )) s"
  ok=1
  for t in "## In breve" "## Chi ha detto cosa" "## Decisioni" "## Da fare"; do grep -qF "$t" <<<"$end" || ok=0; done
  [ $ok -eq 1 ] || fail+=("$(basename "$exe")")
done
[ ${#fail[@]} -eq 0 ] || { echo "Motori che non riassumono: ${fail[*]}"; exit 1; }
echo "Tutti i motori riassumono."
