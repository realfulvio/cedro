#!/usr/bin/env bash
# Compila il motore di trascrizione in tutte le varianti per Linux (x86-64).
# Uso:  trascrivi/build.sh [-o dist] [-c] [-t /tmp/ctv] [variante...]
#   -o  cartella dei file finiti (dist)          -c  solo le varianti per processore, niente Vulkan
#   -t  cartella di compilazione (/tmp/ctv)      variante: base avx2 avxvnni avx512 avx512vnni vulkan
#
# Servono: Rust, cmake, un compilatore C++, libclang (per bindgen) e libopus con i suoi header.
# Per la variante «vulkan» anche gli header di Vulkan, `glslc` e il caricatore (libvulkan.so):
#   Arch: vulkan-headers vulkan-icd-loader shaderc   Debian/Ubuntu: libvulkan-dev glslc
# Senza i pacchetti di sistema: VULKAN_SDK=<cartella con include/, lib/libvulkan.so e Bin/glslc> e Bin nel PATH.
# Provato sul mini PC (Radeon integrata, RADV): «scheda video: AMD Radeon Graphics (RADV RENOIR)».
# Il nome dei file è quello che Cedro cerca nella release: cedro-trascrivi-<versione>-linux-x64-<variante>.
set -euo pipefail
cd "$(dirname "$0")"
out=dist; target=/tmp/ctv; only_cpu=0
while getopts "o:ct:" opt; do
  case $opt in
    o) out=$OPTARG ;;
    c) only_cpu=1 ;;
    t) target=$OPTARG ;;
    *) exit 2 ;;
  esac
done
shift $((OPTIND - 1))
version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p "$out"

# Ogni variante usa istruzioni diverse; GGML_NATIVE=OFF evita che il PC di build decida al posto di chi lo usa.
declare -A cpu=(
  [base]="GGML_AVX=OFF GGML_AVX2=OFF GGML_FMA=OFF GGML_F16C=OFF GGML_BMI2=OFF"
  [avx2]="GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON GGML_BMI2=ON"
  [avxvnni]="GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON GGML_BMI2=ON GGML_AVX_VNNI=ON"
  [avx512]="GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON GGML_BMI2=ON GGML_AVX512=ON"
  [avx512vnni]="GGML_AVX=ON GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON GGML_BMI2=ON GGML_AVX512=ON GGML_AVX512_VNNI=ON GGML_AVX512_VBMI=ON"
)
variants=("$@")
if [ ${#variants[@]} -eq 0 ]; then
  variants=(base avx2 avxvnni avx512 avx512vnni)
  if [ $only_cpu -eq 0 ] && command -v glslc >/dev/null && { [ -e /usr/include/vulkan/vulkan.h ] || [ -n "${VULKAN_SDK:-}" ]; }; then
    variants+=(vulkan)
  fi
fi

for name in "${variants[@]}"; do
  echo "== $name"
  # Le variabili GGML_* di una variante non devono passare alla successiva.
  for k in $(env | sed -n 's/^\(GGML_[A-Z0-9_]*\)=.*/\1/p'); do unset "$k"; done
  export GGML_NATIVE=OFF
  features=()
  if [ "$name" = vulkan ]; then
    # La scheda video fa il lavoro pesante: il processore resta sul minimo compatibile.
    for kv in ${cpu[avx2]}; do export "$kv"; done
    features=(--features vulkan)
  else
    [ -n "${cpu[$name]:-}" ] || { echo "variante sconosciuta: $name" >&2; exit 2; }
    for kv in ${cpu[$name]}; do export "$kv"; done
  fi
  CARGO_TARGET_DIR="$target/$name" cargo build --release "${features[@]}"
  install -m 755 "$target/$name/release/cedro-trascrivi" "$out/cedro-trascrivi-$version-linux-x64-$name"
done
echo "Pronto in $out (versione $version)"
