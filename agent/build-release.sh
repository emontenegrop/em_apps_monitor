#!/usr/bin/env bash
# Compila el agente para Linux x86_64 y aarch64 (binarios estáticos) y deja en dist/agent/:
#   em-agent-linux-amd64, em-agent-linux-arm64, install.sh, SHA256SUMS
# nginx los publica en https://<central>/agent/ para el instalador.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DOCKER_BUILDKIT=1 docker build \
  -f agent/Dockerfile.release \
  --output "type=local,dest=dist/agent" \
  .

echo
echo "Binarios en dist/agent/:"
cat dist/agent/SHA256SUMS
