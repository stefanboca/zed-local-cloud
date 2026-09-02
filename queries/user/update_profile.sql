UPDATE local_user_profiles
SET
    avatar_url = $2,
    name = $3,
    email_address = $4,
    upstream_is_staff = false,
    updated_at = now()
WHERE user_id = $1
