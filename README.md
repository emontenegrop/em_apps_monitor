# em_apps_monitor

Monitor de hosts con **Docker** y **Podman**. Cada host ejecuta un agente (`em-agent`) que se conecta
**hacia** el central por HTTPS: los hosts no exponen puertos ni el socket del runtime.

```
 Host (N)                                              Central
 ┌───────────────────────────────┐   wss:// saliente   ┌──────────────────────────────────┐
 │ em-agent (systemd)            │ ──────────────────► │ nginx :8443 (TLS, CA interna)    │
 │  · CPU, RAM, red, discos      │                     │  └─ backend (Rust/axum)          │
 │  · Docker / Podman (rootless) │ ◄────────────────── │      ├─ PostgreSQL / TimescaleDB │
 │  · solo lectura               │   inspect, logs…    │      └─ UI (React)               │
 └───────────────────────────────┘                     └──────────────────────────────────┘
```

- El agente envía un snapshot cada 15 s (host, discos, contenedores, stats). Si pierde la conexión,
  guarda hasta 15 min y los reenvía al reconectar.
- El central pide bajo demanda inspect, logs, imágenes, redes y volúmenes. El agente **solo lee**:
  no puede crear, detener ni borrar contenedores.
- Los valores de las variables de entorno se ocultan en el host antes de salir (`DB_PASSWORD=***`).

## Componentes

| Ruta | Qué es |
|---|---|
| `backend/` | API, WebSocket de agentes, ingesta y autenticación (Rust, axum, sqlx) |
| `agent/` | `em-agent`: recolección, conexión al central, instalador y Dockerfiles |
| `crates/protocol/` | Tipos y mensajes compartidos agente ↔ central |
| `frontend/` | UI (React, Vite, Tailwind) servida por nginx |
| `pki/` | CA interna y certificados del central ([pki/README.md](pki/README.md)) |

## Puesta en marcha del central

Requisitos: Docker con Compose. Los pasos 1–3 se hacen una sola vez.

```bash
# 1. CA interna y certificado del central, con todas las IPs/nombres por los que lo alcanzarán los agentes
./pki/em-pki.sh init-ca
./pki/em-pki.sh server central 172.16.27.31 frontend localhost 127.0.0.1

# 2. Administrador inicial (mínimo 12 caracteres)
cp .env.example .env        # y define ADMIN_PASSWORD

# 3. Binarios del agente para los hosts (x86_64 y aarch64) → dist/agent/
./agent/build-release.sh

# 4. Levantar
docker compose up -d --build
```

La UI queda en `https://172.16.27.31:8443` (el puerto 8880 redirige a HTTPS). El navegador avisará del
certificado hasta que importes `pki/ca/ca.crt` como CA de confianza.

Después del primer ingreso cambia la contraseña en **Mi cuenta** y borra `ADMIN_PASSWORD` del `.env`.

### Variables del backend

| Variable | Por defecto | Uso |
|---|---|---|
| `ADMIN_USERNAME` / `ADMIN_PASSWORD` | `admin` / — | Administrador inicial (solo si no hay usuarios) |
| `PUBLIC_URL` | `https://172.16.27.31:8443` | URL de los comandos de instalación |
| `AGENT_CA_FILE` | `/etc/em-monitor/ca.crt` | CA que se entrega a los agentes |
| `COLLECTION_INTERVAL_SECS` | `15` | Intervalo que se indica a los agentes |
| `COOKIE_SECURE` | `true` | Cookie de sesión solo por HTTPS |
| `LEGACY_COLLECTOR` | `true` | Recolector central heredado (servidores sin agente) |

## Agregar un servidor

En la UI: **Agregar Servidor** → nombre → el asistente muestra el comando de instalación con una
**ficha de un solo uso** (vence en 1 h) y espera a que el agente se conecte.

### Host con systemd (recomendado)

Copia el comando de la pestaña **Linux (systemd)** y ejecútalo en el host. Tiene esta forma:

```bash
curl -fsSk https://172.16.27.31:8443/api/agent/ca.der -o /tmp/em-ca.der && \
echo "<huella>  /tmp/em-ca.der" | sha256sum -c - && \
{ echo '-----BEGIN CERTIFICATE-----'; base64 -w 64 /tmp/em-ca.der; echo '-----END CERTIFICATE-----'; } > /tmp/em-ca.crt && \
curl -fsS --cacert /tmp/em-ca.crt https://172.16.27.31:8443/agent/install.sh | sudo sh -s -- \
  --url https://172.16.27.31:8443 --token <ficha> --ca-fingerprint <huella> --ca-file /tmp/em-ca.crt
```

1. Descarga la CA del central y la valida **por huella** (la única descarga sin verificación TLS).
2. Descarga el instalador y el binario **validando TLS contra esa CA** y comprueba `SHA256SUMS`.
3. Crea el usuario de sistema `em-agent` y le da acceso de lectura a los runtimes:
   - Docker: grupo `docker`.
   - Podman con root: `podman.socket` abierto al grupo `em-agent` (drop-in en `/etc/systemd/system/podman.socket.d/`).
4. Inscribe el host (`/etc/em-agent/`, credencial `0640 root:em-agent`) e instala el servicio `em-agent`
   con endurecimiento de systemd (`ProtectSystem=strict`, sin capacidades, etc.).

**Podman sin root (rootless):** agrega `--run-as-user USUARIO` al final del comando. El agente corre
como ese usuario, se habilita `loginctl enable-linger` y su `podman.socket` de usuario. En cgroups v2,
la CPU por contenedor requiere delegar el controlador `cpu` al usuario (`em-agent check` lo indica).

### Contenedor

Útil para el host del propio central o donde no se quiera instalar un servicio.

```bash
docker build -f agent/Dockerfile -t em-agent .      # desde este repositorio
docker run --rm -v em-agent:/etc/em-agent em-agent enroll --url URL --token FICHA --ca-fingerprint HUELLA
docker run -d --name em-agent --restart unless-stopped --pid host \
  -e EM_AGENT_HOST_ROOT=/host -v em-agent:/etc/em-agent -v /:/host:ro \
  -v /var/run/docker.sock:/var/run/docker.sock em-agent
```

Para el host del central, el `docker-compose.yml` trae el servicio `agent` (perfil `agent`):

```bash
docker compose --profile agent run --rm agent enroll --url https://frontend --token FICHA --ca-fingerprint HUELLA
docker compose --profile agent up -d agent
```

En modo contenedor las métricas de red son las del contenedor salvo que se use `--network host`.

## Operación

| Tarea | Cómo |
|---|---|
| Reinstalar / mover el agente | Detalle del servidor → **Reinstalar (nueva ficha)**. La credencial anterior deja de valer al usar la ficha. |
| Revocar | Detalle del servidor → **Revocar**. El agente se desconecta al instante y espera una nueva inscripción. |
| Ver variables de entorno | Detalle del contenedor → **Mostrar valores** (queda registrado en el log del backend). |
| Desinstalar del host | `sudo sh install.sh --uninstall [--purge]` (el script está en `https://<central>/agent/install.sh`). |
| Renovar el certificado del central | Ver [pki/README.md](pki/README.md). Los agentes no cambian mientras la CA sea la misma. |

## Diagnóstico en el host

```bash
sudo -u em-agent em-agent --config /etc/em-agent/em-agent.toml check   # runtimes, permisos, cgroups, CA
journalctl -u em-agent -f
```

| Mensaje del agente | Causa |
|---|---|
| `la huella de la CA … no coincide` | URL equivocada o un intermediario: no se envió la ficha |
| `el central rechazó la ficha` | Ficha usada, vencida o inválida: genera otra en la UI |
| `el central revocó la credencial` | Revocada desde la UI: reinscribe el host |
| `la credencial … ya la usa un agente en otro host` | VM clonada o credencial copiada: cada host necesita su propia ficha |
| `sin permiso sobre el socket` | El usuario del agente no tiene acceso a Docker/Podman |

## Desarrollo

```bash
cargo test --workspace          # backend, agente y protocolo
cargo clippy --workspace --all-targets
cd frontend && npm ci && npm run build
em-agent collect                 # snapshot JSON local, sin central
```
