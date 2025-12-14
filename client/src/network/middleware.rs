use async_trait::async_trait;
use http::Extensions;
use reqwest::{Request, Response, header};
use reqwest_middleware::{Middleware, Next, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type TokenStore = Arc<RwLock<Option<String>>>;

pub struct TokenRefresher {
    // implement your refresh logic here (e.g. username/password or refresh token).
    // This could be an Arc<dyn Fn() -> Future<Output=Result<String, Error>>> etc.
}

pub struct AuthMiddleware {
    token_store: TokenStore,
    // optionally: a refresh callback to run on 401
    // refresh: Arc<dyn Fn() -> BoxFuture<'static, Result<String, anyhow::Error>> + Send + Sync>,
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
        // Read the current token
        let token_guard = self.token_store.read().await;

        // attach auth header from store if present
        if let Some(token) = &*token_guard {
            // If have token, attach to request
            let mut auth_value = header::HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|e| reqwest_middleware::Error::Middleware(anyhow::anyhow!(e)))?;
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
