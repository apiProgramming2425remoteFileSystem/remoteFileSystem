use std::future::{Ready, ready};

use actix_web::body::MessageBody;
use actix_web::dev::{Payload, ServiceRequest, ServiceResponse};
use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};
use actix_web::{error, http::header, middleware, web};

use crate::db::DB;
use crate::models::AuthenticatedUser;

const AUTH_HEADER_PREFIX: &str = "Bearer ";

pub async fn auth_middleware(
    req: ServiceRequest,
    next: middleware::Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    // 1. Extract Authorization Header
    println!("SIAMO NEL MIDDLEWARE....");
    let auth_header = req.headers().get(header::AUTHORIZATION);

    println!("HO OTTENUTO L'HEADER");
    let token = match auth_header {
        Some(header) => {
            let header_str = header
                .to_str()
                .map_err(|_| error::ErrorBadRequest("Invalid Header"))?;

            if !header_str.starts_with(AUTH_HEADER_PREFIX) {
                return Err(error::ErrorUnauthorized("Invalid Token Format"));
            }

            // Remove the prefix to get the actual token
            &header_str[AUTH_HEADER_PREFIX.len()..]
        }
        None => {
            return Err(actix_web::error::ErrorUnauthorized(
                "Missing Authorization Header",
            ));
        }
    };

    // 2. Get access to the DB (State)
    // We need to clone the pointer (Arc) because 'req' is consumed/moved afterwards
    let pool = req
        .app_data::<web::Data<DB>>()
        .ok_or_else(|| error::ErrorInternalServerError("Database configuration error"))?
        .clone();

    // 3. Verify the token (Async call to DB)
    // Assume verify_token returns Result<AuthenticatedUser, String/Error>
    // Note: we pass the token as owned string or slice
    match pool.verify_token(token).await {
        Ok(user) => {
            // 4. SUCCESS: Insert the user into the request's Extensions
            // This allows subsequent handlers to retrieve it
            req.extensions_mut().insert(user);

            println!("FINITO ESECUZIONE");
            // Pass control to the next handler
            next.call(req).await
        }
        Err(_) => {
            // 5. FAILURE: Return 401 error
            println!("FINITO ESECUZIONE");
            Err(actix_web::error::ErrorUnauthorized(
                "Invalid or expired token",
            ))
        }

    }
}

// Assume AuthenticatedUser is Clone (necessary to pass it to the handler)
impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        // Look for the user inserted by the middleware
        match req.extensions().get::<AuthenticatedUser>() {
            Some(user) => ready(Ok(user.clone())),
            None => ready(Err(actix_web::error::ErrorUnauthorized(
                "Authentication required",
            ))),
        }
    }
}
