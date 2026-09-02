UPDATE users
SET admin = $2,
    email_address = $3,
    name = $4
WHERE id = $1
