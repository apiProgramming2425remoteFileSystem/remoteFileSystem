use server::app;
use server::error::RfsServerError;

type Result<T> = std::result::Result<T, RfsServerError>;

#[actix_web::main]
async fn main() -> Result<()> {
    app::run().await?;
    Ok(())
}
