#!/usr/bin/env bash
# Compila cedro e crea dist/cedro-<versione>-linux-x64.tar.gz (con il file delle licenze e install.sh).
# Uso:  installer/build-linux.sh [-o dist]
# Il motore di trascrizione si compila a parte con trascrivi/build.sh: i suoi file vanno nella stessa
# release, perché Cedro li scarica alla prima trascrizione.
# Per provare lo script senza compilare in release: CEDRO_BIN=target/debug/cedro installer/build-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."
out=dist
while getopts "o:" opt; do
  case $opt in
    o) out=$OPTARG ;;
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
# Crea dist/<prefisso>-<versione>-linux-x64.tar.gz.
pack() {
  local prefix=$1
  local name="$prefix-$version-linux-x64"
  mkdir -p "$stage/$name"
  install -m 755 "$bin" "$stage/$name/cedro"
  install -m 644 "$licenses" "$stage/$name/Licenze di terze parti.txt"
  install -m 644 LICENSE "$stage/$name/LICENSE"
  install -m 755 installer/install-linux.sh "$stage/$name/install.sh"
  tar -C "$stage" --owner=0 --group=0 --sort=name -czf "$out/$name.tar.gz" "$name"
  (cd "$out" && sha256sum "$name.tar.gz" | tee "$name.tar.gz.sha256")
  echo "Pronto: $out/$name.tar.gz"
}
pack cedro
exit 0
