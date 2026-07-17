#!/bin/zsh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT="$ROOT/.github-secrets"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$OUTPUT"
PASSWORD="$(/usr/bin/openssl rand -base64 24)"

cat > "$TMP/openssl.conf" <<'EOF'
[ req ]
distinguished_name = dn
x509_extensions = ext
prompt = no
[ dn ]
CN = Kedu Release
O = 0x30
[ ext ]
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
basicConstraints = critical,CA:false
EOF

/usr/bin/openssl req -x509 -newkey rsa:2048 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -days 3650 -nodes -config "$TMP/openssl.conf" >/dev/null 2>&1
/usr/bin/openssl pkcs12 -export \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -out "$TMP/cert.p12" -passout "pass:$PASSWORD" >/dev/null 2>&1

base64 -i "$TMP/cert.p12" | tr -d '\n' > "$OUTPUT/KEDU_CERT_P12.txt"
printf '%s' "$PASSWORD" > "$OUTPUT/KEDU_CERT_PWD.txt"
chmod 600 "$OUTPUT/KEDU_CERT_P12.txt" "$OUTPUT/KEDU_CERT_PWD.txt"

echo "Generated GitHub Secret values in $OUTPUT"
echo "gh secret set KEDU_CERT_P12 < $OUTPUT/KEDU_CERT_P12.txt"
echo "gh secret set KEDU_CERT_PWD < $OUTPUT/KEDU_CERT_PWD.txt"
