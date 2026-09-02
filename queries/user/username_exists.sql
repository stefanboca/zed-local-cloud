SELECT EXISTS(
    SELECT 1
    FROM local_user_profiles
    WHERE lower(github_login) = lower($1)
) AS "exists!"
