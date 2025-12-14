use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use argon2::{
    Argon2, PasswordHash,
    password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sqlx::{SqlitePool, migrate::{Migrator, MigrateDatabase}, Sqlite};
use tracing::{Level, instrument};
use uuid::Uuid;

use crate::models::{AuthenticatedUser, Claims, ListXattributes, User, Xattributes};

static MIGRATOR: Migrator = sqlx::migrate!();
pub const JWT_KEY: &[u8] = b"911b7253947ddeec29e42071cdbbffd33cf316ef5a06f9ca690572a9d997711bfea1b7605d34192121bb8a9a73df5a8628c185138af09d37bd3d68b0f112e0bf";

#[derive(Debug)]
pub struct DB {
    pool: SqlitePool,
}

#[instrument(ret(level = Level::DEBUG))]
async fn hash_password(password: &str) -> anyhow::Result<String> {
    let algorithm = Argon2::default();

    let salt = SaltString::generate(&mut OsRng);

    let password_hash = algorithm
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| anyhow!("Server error: problem during authentication!"))?
        .to_string();

    Ok(password_hash)
}

#[instrument(ret(level = Level::DEBUG))]
async fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => {
            let algorithm = Argon2::default();
            match algorithm.verify_password(password.as_bytes(), &parsed) {
                Ok(_) => true,
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

#[instrument(ret(level = Level::DEBUG))]
async fn generate_token(user_id: u64) -> anyhow::Result<String> {
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
        user_id: user_id,
        token_id: Uuid::new_v4().to_string(),
        exp: expiration_time as usize,
    };

    // 4. token encoding
    let token = encode(&header, &claims, &EncodingKey::from_secret(JWT_KEY))?;

    Ok(token)
}

#[instrument(ret(level = Level::DEBUG))]
pub fn get_expiration_time(token: &str) -> anyhow::Result<u64> {
    let decoding_key = DecodingKey::from_secret(JWT_KEY);
    let validation = Validation::new(Algorithm::HS256);

    let token_data = jsonwebtoken::decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| anyhow!(e))?;

    Ok(token_data.claims.exp as u64)
}

impl DB {
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn open_connection() -> anyhow::Result<DB> {
        // costruisce un path assoluto relativo alla root del crate
        let manifest = env!("CARGO_MANIFEST_DIR");
        let mut db_path = std::path::Path::new(manifest).join("db.db");

        // assicura che la cartella esista (utile alla prima esecuzione)
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("Database error: cannot create db directory: {}", e)
            })?;
        }

        // prova a canonicalizzare; se fallisce usa il path così com'è
        db_path = db_path.canonicalize().unwrap_or(db_path);

        // sqlite URI richiede 'sqlite://<absolute-path>'; normalizziamo le backslash per Windows
        let mut database_url = format!("sqlite://{}", db_path.to_string_lossy());

        if cfg!(target_os = "windows") {
            database_url = database_url.replace("\\", "/");
        }

        Sqlite::create_database(&database_url).await.map_err(|e| {
            anyhow::anyhow!("Database error: impossible to create database beacause of {}", e)
        })?;

        let pool = SqlitePool::connect(&database_url).await.map_err(|e| {
            anyhow::anyhow!("Database error: impossible to connect beacause of {}", e)
        })?;

        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))?;

        /* AGGIUNTA UTENTE */
        let db = DB { pool: pool };

        db.create_user("mirko", "password").await?;

        Ok(db)
    }

    /* -- REVOKED TOKEN MANAGEMENT */
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn insert_revoked_token(&self, user: &AuthenticatedUser) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO revoked_tokens VALUES(?, ?, ?)")
            .bind(user.user_id)
            .bind(user.token_id.clone())
            .bind(user.expiration_time)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                anyhow!(
                    "Database error: error inserting revoked token because of {}.",
                    e
                )
            })?;
        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn clean_revoked_token(&self) -> anyhow::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind.")
            .as_secs() as i64;

        sqlx::query("DELETE FROM revoked_tokens WHERE expiration_time = ?")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                anyhow!(
                    "Database error: error removing revoked token because of {}.",
                    e
                )
            })?;
        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn is_token_revoked(&self, user_id: i64, token_id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM revoked_tokens WHERE user_id = ? AND token_id = ?",
        )
        .bind(user_id)
        .bind(token_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            anyhow!(
                "Database error: error retrieving token information because of {}.",
                e
            )
        })?;

        if result == 1 { Ok(true) } else { Ok(false) }
    }

    pub async fn verify_token(&self, token: &str) -> anyhow::Result<AuthenticatedUser> {
        // 3. Validation and decode key configuration
        let decoding_key = DecodingKey::from_secret(JWT_KEY);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        // 4. Token decoding and verification
        let token_data = match decode::<Claims>(token, &decoding_key, &validation) {
            Ok(data) => data,
            Err(_) => {
                return Err(anyhow!("Token is invalid."));
            }
        };

        let user_id = token_data.claims.user_id as i64;
        let token_id = token_data.claims.token_id;
        let expiration_time = token_data.claims.exp as i64;

        // 5. Check if token is expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind.")
            .as_secs() as i64;
        let is_expired = now >= expiration_time;

        if is_expired {
            return Err(anyhow!("Token is expired."));
        }

        // 6. Check if the token has been revoked
        let is_revoked = match self.is_token_revoked(user_id, &token_id).await {
            Ok(flag) => flag,
            Err(_) => {
                return Err(anyhow!("Error while checking token revocation."));
            }
        };

        if is_revoked {
            return Err(anyhow!("Token has been revoked."));
        }

        Ok(AuthenticatedUser {
            user_id,
            token_id,
            expiration_time,
        })
    }

    // -- AUTHENTICATION MANAGEMENT --
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn authenticate_user(
        &self,
        username: &str,
        password: &str,
    ) -> anyhow::Result<Option<String>> {
        let result = sqlx::query_as::<_, User>(
            "SELECT user_id, username, password FROM users WHERE username = ?",
        )
        .bind(username.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("Database error: error retrieving user because of {}.", e))?;

        match result {
            Some(user) => {
                if verify_password(password, &user.password).await {
                    let token = generate_token(user.user_id).await.map_err(|e| anyhow!(e))?;
                    self.clean_revoked_token().await?;
                    Ok(Some(token))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn create_user(
        &self,
        username: &str,
        password: &str) -> anyhow::Result<()>{
            let pass = hash_password(password).await.map_err(|e| anyhow!("Error while hashing the password: {}", e))?;

            let result = sqlx::query_scalar::<_, u8>(
            "SELECT COUNT(*) FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            anyhow!(
                "Database error: error executing the query because of {}.",
                e
            )
        })?;
        
        if result == 0 {
            sqlx::query("INSERT INTO users(username, password) VALUES(?, ?)")
                .bind(username)
                .bind(pass)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    anyhow!(
                        "Database error: error executing the query because of {}.",
                        e
                    )
                })?;
        }
            Ok(())
        }
    
    // -- XATTRIBUTES MANAGEMENT --
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn set_x_attributes(
        &self,
        path: &str,
        name: &str,
        xattributes: &[u8],
    ) -> anyhow::Result<()> {
        let result = sqlx::query_scalar::<_, u8>(
            "SELECT COUNT(*) FROM xattributes WHERE path = ? AND name = ?",
        )
        .bind(path)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            anyhow!(
                "Database error: error executing the query because of {}.",
                e
            )
        })?;
        if result == 0 {
            sqlx::query("INSERT INTO xattributes(path, name, xattributes) VALUES(?, ?, ?)")
                .bind(path)
                .bind(name)
                .bind(xattributes.to_vec())
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    anyhow!(
                        "Database error: error executing the query because of {}.",
                        e
                    )
                })?;
        } else {
            sqlx::query("UPDATE xattributes SET xattributes = ? WHERE path = ? AND name = ?")
                .bind(xattributes.to_vec())
                .bind(name)
                .bind(path)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    anyhow!(
                        "Database error: error executing the query because of {}.",
                        e
                    )
                })?;
        }

        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn remove_x_attributes(&self, path: &str, name: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM xattributes WHERE path = ? AND name = ?")
            .bind(path)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                anyhow!(
                    "Database error: error updating xattrbutes table because of {}.",
                    e
                )
            })?;
        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn list_x_attributes(&self, path: &str) -> anyhow::Result<Option<ListXattributes>> {
        let result = sqlx::query_scalar::<_, String>("SELECT name FROM xattributes WHERE path = ?")
            .bind(path)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                anyhow!(
                    "Database error: error executing the query because of {}.",
                    e
                )
            })?;
        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ListXattributes { names: result }))
        }
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(
        &self,
        path: &str,
        name: &str,
    ) -> anyhow::Result<Option<Xattributes>> {
        let result = sqlx::query_as::<_, Xattributes>(
            "SELECT xattributes FROM xattributes WHERE path = ? AND name = ?",
        )
        .bind(path)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow!("Database error: error retrieving user because of {}.", e))?;

        match result {
            Some(attr) => Ok(Some(attr)),
            None => Ok(None),
        }
    }

    // -- CONCURRENCY MANAGEMENT --
}
