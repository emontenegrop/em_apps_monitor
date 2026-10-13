#!/usr/bin/env bash
# CA interna de EM Monitor.
#
#   ./pki/em-pki.sh init-ca                         Crea la CA raíz (una sola vez)
#   ./pki/em-pki.sh server <nombre> <SAN> [SAN...]  Emite un certificado de servidor
#                                                   SAN: DNS (monitor.local) o IP (10.0.0.5)
#   ./pki/em-pki.sh fingerprint                     Huella SHA-256 de la CA (para fijarla en los agentes)
#
# Salida:
#   pki/ca/ca.key, pki/ca/ca.crt                    ← ca.key NUNCA sale de esta máquina
#   pki/issued/<nombre>.key, pki/issued/<nombre>.crt
#
# Variables opcionales: EM_PKI_DIR, EM_CA_DAYS (3650), EM_CERT_DAYS (397), EM_CA_CN

set -euo pipefail

PKI_DIR="${EM_PKI_DIR:-$(cd "$(dirname "$0")" && pwd)}"
CA_DIR="$PKI_DIR/ca"
ISSUED_DIR="$PKI_DIR/issued"
CA_DAYS="${EM_CA_DAYS:-3650}"
CERT_DAYS="${EM_CERT_DAYS:-397}"
CA_CN="${EM_CA_CN:-EM Monitor Internal CA}"

die() { echo "error: $*" >&2; exit 1; }

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

init_ca() {
    [[ -f "$CA_DIR/ca.key" ]] && die "la CA ya existe en $CA_DIR (bórrala a mano si de verdad quieres regenerarla)"
    mkdir -p "$CA_DIR"
    chmod 700 "$CA_DIR"

    local cnf="$TMP_DIR/ca.cnf"
    cat > "$cnf" <<EOF
[req]
prompt             = no
distinguished_name = dn
x509_extensions    = v3_ca

[dn]
CN = $CA_CN
O  = EM Monitor

[v3_ca]
basicConstraints       = critical, CA:TRUE, pathlen:0
keyUsage               = critical, keyCertSign, cRLSign
subjectKeyIdentifier   = hash
authorityKeyIdentifier = keyid:always
EOF

    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out "$CA_DIR/ca.key"
    chmod 600 "$CA_DIR/ca.key"
    openssl req -new -x509 -sha384 -days "$CA_DAYS" -key "$CA_DIR/ca.key" \
        -config "$cnf" -out "$CA_DIR/ca.crt"

    echo "CA creada: $CA_DIR/ca.crt (válida $CA_DAYS días)"
    fingerprint
}

issue_server() {
    local name="${1:-}"; shift || true
    [[ -n "$name" && $# -gt 0 ]] || die "uso: $0 server <nombre> <SAN> [SAN...]"
    [[ "$name" =~ ^[A-Za-z0-9._-]+$ ]] || die "nombre inválido: $name"
    [[ -f "$CA_DIR/ca.key" ]] || die "no hay CA; ejecuta primero: $0 init-ca"
    mkdir -p "$ISSUED_DIR"

    local sans=() i=1 san
    for san in "$@"; do
        if [[ "$san" =~ ^[0-9.]+$ || "$san" == *:* ]]; then
            sans+=("IP.$i = $san")
        else
            sans+=("DNS.$i = $san")
        fi
        i=$((i + 1))
    done

    local cnf="$TMP_DIR/server.cnf" csr="$TMP_DIR/server.csr"
    cat > "$cnf" <<EOF
[req]
prompt             = no
distinguished_name = dn

[dn]
CN = $1
O  = EM Monitor

[v3_server]
basicConstraints       = critical, CA:FALSE
keyUsage               = critical, digitalSignature
extendedKeyUsage       = serverAuth
subjectKeyIdentifier   = hash
authorityKeyIdentifier = keyid
subjectAltName         = @alt_names

[alt_names]
$(printf '%s\n' "${sans[@]}")
EOF

    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "$ISSUED_DIR/$name.key"
    chmod 600 "$ISSUED_DIR/$name.key"
    openssl req -new -key "$ISSUED_DIR/$name.key" -config "$cnf" -out "$csr"
    openssl x509 -req -sha256 -days "$CERT_DAYS" -in "$csr" \
        -CA "$CA_DIR/ca.crt" -CAkey "$CA_DIR/ca.key" -CAserial "$CA_DIR/ca.srl" -CAcreateserial \
        -extfile "$cnf" -extensions v3_server -out "$ISSUED_DIR/$name.crt"

    openssl verify -CAfile "$CA_DIR/ca.crt" "$ISSUED_DIR/$name.crt" >/dev/null
    echo "Certificado emitido: $ISSUED_DIR/$name.crt (válido $CERT_DAYS días, SAN: $*)"
}

fingerprint() {
    [[ -f "$CA_DIR/ca.crt" ]] || die "no hay CA; ejecuta primero: $0 init-ca"
    openssl x509 -in "$CA_DIR/ca.crt" -noout -fingerprint -sha256 | sed 's/^.*=//; s/://g' | tr 'A-F' 'a-f'
}

case "${1:-}" in
    init-ca)     init_ca ;;
    server)      shift; issue_server "$@" ;;
    fingerprint) fingerprint ;;
    *)           sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
