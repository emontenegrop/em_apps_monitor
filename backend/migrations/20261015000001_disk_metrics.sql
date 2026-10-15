-- EM Monitor — historial del uso de disco por servidor (suma de sus sistemas de archivos).
-- disk_usage_latest solo guarda el último valor; esta serie alimenta las gráficas del Dashboard.

CREATE TABLE IF NOT EXISTS disk_metrics (
    time         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    server_id    UUID        NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    used_bytes   BIGINT      NOT NULL,
    total_bytes  BIGINT      NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_disk_metrics_server_time ON disk_metrics (server_id, time DESC);

-- TimescaleDB hypertable (no-op si la extensión no está)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb') THEN
        PERFORM create_hypertable('disk_metrics', 'time', if_not_exists => TRUE);
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;
