CREATE INDEX local_user_profiles_github_login_trgm_idx
ON local_user_profiles USING gin (lower(github_login) gin_trgm_ops);

CREATE INDEX local_user_profiles_name_trgm_idx
ON local_user_profiles USING gin (lower(coalesce(name, '')) gin_trgm_ops);
