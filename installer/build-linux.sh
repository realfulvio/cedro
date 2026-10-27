#!/usr/bin/env bash
# Compila cedro e crea dist/cedro-<versione>-linux-x64.tar.gz (con il file delle licenze e install.sh).
# Uso:  installer/build-linux.sh [-o dist] [-b]
# Con -b crea anche la copia per chi ha ancora la vecchia edizione Comune di Bondeno (che cerca solo
# dist/cedro-bondeno-<versione>-linux-x64.tar.gz per aggiornarsi): stesso programma, più il file «da-bondeno»,
# che fa accendere il tema Bondeno al primo avvio. Il Comune ora è un tema di Cedro.
# Il motore di trascrizione si compila a parte con trascrivi/build.sh: i suoi file vanno nella stessa
# release, perché Cedro li scarica alla prima trascrizione.
# Per provare lo script senza compilare in release: CEDRO_BIN=target/debug/cedro installer/build-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."
out=dist
bondeno=0
while getopts "o:b" opt; do
  case $opt in
    o) out=$OPTARG ;;
    b) bondeno=1 ;;
    *) exit 2 ;;
  esac
done
version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
bin=${CEDRO_BIN:-target/release/cedro}
if [ -z "${CEDRO_BIN:-}" ]; then
  cargo build --release
fi
mkdir -p "$out"
licenses="$out/Licenze di terze parti.txt"
# Il file con tutte le licenze (autori e testi) lo fa lo stesso Cedro dai sorgenti usati.
cargo run --release --example licenze -- "$licenses"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
# Crea dist/<prefisso>-<versione>-linux-x64.tar.gz; con "da-bondeno" il pacchetto lascia il file che accende il tema Bondeno.
pack() {
  local prefix=$1 marker=$2
  local name="$prefix-$version-linux-x64"
  mkdir -p "$stage/$name"
  install -m 755 "$bin" "$stage/$name/cedro"
  install -m 644 "$licenses" "$stage/$name/Licenze di terze parti.txt"
  install -m 644 LICENSE "$stage/$name/LICENSE"
  install -m 755 installer/install-linux.sh "$stage/$name/install.sh"
  [ "$marker" = "da-bondeno" ] && : > "$stage/$name/da-bondeno"
  tar -C "$stage" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
  (cd "$out" && sha256sum "$name.tar.gz" | tee "$name.tar.gz.sha256")
  echo "Pronto: $out/$name.tar.gz"
}
pack cedro ""
[ "$bondeno" = 1 ] && pack cedro-bondeno da-bondeno
exit 0
