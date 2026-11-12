#!/usr/bin/env bash
# Fa analizzare su VirusTotal (70 e più antivirus) i file di una release di Cedro e scrive il risultato
# in fondo alle note della release, nel blocco tra <!-- antivirus --> e <!-- /antivirus -->.
# Uso: GH_TOKEN=… GH_REPO=owner/repo VT_API_KEY=… tools/antivirus.sh v0.43.0
# La chiave gratuita di VirusTotal permette 4 richieste al minuto: tra una richiesta e l'altra si aspetta.
# Il risultato si scrive com'è: se un antivirus segnala qualcosa (succede con gli exe non firmati) compare
# nella tabella, non si nasconde.
set -euo pipefail

TAG="${1:?tag della release}"
V="${TAG#v}"
: "${VT_API_KEY:?manca VT_API_KEY}"
: "${GH_REPO:?manca GH_REPO}"
API=https://www.virustotal.com/api/v3
LAST=0

vt() { # vt <curl args…>: una richiesta a VirusTotal, al massimo 4 al minuto
  local now wait
  now=$(date +%s); wait=$((LAST + 16 - now))
  [ "$wait" -gt 0 ] && sleep "$wait"
  LAST=$(date +%s)
  curl -sS --retry 4 --retry-delay 20 -H "x-apikey: $VT_API_KEY" "$@"
}

dir=$(mktemp -d)
gh release download "$TAG" --dir "$dir" \
  --pattern 'Cedro-*Setup-*.exe' --pattern 'Cedro-Portable-*.zip' --pattern 'cedro-*-linux-x64.tar.gz'
mapfile -t files < <(find "$dir" -maxdepth 1 -type f -printf '%f\n' | sort)
[ "${#files[@]}" -gt 0 ] || { echo "Nessun file da analizzare in $TAG"; exit 1; }

rows=""; worst=0; most=0
declare -A fatto   # sha256 -> "esito|segnalazioni": file identici si analizzano una volta sola
for f in "${files[@]}"; do
  sha=$(sha256sum "$dir/$f" | cut -d' ' -f1)
  echo "== $f  $sha"
  if [ -z "${fatto[$sha]:-}" ]; then
    id=""
    for _ in 1 2 3; do
      url=$(vt "$API/files/upload_url" | jq -r '.data // empty')
      [ -n "$url" ] && id=$(vt -F "file=@$dir/$f" "$url" | jq -r '.data.id // empty')
      [ -n "$id" ] && break
      sleep 30
    done
    [ -n "$id" ] || { echo "Caricamento di $f non riuscito"; exit 1; }
    a=""
    for _ in $(seq 1 60); do
      a=$(vt "$API/analyses/$id")
      [ "$(jq -r .data.attributes.status <<<"$a")" = completed ] && break
      sleep 30
    done
    [ "$(jq -r .data.attributes.status <<<"$a")" = completed ] || { echo "Analisi di $f non finita in tempo"; exit 1; }
    read -r bad sus tot < <(jq -r '.data.attributes.stats | "\(.malicious) \(.suspicious) \(.malicious + .suspicious + .undetected + .harmless)"' <<<"$a")
    chi=$(jq -r '[.data.attributes.results[] | select(.category=="malicious" or .category=="suspicious") | "\(.engine_name): \(.result // .category)"] | join("; ")' <<<"$a" | tr "|" "/")
    if [ $((bad + sus)) -eq 0 ]; then esito="✅ nessuna segnalazione su $tot"; else esito="⚠️ $((bad + sus)) segnalazione su $tot ($chi)"; fi
    fatto[$sha]="$esito|$((bad + sus))|$tot"
  fi
  IFS='|' read -r esito n tot <<<"${fatto[$sha]}"
  echo "   $esito"
  [ "$n" -gt "$worst" ] && worst=$n
  [ "$tot" -gt "$most" ] && most=$tot
  rows+="| \`$f\` | $esito | [rapporto](https://www.virustotal.com/gui/file/$sha) |"$'\n'
done

if [ "$worst" -eq 0 ]; then
  head="Ogni file di questa release è stato analizzato su [VirusTotal](https://www.virustotal.com) con $most antivirus: nessuno segnala problemi."
else
  head="Ogni file di questa release è stato analizzato su [VirusTotal](https://www.virustotal.com) con $most antivirus. Qualche segnalazione su programmi non ancora firmati è di solito un falso positivo: il rapporto dice quale antivirus e perché."
fi
blocco="<!-- antivirus -->
## Analisi antivirus

$head

| File | Risultato | Dettagli |
| --- | --- | --- |
${rows}
Analisi del $(date -u +%d/%m/%Y). I codici SHA-256 dei rapporti sono quelli dei file qui sopra.
<!-- /antivirus -->"

gh release view "$TAG" --json body --jq .body > "$dir/note.md"
# Toglie il blocco di un'analisi precedente e mette quello nuovo in fondo.
awk '/<!-- antivirus -->/{skip=1} !skip{print} /<!-- \/antivirus -->/{skip=0}' "$dir/note.md" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' > "$dir/nuove.md"
printf '\n\n%s\n' "$blocco" >> "$dir/nuove.md"
gh release edit "$TAG" --notes-file "$dir/nuove.md"
echo "$blocco"
