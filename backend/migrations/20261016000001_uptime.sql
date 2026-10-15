-- EM Monitor — monitoreo de disponibilidad de URLs/IPs y notificaciones por correo
-- (reemplaza a qh-python-dash/url_monitor). Solo se crean las tablas: las URLs, el SMTP y
-- los destinatarios se registran desde la UI; esta migración no inserta datos.

-- URLs (HTTP/HTTPS) o hosts/IPs (ping) a verificar desde el central
CREATE TABLE IF NOT EXISTS monitored_targets (
    id                     UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    name                   VARCHAR(255) NOT NULL,
    url                    VARCHAR(512) NOT NULL UNIQUE,          -- URL (http) o host/IP (ping)
    description            TEXT,
    monitor_type           VARCHAR(10)  NOT NULL DEFAULT 'http' CHECK (monitor_type IN ('http', 'ping')),
    is_active              BOOLEAN      NOT NULL DEFAULT true,
    check_interval_secs    INTEGER      NOT NULL DEFAULT 60 CHECK (check_interval_secs BETWEEN 30 AND 3600),
    timeout_secs           INTEGER      NOT NULL DEFAULT 10 CHECK (timeout_secs BETWEEN 1 AND 60),
    expected_status_codes  VARCHAR(100) NOT NULL DEFAULT '200,201,202,301,302',  -- solo http
    alert_threshold        INTEGER      NOT NULL DEFAULT 1 CHECK (alert_threshold BETWEEN 1 AND 20),
    consecutive_failures   INTEGER      NOT NULL DEFAULT 0,
    last_status            VARCHAR(20),                           -- online | warning | offline
    last_check_at          TIMESTAMPTZ,
    last_response_ms       FLOAT8,
    created_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Resultado de cada verificación (se conservan 7 días)
CREATE TABLE IF NOT EXISTS target_checks (
    time             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    target_id        UUID         NOT NULL REFERENCES monitored_targets(id) ON DELETE CASCADE,
    is_success       BOOLEAN      NOT NULL,
    response_ms      FLOAT8,
    error_message    TEXT,
    -- http
    status_code      INTEGER,
    ssl_valid        BOOLEAN,
    ssl_expiry_days  INTEGER,
    redirect_url     VARCHAR(512),
    -- ping
    packet_loss      FLOAT8,
    ttl              INTEGER
);

CREATE INDEX IF NOT EXISTS idx_target_checks_target_time ON target_checks (target_id, time DESC);

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'timescaledb') THEN
        PERFORM create_hypertable('target_checks', 'time', if_not_exists => TRUE);
    END IF;
EXCEPTION WHEN OTHERS THEN NULL;
END $$;

-- Configuración SMTP (una sola fila, id = 1; se crea al guardar desde la UI).
-- La contraseña se guarda cifrada con AES-256-GCM (clave SETTINGS_KEY del backend).
CREATE TABLE IF NOT EXISTS notification_settings (
    id                SMALLINT     PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    enabled           BOOLEAN      NOT NULL DEFAULT false,
    smtp_host         VARCHAR(255) NOT NULL,
    smtp_port         INTEGER      NOT NULL CHECK (smtp_port BETWEEN 1 AND 65535),
    security          VARCHAR(10)  NOT NULL CHECK (security IN ('starttls', 'ssl', 'none')),
    username          VARCHAR(255) NOT NULL DEFAULT '',
    password_enc      TEXT,
    from_address      VARCHAR(255) NOT NULL,
    single_email_bcc  BOOLEAN      NOT NULL DEFAULT false,  -- un correo con todos en CCO
    updated_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Destinatarios de las alertas
CREATE TABLE IF NOT EXISTS notification_recipients (
    id          UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    email       VARCHAR(255) NOT NULL,
    name        VARCHAR(255),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_notification_recipients_email ON notification_recipients (lower(email));
