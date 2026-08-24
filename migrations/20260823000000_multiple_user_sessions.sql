ALTER TABLE local_user_tokens
DROP CONSTRAINT local_user_tokens_pkey;

ALTER TABLE local_user_tokens
ADD PRIMARY KEY (user_id, access_token_hash);
