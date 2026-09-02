SELECT EXISTS(
    SELECT 1
    FROM local_user_tokens
    WHERE user_id = $1 AND access_token_hash = $2
) AS "exists!"
