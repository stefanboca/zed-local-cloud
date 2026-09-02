UPDATE local_oidc_identities
SET updated_at = now()
WHERE issuer = $1 AND subject = $2
