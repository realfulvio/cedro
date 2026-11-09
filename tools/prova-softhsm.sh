#!/usr/bin/env bash
# Prepara una «chiavetta» finta con SoftHSM2 (PKCS#11 software) per provare la firma di Cedro
# senza una chiavetta vera: un token con PIN 123456 e PUK (SO PIN) 87654321, una chiave RSA 2048
# e un certificato di firma (non ripudio) intestato a MARIO ROSSI.
#   source tools/prova-softhsm.sh   → poi: cargo test --lib firma::pkcs11 -- --nocapture
# Serve: softhsm2, opensc (pkcs11-tool), openssl.
set -euo pipefail
DIR="${CEDRO_SOFTHSM_DIR:-${TMPDIR:-/tmp}/cedro-softhsm}"
rm -rf "$DIR" && mkdir -p "$DIR/tokens"
cat > "$DIR/softhsm2.conf" <<CONF
directories.tokendir = $DIR/tokens
objectstore.backend = file
log.level = ERROR
CONF
export SOFTHSM2_CONF="$DIR/softhsm2.conf"
LIB=""
for p in /usr/lib/softhsm/libsofthsm2.so /usr/lib/x86_64-linux-gnu/softhsm/libsofthsm2.so /usr/lib64/softhsm/libsofthsm2.so /usr/local/lib/softhsm/libsofthsm2.so; do
  [ -f "$p" ] && LIB="$p" && break
done
[ -n "$LIB" ] || { echo "libsofthsm2.so non trovata"; return 1 2>/dev/null || exit 1; }
softhsm2-util --init-token --free --label "ArubaKey prova" --pin 123456 --so-pin 87654321 >/dev/null
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$DIR/key.pem" -out "$DIR/cert.pem" -days 365 \
  -subj "/C=IT/O=Comune di Bondeno/GN=MARIO/SN=ROSSI/serialNumber=TINIT-RSSMRA80A01H501U/CN=MARIO ROSSI" \
  -addext "keyUsage=critical,nonRepudiation" -addext "basicConstraints=CA:FALSE" 2>/dev/null
openssl pkcs8 -topk8 -nocrypt -in "$DIR/key.pem" -out "$DIR/key8.pem"
softhsm2-util --import "$DIR/key8.pem" --token "ArubaKey prova" --label "firma" --id 01 --pin 123456 >/dev/null
openssl x509 -in "$DIR/cert.pem" -outform der -out "$DIR/cert.der"
pkcs11-tool --module "$LIB" --token-label "ArubaKey prova" --login --pin 123456 --write-object "$DIR/cert.der" --type cert --id 01 --label "firma" >/dev/null
export CEDRO_PKCS11="$LIB" CEDRO_SOFTHSM_PIN=123456 CEDRO_SOFTHSM_SO_PIN=87654321
echo "SoftHSM pronto: $LIB (PIN 123456, PUK 87654321)"
