-- 1. users table
CREATE TABLE users (
    userID      INTEGER PRIMARY KEY,
    username    TEXT NOT NULL,
    password    TEXT NOT NULL
);

-- 2. permissions table
CREATE TABLE user_permissions(
    userID      TEXT PRIMARY KEY,
    item_path   TEXT NOT NULL,
    permissions INTEGER NOT NULL
);