-- La UI necesita saber si el agente tiene una credencial vigente (inscrito y no revocado)
-- sin exponer el hash. Columna calculada: siempre consistente con agent_secret_hash.
ALTER TABLE servers
    ADD COLUMN IF NOT EXISTS agent_enrolled BOOLEAN GENERATED ALWAYS AS (agent_secret_hash IS NOT NULL) STORED;
