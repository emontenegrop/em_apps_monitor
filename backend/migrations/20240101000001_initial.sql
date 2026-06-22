-- EM Apps Monitor — Initial Schema
-- Compatible with PostgreSQL 14+ and TimescaleDB (optional)

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Servers to monitor (local or remote)
CREATE TABLE IF NOT EXISTS servers (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name             VARCHAR(255) NOT NULL,
    host             VARCHAR(255) NOT NULL DEFAULT 'localhost',
    connection_type  VARCHAR(50)  NOT NULL DEFAULT 'local',  -- local | tcp | ssh
    docker_host      VARCHAR(500),                            -- unix:///var/run/docker.sock | tcp://host:2375
    port             INTEGER      NOT NULL DEFAULT 2375,
    tls_enabled      BOOLEAN      NOT NULL DEFAULT false,
    tags             JSONB        NOT NULL DEFAULT '[]',
    is_active        BOOLEAN      NOT NULL DEFAULT true,
    status           VARCHAR(50)  NOT NULL DEFAULT 'unknown', -- online | offline | unknown
    last_seen_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Container state (refreshed on every collection cycle)
CREATE TABLE IF NOT EXISTS containers (
    id                   VARCHAR(64)  NOT NULL,
    server_id            UUID         NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    short_id             VARCHAR(12),
    name                 VARCHAR(255),
    image                VARCHAR(500),
    image_id             VARCHAR(72),
    status               VARCHAR(100),
    state                VARCHAR(50),
    container_created_at TIMESTAMPTZ,
    ports                JSONB        NOT NULL DEFAULT '[]',
    labels               JSONB        NOT NULL DEFAULT '{}',
    networks             JSONB        NOT NULL DEFAULT '[]',
    mounts               JSONB        NOT NULL DEFAULT '[]',
    command              VARCHAR(1000),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, server_id)
);

-- Host metrics time series
CREATE TABLE IF NOT EXISTS host_metrics (
    time                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    server_id           UUID        NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    cpu_usage_percent   FLOAT8,
    memory_used_bytes   BIGINT,
    memory_total_bytes  BIGINT,
    memory_usage_percent FLOAT8,
    swap_used_bytes     BIGINT,
    swap_total_bytes    BIGINT,
    net_rx_bytes        BIGINT,
    net_tx_bytes        BIGINT,
    load_avg_1          FLOAT8,
    load_avg_5          FLOAT8,
    load_avg_15         FLOAT8,
    process_count       INTEGER,
    uptime_seconds      BIGINT
);

CREATE INDEX IF NOT EXISTS idx_host_metrics_server_time
    ON host_metrics(server_id, time DESC);

-- Container metrics time series
CREATE TABLE IF NOT EXISTS container_metrics (
    time                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    container_id         VARCHAR(64) NOT NULL,
    server_id            UUID        NOT NULL,
    cpu_usage_percent    FLOAT8,
    memory_used_bytes    BIGINT,
    memory_limit_bytes   BIGINT,
    memory_usage_percent FLOAT8,
    net_rx_bytes         BIGINT,
    net_tx_bytes         BIGINT,
    block_read_bytes     BIGINT,
    block_write_bytes    BIGINT,
    pids                 BIGINT
);

CREATE INDEX IF NOT EXISTS idx_container_metrics_container_time
    ON container_metrics(container_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_container_metrics_server_time
    ON container_metrics(server_id, time DESC);

-- Docker daemon info (latest snapshot per server)
CREATE TABLE IF NOT EXISTS docker_info (
    server_id           UUID        PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
    docker_version      VARCHAR(100),
    api_version         VARCHAR(20),
    os_type             VARCHAR(100),
    arch                VARCHAR(50),
    kernel_version      VARCHAR(100),
    containers_total    INTEGER,
    containers_running  INTEGER,
    containers_paused   INTEGER,
    containers_stopped  INTEGER,
    images_total        INTEGER,
    storage_driver      VARCHAR(100),
    runtime             VARCHAR(100),
    docker_root_dir     VARCHAR(500),
    swarm_active        BOOLEAN     DEFAULT false,
    raw_info            JSONB,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed local server
INSERT INTO servers (name, host, connection_type, docker_host, port)
VALUES ('Local', 'localhost', 'local', 'unix:///var/run/docker.sock', 2375)
ON CONFLICT DO NOTHING;

-- TimescaleDB hypertables (safe no-op if extension not present)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb') THEN
        PERFORM create_hypertable('host_metrics',       'time', if_not_exists => TRUE);
        PERFORM create_hypertable('container_metrics',  'time', if_not_exists => TRUE);
        RAISE NOTICE 'TimescaleDB hypertables created.';
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;
