# Imagen del agente para ejecutarlo en contenedor (p. ej. el host del propio central).
# Contexto de build: raíz del workspace.
#
#   docker build -f agent/Dockerfile -t em-agent .
#
# Modo contenedor: ver README ("Instalar el agente en contenedor").

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p em_agent

FROM scratch
COPY --from=build /src/target/release/em-agent /em-agent
# Configuración, CA y credencial (volumen)
VOLUME ["/etc/em-agent"]
ENTRYPOINT ["/em-agent"]
CMD ["run"]
