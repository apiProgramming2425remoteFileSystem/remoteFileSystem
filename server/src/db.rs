use std::time::{SystemTime, UNIX_EPOCH, Duration};

use anyhow::anyhow;
use argon2::{Argon2, PasswordHash, password_hash::{PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng}};
use sqlx::{SqlitePool, migrate::Migrator};
use tracing::{Level, instrument};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, encode};

use crate::models::{Claims, User, Xattributes};

static MIGRATOR: Migrator = sqlx::migrate!();
pub const JWT_KEY: &[u8] = b"911b7253947ddeec29e42071cdbbffd33cf316ef5a06f9ca690572a9d997711bfea1b7605d34192121bb8a9a73df5a8628c185138af09d37bd3d68b0f112e0bf";

#[derive(Debug)]
pub struct DB {
    pool: SqlitePool,
}

#[instrument(ret(level = Level::DEBUG))]
async fn hash_password(password: &str) -> anyhow::Result<String>{
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
async fn generate_token(user_id: u64) -> anyhow::Result<String>{
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
    let claims = Claims{
        user_id: user_id,
        exp: expiration_time as usize,
    };

    // 4. token encoding
    let token = encode(&header, &claims, &EncodingKey::from_secret(JWT_KEY))?;

    Ok(token)
}

#[instrument(ret(level = Level::DEBUG))]
pub fn get_token_expiration(token: &str) -> anyhow::Result<u64> {
    let decoding_key = DecodingKey::from_secret(JWT_KEY);
    let validation = Validation::new(Algorithm::HS256);
    
    let token_data = jsonwebtoken::decode::<Claims>(token, &decoding_key, &validation).map_err(|e| anyhow!(e))?;
    
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
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Database error: cannot create db directory: {}", e))?;
        }

        // prova a canonicalizzare; se fallisce usa il path così com'è
        db_path = db_path.canonicalize().unwrap_or(db_path);

        // sqlite URI richiede 'sqlite://<absolute-path>'; normalizziamo le backslash per Windows
        let mut database_url = format!("sqlite://{}", db_path.to_string_lossy());
        
        if cfg!(target_os = "windows") {
            database_url = database_url.replace("\\", "/");
        }

        let pool = SqlitePool::connect(&database_url)
            .await
            .map_err(|e| anyhow::anyhow!("Database error: impossible to connect beacause of {}", e))?;

        MIGRATOR.run(&pool).await.map_err(|e| anyhow::anyhow!("Database error: {}", e))?;
        
        Ok(DB{pool: pool})
    }

    /* -- REVOKED TOKEN MANAGEMENT */
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn insert_revoked_token(&self, user_id: i64) -> anyhow::Result<()>{
        let revocation_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time has gone behind.").as_secs() as i64;

        sqlx::query("INSERT INTO revoked_tokens VALUES(?, ?)")
            .bind(user_id)
            .bind(revocation_time)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Database error: error inserting revoked token because of {}.", e))?;
        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn remove_revoked_token(&self, user_id: i64) -> anyhow::Result<()>{
        sqlx::query("DELETE FROM revoked_tokens WHERE userID = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("Database error: error removing revoked token because of {}.", e))?;
        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn is_token_revoked(&self, user_id: i64, expiration_time: i64) -> anyhow::Result<bool> {
        let result = sqlx::query_scalar::<_, i64>("SELECT revocation_time FROM revoked_tokens WHERE userID = ?")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("Database error: error retrieving token information because of {}.", e))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time has gone behind in time!")
            .as_secs() as i64;

        match result {
            Some(revocation_time) => {
                if now > revocation_time {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => Ok(false),
        }
    }

    // -- AUTHENTICATION MANAGEMENT --
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn authenticate_user(&self, username: &str, password: &str) -> anyhow::Result<Option<String>>{
        let result = sqlx::query_as::<_, User>("SELECT userID, username, password FROM users WHERE username = ?")
            .bind(username.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("Database error: error retrieving user because of {}.", e))?;

        match result {
            Some(user) => {
                if verify_password(password, &user.password).await {
                    let token = generate_token(user.userID).await.map_err(|e| anyhow!(e))?;
                    self.remove_revoked_token(user.userID as i64).await?;
                    Ok(Some(token))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    // -- XATTRIBUTES MANAGEMENT --
    #[instrument(ret(level = Level::DEBUG))]
    pub async fn set_x_attributes(&self, path: &str, xattributes: &[u8]) -> anyhow::Result<()>{
        let result = sqlx::query_scalar::<_, u8>("SELECT COUNT(*) FROM xattributes WHERE path = ?")
                                                        .bind(path.to_string())
                                                        .fetch_one(&self.pool)
                                                        .await
                                                        .map_err(|e| anyhow!("Database error: error executing the query because of {}.", e))?;
        if result == 0{
            sqlx::query("INSERT INTO xattributes(path, xattributes) VALUES(?, ?)")
                    .bind(path.to_string())
                    .bind(xattributes.to_vec())
                    .execute(&self.pool)
                    .await
                    .map_err(|e| anyhow!("Database error: error executing the query because of {}.", e))?;
        }else{
            sqlx::query("UPDATE xattributes SET xattributes = ? WHERE path = ?")
                    .bind(xattributes.to_vec())
                    .bind(path.to_string())
                    .execute(&self.pool)
                    .await
                    .map_err(|e| anyhow!("Database error: error executing the query because of {}.", e))?;
        }

        Ok(())
    }

    #[instrument(ret(level = Level::DEBUG))]
    pub async fn get_x_attributes(&self, path: &str) -> anyhow::Result<Option<Xattributes>>{
        let result = sqlx::query_as::<_, Xattributes>("SELECT xattributes FROM xattributes WHERE path = ?")
            .bind(path.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("Database error: error retrieving user because of {}.", e))?;

        match result {
            Some(attr) => {
                Ok(Some(attr))
            }
            None => Ok(None),
        }
    }

    // -- PERMISSIONS MANAGEMENT --

    // -- CONCURRENCY MANAGEMENT --

}
