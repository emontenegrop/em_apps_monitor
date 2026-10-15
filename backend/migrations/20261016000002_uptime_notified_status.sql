-- Último estado del que se notificó por correo (offline | online). Una alerta queda
-- pendiente hasta entregarse: si al caer un servicio las notificaciones estaban
-- desactivadas o el SMTP falló, se reintenta en las verificaciones siguientes.
ALTER TABLE monitored_targets ADD COLUMN IF NOT EXISTS notified_status VARCHAR(20);
