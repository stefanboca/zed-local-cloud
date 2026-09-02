INSERT INTO local_user_profiles (
    user_id,
    github_login,
    avatar_url,
    name,
    email_address,
    metrics_id,
    upstream_is_staff
)
VALUES ($1, $2, $3, $4, $5, $6, false)
