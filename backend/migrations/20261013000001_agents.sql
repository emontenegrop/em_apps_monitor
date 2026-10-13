-- EM Monitor — agentes por host (fase 2)

-- Servidores monitoreados por agente
ALTER TABLE servers
    ADD COLUMN IF NOT EXISTS agent_secret_hash      TEXT,          -- sha256 del secreto del agente; NULL = sin agente
    ADD COLUMN IF NOT EXISTS agent_version          VARCHAR(50),
    ADD COLUMN IF NOT EXISTS agent_protocol_version INTEGER,
    ADD COLUMN IF NOT EXISTS hostname               VARCHAR(255),
    ADD COLUMN IF NOT EXISTS os                     VARCHAR(255),
    ADD COLUMN IF NOT EXISTS arch                   VARCHAR(50),
    ADD COLUMN IF NOT EXISTS runtimes               JSONB NOT NULL DEFAULT '[]',
    ADD COLUMN IF NOT EXISTS last_connected_at      TIMESTAMPTZ;

-- Un host puede tener Docker y Podman a la vez.
-- `runtime_kind` (docker | podman) no es `docker_info.runtime`, que guarda el nombre de plataforma.
ALTER TABLE containers ADD COLUMN IF NOT EXISTS runtime_kind VARCHAR(20) NOT NULL DEFAULT 'docker';
ALTER TABLE containers DROP CONSTRAINT IF EXISTS containers_pkey;
ALTER TABLE containers ADD PRIMARY KEY (server_id, runtime_kind, id);

ALTER TABLE docker_info ADD COLUMN IF NOT EXISTS runtime_kind VARCHAR(20) NOT NULL DEFAULT 'docker';
ALTER TABLE docker_info DROP CONSTRAINT IF EXISTS docker_info_pkey;
ALTER TABLE docker_info ADD PRIMARY KEY (server_id, runtime_kind);

-- Último uso de discos reportado por cada agente
CREATE TABLE IF NOT EXISTS disk_usage_latest (
    server_id       UUID         NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    mount_point     VARCHAR(500) NOT NULL,
    name            VARCHAR(500) NOT NULL,
    fs_type         VARCHAR(100) NOT NULL,
    total_bytes     BIGINT       NOT NULL,
    used_bytes      BIGINT       NOT NULL,
    available_bytes BIGINT       NOT NULL,
    usage_percent   FLOAT8       NOT NULL,
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY (server_id, mount_point)
);
