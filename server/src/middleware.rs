use std::future::{Ready, ready};

use actix_web::body::MessageBody;
use actix_web::dev::{Payload, ServiceRequest, ServiceResponse};
use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};
use actix_web::{http::header, middleware, web};

use crate::api_err;
use crate::db::DB;
use crate::error::ApiError;
use crate::models::AuthenticatedUser;

const AUTH_HEADER_PREFIX: &str = "Bearer ";

pub async fn auth_middleware(
    req: ServiceRequest,
    next: middleware::Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    // 1. Extract Authorization Header
    let auth_header = req.headers().get(header::AUTHORIZATION);

    let token = match auth_header {
        Some(header) => {
            let Ok(header_str) = header.to_str() else {
                tracing::error!("Invalid Authorization header format");
                return Err(api_err!(InvalidInput, "Invalid Header").into());
            };

            if !header_str.starts_with(AUTH_HEADER_PREFIX) {
                tracing::error!("Invalid Authorization header prefix");
                return Err(api_err!(Unauthorized, "Invalid Token Format").into());
            }

            // Remove the prefix to get the actual token
            &header_str[AUTH_HEADER_PREFIX.len()..]
        }
        None => {
            tracing::error!("Missing Authorization header");
            return Err(api_err!(Unauthorized, "Missing Authorization Header").into());
        }
    };

    // 2. Get access to the DB (State)
    // We need to clone the pointer (Arc) because 'req' is consumed/moved afterwards
    let pool = req
        .app_data::<web::Data<DB>>()
        .ok_or_else(|| {
            tracing::error!("Database configuration error");
            ApiError::InternalError("Database configuration error".to_string())
        })?
        .clone();

    // 3. Verify the token (Async call to DB)
    match pool.verify_token(token).await {
        Ok(user) => {
            // 4. SUCCESS: Insert the user into the request's Extensions
            // This allows subsequent handlers to retrieve it
            req.extensions_mut().insert(user);
            tracing::debug!("Token verified successfully");

            // Pass control to the next handler
            next.call(req).await
        }
        Err(_) => {
            // 5. FAILURE: Return 401 error
            tracing::debug!("Token verification failed");
            Err(api_err!(Unauthorized, "Invalid or expired token").into())
        }
    }
}

// Assume AuthenticatedUser is Clone (necessary to pass it to the handler)
impl FromRequest for AuthenticatedUser {
    type Error = ApiError;
    type Future = Ready<Result<Self, ApiError>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        // Look for the user inserted by the middleware
        match req.extensions().get::<AuthenticatedUser>() {
            Some(user) => ready(Ok(user.clone())),
            None => ready(Err(api_err!(Unauthorized, "Authentication required",))),
        }
    }
}
