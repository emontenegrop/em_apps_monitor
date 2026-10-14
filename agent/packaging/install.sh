#!/bin/sh
# Instalador del agente de EM Monitor para hosts Linux con systemd.
# Ver usage() o ejecutar con --help.
set -eu

PREFIX=/usr/local/bin
CONFIG_DIR=/etc/em-agent
UNIT=/etc/systemd/system/em-agent.service
PODMAN_DROPIN_DIR=/etc/systemd/system/podman.socket.d
AGENT_USER=em-agent

usage() {
	cat <<'EOF'
Instalador del agente de EM Monitor (Linux con systemd).

Instalar (el comando completo lo genera la UI del central):
  sh install.sh --url https://CENTRAL:8443 --token FICHA --ca-fingerprint HUELLA [opciones]

Opciones:
  --ca-file RUTA         CA del central ya descargada (si no, se descarga y verifica por huella)
  --run-as-user USUARIO  Ejecuta el agente como ese usuario, para Podman sin root (rootless)
  --no-start             Instala e inscribe, pero no habilita ni inicia el servicio

Desinstalar:
  sh install.sh --uninstall [--purge]   (--purge borra también /etc/em-agent)
EOF
}

log() { printf '%s\n' "$*"; }
step() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
need_value() { [ $# -ge 2 ] && [ -n "$2" ] || die "falta el valor de $1"; }

# Un comando pegado con finales de linea CRLF deja retornos de carro en los argumentos
for arg in "$@"; do
	shift
	arg=$(printf '%s' "$arg" | tr -d '\r')
	[ -n "$arg" ] && set -- "$@" "$arg"
done

URL='' TOKEN='' FP='' CA_FILE='' RUN_AS='' NO_START=0 UNINSTALL=0 PURGE=0
while [ $# -gt 0 ]; do
	case "$1" in
	--url) need_value "$@"; URL=${2%/}; shift 2 ;;
	--token) need_value "$@"; TOKEN=$2; shift 2 ;;
	--ca-fingerprint) need_value "$@"; FP=$2; shift 2 ;;
	--ca-file) need_value "$@"; CA_FILE=$2; shift 2 ;;
	--run-as-user) need_value "$@"; RUN_AS=$2; shift 2 ;;
	--no-start) NO_START=1; shift ;;
	--uninstall) UNINSTALL=1; shift ;;
	--purge) PURGE=1; shift ;;
	-h | --help) usage; exit 0 ;;
	*) usage >&2; die "opción desconocida: $1" ;;
	esac
done

[ "$(id -u)" -eq 0 ] || die "ejecuta el instalador como root (sudo)"
HAVE_SYSTEMD=0
[ -d /run/systemd/system ] && HAVE_SYSTEMD=1

# ── Desinstalación ──────────────────────────────────────────────────────────
if [ "$UNINSTALL" -eq 1 ]; then
	step "Desinstalando el agente"
	if [ "$HAVE_SYSTEMD" -eq 1 ]; then
		systemctl disable --now em-agent.service 2>/dev/null || true
	fi
	rm -f "$UNIT" "$PREFIX/em-agent" "$PODMAN_DROPIN_DIR/em-agent.conf"
	if [ "$HAVE_SYSTEMD" -eq 1 ]; then
		systemctl daemon-reload
		systemctl try-restart podman.socket 2>/dev/null || true
	fi
	if [ "$PURGE" -eq 1 ]; then
		rm -rf /etc/em-agent
		userdel "$AGENT_USER" 2>/dev/null || true
		groupdel "$AGENT_USER" 2>/dev/null || true
		log "Configuración y credencial eliminadas."
	else
		log "La configuración queda en $CONFIG_DIR (usa --purge para borrarla)."
	fi
	log "Recuerda revocar la credencial del servidor en la UI del central."
	exit 0
fi

# ── Validaciones ────────────────────────────────────────────────────────────
[ -n "$URL" ] && [ -n "$TOKEN" ] && [ -n "$FP" ] || { usage >&2; die "faltan --url, --token o --ca-fingerprint"; }
case "$URL" in https://*) ;; *) die "--url debe empezar con https://" ;; esac
for cmd in curl sha256sum base64 install; do
	command -v "$cmd" >/dev/null 2>&1 || die "falta el comando '$cmd'"
done
FP=$(printf '%s' "$FP" | tr -d ':[:space:]' | tr 'A-F' 'a-f')
printf '%s' "$FP" | grep -Eq '^[0-9a-f]{64}$' || die "huella de CA inválida (se esperan 64 caracteres hexadecimales)"

case "$(uname -m)" in
x86_64 | amd64) ARCH=amd64 ;;
aarch64 | arm64) ARCH=arm64 ;;
*) die "arquitectura no soportada: $(uname -m)" ;;
esac

if [ -n "$RUN_AS" ]; then
	id "$RUN_AS" >/dev/null 2>&1 || die "el usuario $RUN_AS no existe"
	RUN_UID=$(id -u "$RUN_AS")
	RUN_GROUP=$(id -gn "$RUN_AS")
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# ── CA del central, verificada por huella ───────────────────────────────────
step "Verificando la CA del central"
if [ -z "$CA_FILE" ]; then
	# Sin -k no se podría descargar: todavía no se confía en la CA. La huella la valida.
	curl -fsSk "$URL/api/agent/ca.der" -o "$TMP/ca.der" || die "no se pudo descargar la CA de $URL"
	{
		echo '-----BEGIN CERTIFICATE-----'
		base64 -w 64 "$TMP/ca.der"
		echo '-----END CERTIFICATE-----'
	} >"$TMP/ca.crt"
	CA_FILE="$TMP/ca.crt"
fi
ACTUAL=$(sed '/-----/d' "$CA_FILE" | base64 -d | sha256sum | cut -d' ' -f1)
[ "$ACTUAL" = "$FP" ] || die "la huella de la CA no coincide (esperada $FP, recibida $ACTUAL): posible intermediario, se aborta"
log "CA verificada: $FP"

# ── Binario, verificado con SHA256SUMS ──────────────────────────────────────
step "Descargando em-agent ($ARCH)"
BIN="em-agent-linux-$ARCH"
curl -fsS --cacert "$CA_FILE" "$URL/agent/$BIN" -o "$TMP/$BIN" || die "no se pudo descargar $BIN"
curl -fsS --cacert "$CA_FILE" "$URL/agent/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "no se pudo descargar SHA256SUMS"
# Acepta el formato de texto ("hash  archivo") y el binario ("hash *archivo")
(cd "$TMP" && grep -E " [*]?$BIN\$" SHA256SUMS | sha256sum -c --quiet -) || die "el binario no coincide con SHA256SUMS"
install -m 0755 "$TMP/$BIN" "$PREFIX/em-agent"
log "Instalado $PREFIX/em-agent ($("$PREFIX/em-agent" --version))"

# ── Usuario y acceso a los sockets ──────────────────────────────────────────
step "Configurando usuario y permisos"
SUPP_GROUPS=''
if [ -n "$RUN_AS" ]; then
	SVC_USER=$RUN_AS
	SVC_GROUP=$RUN_GROUP
	log "El agente correrá como $RUN_AS (Podman sin root)"
	if [ "$HAVE_SYSTEMD" -eq 1 ]; then
		loginctl enable-linger "$RUN_AS"
		# El administrador de sesión del usuario arranca con linger; se espera su directorio
		i=0
		while [ ! -d "/run/user/$RUN_UID" ] && [ $i -lt 20 ]; do sleep 0.5; i=$((i + 1)); done
		runuser -u "$RUN_AS" -- env XDG_RUNTIME_DIR="/run/user/$RUN_UID" \
			systemctl --user enable --now podman.socket ||
			log "aviso: no se pudo habilitar podman.socket del usuario $RUN_AS"
	fi
else
	SVC_USER=$AGENT_USER
	SVC_GROUP=$AGENT_USER
	getent group "$AGENT_USER" >/dev/null || groupadd --system "$AGENT_USER"
	id "$AGENT_USER" >/dev/null 2>&1 ||
		useradd --system --gid "$AGENT_USER" --no-create-home --home-dir /nonexistent \
			--shell /usr/sbin/nologin --comment "EM Monitor agent" "$AGENT_USER"

	# Docker: el grupo docker da acceso al socket (equivale a root sobre Docker; el agente solo lee)
	if getent group docker >/dev/null; then
		SUPP_GROUPS=docker
		# También en la cuenta, para que `sudo -u em-agent em-agent check` vea lo mismo que el servicio
		usermod -aG docker "$AGENT_USER"
		log "Acceso a Docker mediante el grupo docker"
	fi
	# Podman con root: el socket se abre al grupo del agente con un drop-in de podman.socket
	if command -v podman >/dev/null 2>&1 && [ "$HAVE_SYSTEMD" -eq 1 ]; then
		mkdir -p "$PODMAN_DROPIN_DIR"
		cat >"$PODMAN_DROPIN_DIR/em-agent.conf" <<EOF
# Creado por el instalador de em-agent: el agente lee la API de Podman
[Socket]
SocketGroup=$AGENT_USER
SocketMode=0660
EOF
		systemctl daemon-reload
		systemctl enable podman.socket >/dev/null 2>&1 || true
		systemctl restart podman.socket || log "aviso: no se pudo reiniciar podman.socket"
		log "Acceso a Podman mediante el grupo $AGENT_USER"
	fi
fi

# ── Inscripción ─────────────────────────────────────────────────────────────
step "Inscribiendo el host en el central"
"$PREFIX/em-agent" enroll --url "$URL" --token "$TOKEN" --ca-fingerprint "$FP" --config-dir "$CONFIG_DIR"
chown root:"$SVC_GROUP" "$CONFIG_DIR" "$CONFIG_DIR/credentials.toml"
chmod 0750 "$CONFIG_DIR"
chmod 0640 "$CONFIG_DIR/credentials.toml"
chmod 0644 "$CONFIG_DIR/ca.crt" "$CONFIG_DIR/em-agent.toml"

# ── Servicio systemd ────────────────────────────────────────────────────────
step "Instalando el servicio systemd"
{
	cat <<EOF
# Generado por el instalador de em-agent
[Unit]
Description=EM Monitor agent
After=network-online.target docker.service podman.socket
Wants=network-online.target

[Service]
Type=simple
User=$SVC_USER
Group=$SVC_GROUP
EOF
	[ -n "$SUPP_GROUPS" ] && echo "SupplementaryGroups=$SUPP_GROUPS"
	[ -n "$RUN_AS" ] && echo "Environment=XDG_RUNTIME_DIR=/run/user/$RUN_UID"
	cat <<EOF
ExecStart=$PREFIX/em-agent --config $CONFIG_DIR/em-agent.toml run
Restart=always
RestartSec=5

# Endurecimiento: el agente solo lee /proc, /sys, sus archivos y los sockets de los runtimes
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
RestrictSUIDSGID=yes
RestrictRealtime=yes
RestrictNamespaces=yes
LockPersonality=yes
CapabilityBoundingSet=
AmbientCapabilities=
SystemCallArchitectures=native

[Install]
WantedBy=multi-user.target
EOF
} >"$UNIT"
chmod 0644 "$UNIT"

if [ "$NO_START" -eq 1 ] || [ "$HAVE_SYSTEMD" -eq 0 ]; then
	[ "$HAVE_SYSTEMD" -eq 0 ] && log "systemd no está activo: el servicio no se inició."
	log "Para iniciarlo: systemctl daemon-reload && systemctl enable --now em-agent"
else
	systemctl daemon-reload
	systemctl enable --now em-agent.service
	sleep 3
	if systemctl is-active --quiet em-agent.service; then
		log "Servicio em-agent activo."
	else
		systemctl status --no-pager em-agent.service || true
		die "el servicio no quedó activo; revisa: journalctl -u em-agent"
	fi
fi

step "Listo"
log "Diagnóstico: sudo -u $SVC_USER $PREFIX/em-agent --config $CONFIG_DIR/em-agent.toml check"
log "Logs:        journalctl -u em-agent -f"
