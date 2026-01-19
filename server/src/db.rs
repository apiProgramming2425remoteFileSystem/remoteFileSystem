use std::fmt::Debug;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use argon2::{
    Argon2, PasswordHash,
    password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use nix::libc::user;
use sqlx::{
    Sqlite, SqlitePool,
    migrate::{MigrateDatabase, Migrator},
};
use tracing::{Level, instrument};
use uuid::Uuid;

use crate::error::DatabaseError;
use crate::models::{AuthenticatedUser, Claims, ListXattributes, PartialUser, User, Xattributes};

type Result<T> = std::result::Result<T, DatabaseError>;

static MIGRATOR: Migrator = sqlx::migrate!();
pub const JWT_KEY: &[u8] = b"911b7253947ddeec29e42071cdbbffd33cf316ef5a06f9ca690572a9d997711bfea1b7605d34192121bb8a9a73df5a8628c185138af09d37bd3d68b0f112e0bf";

#[derive(Debug)]
pub struct DB {
    pool: SqlitePool,
}

// TODO: create error types for hashing and token generation

#[instrument(skip(password), err(level = Level::ERROR))]
async fn hash_password(password: &str) -> anyhow::Result<String> {
    let algorithm = Argon2::default();

    let salt = SaltString::generate(&mut OsRng);

    let password_hash = algorithm
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| anyhow!("Server error: problem during authentication!"))?
        .to_string();

    Ok(password_hash)
}

#[instrument(skip(password, hash), ret(level = Level::DEBUG))]
async fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => {
            let algorithm = Argon2::default();
            algorithm
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        }
        Err(_) => false,
    }
}

#[instrument(err(level = Level::ERROR))]
async fn generate_token(user_id: i64, group_id: i64) -> anyhow::Result<String> {
    // 1. Compute expiration time
    let expiration_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time has gone behind in time!")
        .as_secs()
        .checked_add(24u64 * 60u64 * 60u64)
        .unwrap_or(0);

    // 2. set header, specifying used algorithm
    let header = Header::new(Algorithm::HS256);

    // 3. create payload
    let claims = Claims {
        user_id,
        group_id,
        token_id: Uuid::new_v4().to_string(),
        exp: expiration_time as usize,
    };

    // 4. token encoding
    let token = encode(&header, &claims, &EncodingKey::from_secret(JWT_KEY))?;

    Ok(token)
}

#[instrument(skip(token), err(level = Level::ERROR), ret(level = Level::DEBUG))]
pub fn get_expiration_time(token: &str) -> anyhow::Result<u64> {
    let decoding_key = DecodingKey::from_secret(JWT_KEY);
    let validation = Validation::new(Algorithm::HS256);

    let token_data = jsonwebtoken::decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| anyhow!(e))?;

    Ok(token_data.claims.exp as u64)
}

impl DB {
    // TODO: if we want to allow custom DB path, config: &Config should be passed here

    /// Open a new database connection, applying migrations if necessary.
    /// Returns the database connection or an error.
    #[instrument(err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn open_connection<P: AsRef<Path> + Debug>(database_path: P) -> Result<Self> {
        // // costruisce un path assoluto relativo alla root del crate
        // let manifest = env!("CARGO_MANIFEST_DIR");
        let mut db_path = database_path.as_ref().to_path_buf();

        // assicura che la cartella esista (utile alla prima esecuzione)
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DatabaseError::CreationError(format!("Cannot create db directory: {}", e))
            })?;
        }

        // prova a canonicalizzare; se fallisce usa il path così com'è
        db_path = db_path.canonicalize().unwrap_or(db_path);

        // REVIEW: is not necessary to use sqlite URI format
        let database_url = db_path.to_string_lossy();

        /*
        // sqlite URI richiede 'sqlite://<absolute-path>'; normalizziamo le backslash per Windows
        let mut database_url = format!("sqlite://{}", db_path.to_string_lossy());

        if cfg!(target_os = "windows") {
            database_url = database_url.replace("\\", "/");
        }
        */

        Sqlite::create_database(&database_url)
            .await
            .map_err(|e| DatabaseError::CreationError(e.to_string()))?;

        let pool = SqlitePool::connect(&database_url)
            .await
            .map_err(|e| DatabaseError::ConnectionError(e.to_string()))?;

        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| DatabaseError::MigrationError(e.to_string()))?;


        // TODO: add them via CLI command or with a setup script
        /* 
        let db = Self { pool };
        db.create_user(1, 1, "mirko", "password").await?;
        db.create_user(2, 2, "fabrizio", "password").await?;
        db.create_user(3, 3, "iulian", "password").await?;
        db.create_user(4, 4, "test_user", "test_password").await?;
        Ok(db)
        */

        Ok( Self { pool })
    }

    /* -- REVOKED TOKEN MANAGEMENT */
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn insert_revoked_token(&self, user: &AuthenticatedUser) -> Result<()> {
        sqlx::query("INSERT INTO revoked_tokens VALUES(?, ?, ?)")
            .bind(user.user_id)
            .bind(user.token_id.clone())
            .bind(user.expiration_time)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryError(format!(
                    "Error inserting revoked token because of {}.",
                    e
                ))
            })?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn clean_revoked_token(&self) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind.")
            .as_secs() as i64;

        sqlx::query("DELETE FROM revoked_tokens WHERE expiration_time = ?")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryError(format!("Error removing revoked token because of {}.", e))
            })?;
        Ok(())
    }

    #[instrument(skip(self, token_id), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn is_token_revoked(&self, user_id: i64, token_id: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM revoked_tokens WHERE user_id = ? AND token_id = ?",
        )
        .bind(user_id)
        .bind(token_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!(
                "Error retrieving token information because of {}.",
                e
            ))
        })?;

        if result == 1 { Ok(true) } else { Ok(false) }
    }

    #[instrument(skip(self, token), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn verify_token(&self, token: &str) -> Result<AuthenticatedUser> {
        // 3. Validation and decode key configuration
        let decoding_key = DecodingKey::from_secret(JWT_KEY);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        // 4. Token decoding and verification
        let token_data = match decode::<Claims>(token, &decoding_key, &validation) {
            Ok(data) => data,
            Err(_) => return Err(DatabaseError::Other(anyhow!("Token is invalid."))),
        };

        let user_id = token_data.claims.user_id as i64;
        let group_id = token_data.claims.group_id as i64;
        let token_id = token_data.claims.token_id;
        let expiration_time = token_data.claims.exp as i64;

        // 5. Check if token is expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind.")
            .as_secs() as i64;
        let is_expired = now >= expiration_time;

        if is_expired {
            return Err(DatabaseError::Other(anyhow!("Token is expired.")));
        }

        // 6. Check if the token has been revoked
        let is_revoked = match self.is_token_revoked(user_id, &token_id).await {
            Ok(flag) => flag,
            Err(_) => {
                return Err(DatabaseError::Other(anyhow!(
                    "Error while checking token revocation."
                )));
            }
        };

        if is_revoked {
            return Err(DatabaseError::Other(anyhow!("Token has been revoked.")));
        }

        Ok(AuthenticatedUser {
            user_id,
            group_id,
            token_id,
            expiration_time,
        })
    }

    // -- AUTHENTICATION MANAGEMENT --
    #[instrument(skip(self, password), err(level = Level::ERROR))]
    pub async fn authenticate_user(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<String>> {
        let result = sqlx::query_as::<_, User>(
            "SELECT user_id, group_id, username, password FROM users WHERE username = ?",
        )
        .bind(username.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Error retrieving user because of {}.", e))
        })?;

        match result {
            Some(user) => {
                if verify_password(password, &user.password).await {
                    let token = generate_token(user.user_id, user.group_id).await?;
                    self.clean_revoked_token().await?;
                    Ok(Some(token))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn count_user_id(&self, user_id: i64) -> Result<u8> {
        let count = sqlx::query_scalar::<_, u8>("SELECT COUNT(*) FROM users WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;
        Ok(count)
    }

    #[instrument(skip(self, password), err(level = Level::ERROR))]
    pub async fn create_user(
        &self,
        user_id: i64,
        group_id: i64,
        username: &str,
        password: &str,
    ) -> Result<()> {
        let count_user_id = self.count_user_id(user_id).await?;

        if count_user_id != 0 {
            return Err(DatabaseError::QueryError(format!(
                "User {} already exists!",
                user_id
            )));
        }

        let pass = hash_password(password)
            .await
            .map_err(|e| anyhow!("Error while hashing the password: {}", e))?;

        sqlx::query("INSERT INTO users(user_id, group_id, username, password) VALUES(?, ?, ?, ?)")
            .bind(user_id)
            .bind(group_id)
            .bind(username)
            .bind(pass)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_user(&self, username: &str) -> Result<Option<PartialUser>> {
        let result = sqlx::query_as::<_, PartialUser>(
            "SELECT user_id, group_id, username, password FROM users WHERE username = ?",
        )
        .bind(username.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            DatabaseError::QueryError(format!("Error retrieving user because of {}.", e))
        })?;

        match result {
            Some(user) => Ok(Some(user)),
            None => Ok(None),
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn delete_user(&self, user_id: i64) -> Result<()> {
        let count_user_id = self.count_user_id(user_id).await?;

        if count_user_id == 0 {
            return Err(DatabaseError::QueryError(format!(
                "User {} does not exist!",
                user_id
            )));
        }

        sqlx::query("DELETE FROM users WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn edit_username(&self, user_id: i64, username: &str) -> Result<()> {
        let count_user_id = self.count_user_id(user_id).await?;

        if count_user_id == 0 {
            return Err(DatabaseError::QueryError(format!(
                "User {} does not exist!",
                user_id
            )));
        }

        let existing_user = self.get_user(username).await?;
        if existing_user.is_some() {
            return Err(DatabaseError::QueryError(format!(
                "Username '{}' is already taken!",
                username
            )));
        }

        sqlx::query("UPDATE users SET username = ? WHERE user_id = ? ")
            .bind(username)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        let existing_user = self.get_user(username).await?;
        if existing_user.is_some() {
            return Err(DatabaseError::QueryError(format!(
                "Username '{}' is already taken!",
                username
            )));
        }

        sqlx::query("UPDATE users SET username = ? WHERE user_id = ? ")
            .bind(username)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self, password), err(level = Level::ERROR))]
    pub async fn edit_password(&self, user_id: i64, password: &str) -> Result<()> {
        let count_user_id = self.count_user_id(user_id).await?;

        if count_user_id == 0 {
            return Err(DatabaseError::QueryError(format!(
                "User {} does not exist!",
                user_id
            )));
        }

        let pass = hash_password(password)
            .await
            .map_err(|e| anyhow!("Error while hashing the password: {}", e))?;

        sqlx::query("UPDATE users SET password = ? WHERE user_id = ?")
            .bind(pass)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn edit_group_id(&self, user_id: i64, group_id: i64) -> Result<()> {
        let count_user_id = self.count_user_id(user_id).await?;

        if count_user_id == 0 {
            sqlx::query("UPDATE users WHERE user_id = ? SET group_id = ?")
                .bind(user_id)
                .bind(group_id)
                .execute(&self.pool)
                .await
                .map_err(|e| DatabaseError::QueryError(e.to_string()))?;
            println!("User {} username modified successfully!", user_id);
        } else {
            println!("User {} does not exist!", user_id);
        }

        Ok(())
    }

    // -- XATTRIBUTES MANAGEMENT --
    /* GESTIRE PERMESSI */
    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn set_x_attributes(&self, path: &str, name: &str, xattributes: &[u8]) -> Result<()> {
        let result = sqlx::query_scalar::<_, u8>(
            "SELECT COUNT(*) FROM xattributes WHERE path = ? AND name = ?",
        )
        .bind(path)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        if result == 0 {
            sqlx::query("INSERT INTO xattributes(path, name, xattributes) VALUES(?, ?, ?)")
                .bind(path)
                .bind(name)
                .bind(xattributes.to_vec())
                .execute(&self.pool)
                .await
                .map_err(|e| DatabaseError::QueryError(e.to_string()))?;
        } else {
            sqlx::query("UPDATE xattributes SET xattributes = ? WHERE path = ? AND name = ?")
                .bind(xattributes.to_vec())
                .bind(name)
                .bind(path)
                .execute(&self.pool)
                .await
                .map_err(|e| DatabaseError::QueryError(e.to_string()))?;
        }

        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR))]
    pub async fn remove_x_attributes(&self, path: &str, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM xattributes WHERE path = ? AND name = ?")
            .bind(path)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryError(format!(
                    "Error updating xattrbutes table because of {}.",
                    e
                ))
            })?;
        Ok(())
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn list_x_attributes(&self, path: &str) -> Result<Option<ListXattributes>> {
        let result = sqlx::query_scalar::<_, String>("SELECT name FROM xattributes WHERE path = ?")
            .bind(path)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        // REVIEW: is not necessary to return None if empty, because then use unwrap_or_default where default is empty list
        // Ok(ListXattributes { names: result }) // change the return type to Result<ListXattributes>

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ListXattributes { names: result }))
        }
    }

    #[instrument(skip(self), err(level = Level::ERROR), ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(&self, path: &str, name: &str) -> Result<Option<Xattributes>> {
        let result = sqlx::query_as::<_, Xattributes>(
            "SELECT xattributes FROM xattributes WHERE path = ? AND name = ?",
        )
        .bind(path)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryError(e.to_string()))?;

        match result {
            Some(attr) => Ok(Some(attr)),
            None => Ok(None),
        }
    }

    // -- CONCURRENCY MANAGEMENT --
}
