#!/usr/bin/env bash
# Sul runner di casa (contenitore Ubuntu 24.04 con il volume /cache): Rust in /cache, e la cartella target
# del repo che punta a /cache/<nome>, così la compilazione riparte da dove era rimasta.
# Uso (dalla radice del repo, dopo il checkout): tools/prepara-contenitore.sh <nome-cartella-target>
set -euo pipefail
nome=${1:?nome della cartella target}
export RUSTUP_HOME=/cache/rustup CARGO_HOME=/cache/cargo
{ echo "RUSTUP_HOME=$RUSTUP_HOME"; echo "CARGO_HOME=$CARGO_HOME"; echo "CEDRO_CACHE=/cache"; } >> "$GITHUB_ENV"
echo "$CARGO_HOME/bin" >> "$GITHUB_PATH"
# Due runner sullo stesso minipc condividono /cache: Rust si installa o aggiorna uno alla volta.
(
  flock 9
  if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
    curl -fsSL --retry 5 https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
  else
    "$CARGO_HOME/bin/rustup" update stable --no-self-update
  fi
) 9>/cache/rust.lock
"$CARGO_HOME/bin/cargo" --version
mkdir -p "/cache/$nome"
rm -rf target
ln -s "/cache/$nome" target
# I motori: le loro prove compilano Cedro con CARGO_TARGET_DIR proprio.
echo "Cartella target: /cache/$nome"
