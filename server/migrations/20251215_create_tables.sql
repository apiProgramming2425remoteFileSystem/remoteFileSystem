-- 1. users
CREATE TABLE IF NOT EXISTS users (
    user_id     INTEGER PRIMARY KEY,
    username    TEXT NOT NULL UNIQUE,
    password    TEXT NOT NULL
);

-- 2. revoked tokens
CREATE TABLE IF NOT EXISTS revoked_tokens(
    user_id     INTEGER NOT NULL,
    token_id    TEXT NOT NULL,
    expiration_time INTEGER NOT NULL, 
    PRIMARY KEY (user_id, token_id)
);

-- 3. xattributes
CREATE TABLE IF NOT EXISTS xattributes(
    path        TEXT NOT NULL,
    name        TEXT NOT NULL,
    xattributes BLOB NOT NULL,
    PRIMARY KEY (path, name)
);