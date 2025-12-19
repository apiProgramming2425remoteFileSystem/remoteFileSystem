use std::fs;

use fuse3::MountOptions;
use fuse3::path::prelude::*;
use tokio::signal;

use client::error::ClientError;
use client::fuse::Fs;
use client::{config, error, logging, network};

type Result<T> = std::result::Result<T, error::ClientError>;

fn main() -> Result<()> {
    // load configuration from args/env
    let config = config::Config::from_args()?;

    // Run the client with the provided configuration
    client::start(&config)?;

    Ok(())
}
