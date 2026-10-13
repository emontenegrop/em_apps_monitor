-- EM Monitor — fase 7: se retira el recolector central heredado. Todos los servidores
-- se monitorean con agente; los que no tenían uno quedan esperando su inscripción.

-- El recolector heredado guardaba red y disco como contadores acumulados; con agente son
-- tasas (bytes/s). Se vacían esos valores en el historial de los servidores sin agente para
-- no mostrar cifras con otro significado. CPU, memoria y el resto se conservan.
UPDATE host_metrics h
   SET net_rx_bytes = NULL, net_tx_bytes = NULL
  FROM servers s
 WHERE s.id = h.server_id AND NOT s.uses_agent;

UPDATE container_metrics m
   SET net_rx_bytes = NULL, net_tx_bytes = NULL, block_read_bytes = NULL, block_write_bytes = NULL
  FROM servers s
 WHERE s.id = m.server_id AND NOT s.uses_agent;

UPDATE servers SET status = 'unknown' WHERE NOT uses_agent;

-- Configuración de la conexión directa (socket/TCP) y de Netdata: ya no se usa
ALTER TABLE servers
    DROP COLUMN IF EXISTS uses_agent,
    DROP COLUMN IF EXISTS host,
    DROP COLUMN IF EXISTS connection_type,
    DROP COLUMN IF EXISTS docker_host,
    DROP COLUMN IF EXISTS port,
    DROP COLUMN IF EXISTS tls_enabled,
    DROP COLUMN IF EXISTS netdata_url;
