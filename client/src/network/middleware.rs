use async_trait::async_trait;
use http::Extensions;
use reqwest::{Request, Response, header};
use reqwest_middleware::{Error, Middleware, Next, Result};
use std::{fmt::Debug, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct TokenStore(Arc<RwLock<Option<String>>>);

pub struct TokenRefresher {
    // implement your refresh logic here (e.g. username/password or refresh token).
    // This could be an Arc<dyn Fn() -> Future<Output=Result<String, Error>>> etc.
}

#[derive(Default)]
pub struct AuthMiddleware {
    token_store: TokenStore,
    // optionally: a refresh callback to run on 401
    // refresh: Arc<dyn Fn() -> BoxFuture<'static, Result<String, anyhow::Error>> + Send + Sync>,
}

impl TokenStore {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(None)))
    }

    pub async fn set_token(&self, token: String) {
        let mut guard = self.0.write().await;
        *guard = Some(token);
    }

    pub async fn clear_token(&self) {
        let mut guard = self.0.write().await;
        *guard = None;
    }

    pub async fn read(&self) -> Option<String> {
        let guard = self.0.read().await;

        let Some(token) = &*guard else {
            return None;
        };

        Some(token.clone())
    }
}

impl Debug for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TokenStore").field(&"********").finish()
    }
}

impl AuthMiddleware {
    pub fn new(token_store: TokenStore) -> Self {
        Self { token_store }
    }
}

#[async_trait]
impl Middleware for AuthMiddleware {
    /// Invoked with a request before sending it. If you want to continue processing the request,
    /// you should explicitly call `next.run(req, extensions)`.
    ///
    /// If you need to forward data down the middleware stack, you can use the `extensions`
    /// argument.
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        // Read the current token and attach auth header from store if present
        if let Some(token) = self.token_store.read().await {
            // If have token, attach to request
            let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| Error::Middleware(e.into()))?;

            auth_value.set_sensitive(true);

            let mut req = req;

            req.headers_mut().insert(header::AUTHORIZATION, auth_value);
            return next.run(req, extensions).await;
        }

        // run the request
        let resp = next.run(req, extensions).await?;

        // If 401 and we have refresh logic, attempt refresh -> update store -> retry once
        /*
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // call your refresh function here, update token_store on success,
            // then clone the original request, attach new header, and call next.run again.
            // If refresh fails, return the 401 response.
        }
        */

        Ok(resp)
    }
}
