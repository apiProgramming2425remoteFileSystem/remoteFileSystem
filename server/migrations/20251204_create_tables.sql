CREATE TABLE revoked_tokens(
    userID      INTEGER PRIMARY KEY,
    expiration  INTEGER NOT NULL
);