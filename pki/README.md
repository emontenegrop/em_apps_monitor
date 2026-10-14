# CA interna de EM Monitor

Los agentes se conectan al central por `wss://` y validan su certificado contra esta CA.
No hace falta un dominio público ni Let's Encrypt.

## Puesta en marcha (una sola vez)

```bash
# 1. Crear la CA raíz (EC P-384, 10 años)
./pki/em-pki.sh init-ca

# 2. Emitir el certificado del central con TODOS los nombres/IPs por los que lo alcanzarán los agentes
./pki/em-pki.sh server central monitor.midominio.local 10.0.0.5

# 3. Levantar el stack: nginx toma pki/issued/central.{crt,key} y escucha en 8443 (HTTPS)
docker compose up -d --build
```

## Archivos

| Archivo | Uso | ¿Sale de la máquina? |
|---|---|---|
| `pki/ca/ca.key` | Firma certificados | **Nunca.** Respaldo cifrado fuera de línea. |
| `pki/ca/ca.crt` | Los agentes confían en ella | Sí, se distribuye a cada host |
| `pki/issued/central.key` | Clave TLS del central | Solo al contenedor nginx |
| `pki/issued/central.crt` | Certificado TLS del central | Sí (lo presenta nginx) |

`pki/ca/` y `pki/issued/` están en `.gitignore` y `.dockerignore`.

## Confianza en los agentes

El comando de instalación incluirá la huella SHA-256 de la CA:

```bash
./pki/em-pki.sh fingerprint
```

El instalador descarga `ca.crt` del central, verifica que su huella coincida con la del comando
y lo guarda en `/etc/em-agent/ca.crt`. A partir de ahí el agente solo acepta el certificado del central
si lo firmó esta CA.

## Renovación

El certificado del central dura 397 días. Para renovarlo:

```bash
./pki/em-pki.sh server central monitor.midominio.local 10.0.0.5   # sobrescribe pki/issued/central.*
docker compose restart frontend
```

Los agentes no necesitan cambios mientras la CA sea la misma. La CA dura 10 años.
Rotarla obliga a redistribuir `ca.crt` a todos los hosts.
