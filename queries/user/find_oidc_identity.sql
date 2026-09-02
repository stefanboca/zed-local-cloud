SELECT user_id
FROM local_oidc_identities
WHERE issuer = $1 AND subject = $2
