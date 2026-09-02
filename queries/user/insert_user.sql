INSERT INTO users (admin, email_address, connected_once, name)
VALUES ($1, $2, false, $3)
RETURNING id, metrics_id
