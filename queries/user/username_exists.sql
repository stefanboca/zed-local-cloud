SELECT EXISTS(
    SELECT 1
    FROM local_user_profiles
    WHERE LOWER(github_login) = LOWER($1)
) AS "exists!"
