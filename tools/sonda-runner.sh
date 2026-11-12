#!/usr/bin/env bash
# Sceglie dove far girare una build: su un runner di casa con l'etichetta $1 (cedro-linux, cedro-windows)
# se ce n'è uno acceso e libero, altrimenti sui runner di GitHub.
# Lancia la sonda (.github/workflows/sonda-runner.yml) e aspetta al massimo ${SONDA_ATTESA:-150} secondi
# che un runner la prenda; se non succede la annulla. Scrive «locale=true|false» in $GITHUB_OUTPUT.
# Serve: gh con GH_TOKEN (permesso actions: write), GH_REPO, e la ref da cui lanciare in SONDA_REF.
set -uo pipefail
etichetta=$1
attesa=${SONDA_ATTESA:-150}
id="${GITHUB_RUN_ID:-0}-${GITHUB_RUN_ATTEMPT:-1}-${GITHUB_JOB:-job}-$etichetta"
titolo="Sonda $etichetta $id"
locale=false
esito() { echo "locale=$locale" >> "${GITHUB_OUTPUT:-/dev/stdout}"; echo "Build su: $([ $locale = true ] && echo "runner di casa ($etichetta)" || echo 'runner di GitHub')"; exit 0; }

if ! gh workflow run sonda-runner.yml --ref "${SONDA_REF:-main}" -f etichetta="$etichetta" -f id="$id"; then
  echo "::warning::Sonda non lanciata: si usa GitHub"; esito
fi
run=""
for _ in $(seq 1 30); do
  run=$(gh run list --workflow sonda-runner.yml --event workflow_dispatch --limit 30 --json databaseId,displayTitle \
        --jq ".[] | select(.displayTitle == \"$titolo\") | .databaseId" | head -1)
  [ -n "$run" ] && break
  sleep 3
done
[ -n "$run" ] || { echo "::warning::Sonda lanciata ma non trovata: si usa GitHub"; esito; }
echo "Sonda: https://github.com/$GH_REPO/actions/runs/$run"
fine=$((SECONDS + attesa))
while [ $SECONDS -lt $fine ]; do
  stato=$(gh run view "$run" --json status,conclusion --jq '.status + " " + .conclusion')
  case "$stato" in
    "completed success") locale=true; esito ;;
    completed*) echo "Sonda finita: $stato"; esito ;;
  esac
  # Se un runner l'ha già presa (job in corso) basta aspettare che finisca, anche oltre l'attesa.
  if gh run view "$run" --json jobs --jq '.jobs[0].status' | grep -q in_progress; then fine=$((SECONDS + 120)); fi
  sleep 5
done
echo "Nessun runner $etichetta libero entro $attesa s: annullo la sonda"
gh run cancel "$run" >/dev/null 2>&1 || true
esito
