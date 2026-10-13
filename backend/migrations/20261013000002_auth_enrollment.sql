-- EM Monitor — fase 4: administradores, sesiones e inscripción de agentes

CREATE TABLE IF NOT EXISTS users (
    id            UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    username      VARCHAR(100) NOT NULL UNIQUE,
    password_hash TEXT         NOT NULL,              -- argon2id (PHC)
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sessions (
    token_hash   TEXT        PRIMARY KEY,             -- sha256 del token de la cookie
    user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

ALTER TABLE servers
    -- El servidor lo monitorea un agente (aunque su credencial esté revocada).
    -- Evita que el recolector central lo tome al revocar la credencial.
    ADD COLUMN IF NOT EXISTS uses_agent            BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS enrollment_token_hash TEXT,
    ADD COLUMN IF NOT EXISTS enrollment_expires_at TIMESTAMPTZ;

UPDATE servers SET uses_agent = true WHERE agent_secret_hash IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_servers_enrollment_token
    ON servers(enrollment_token_hash) WHERE enrollment_token_hash IS NOT NULL;
