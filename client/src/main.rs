use client::app;
use client::error::RfsClientError;

type Result<T> = std::result::Result<T, RfsClientError>;

fn main() -> Result<()> {
    app::run()?;
    Ok(())
}
