use async_trait::async_trait;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use serde_json::Value;

use crate::app::Executable;
use crate::config::{DEFAULT_CONFIG_FILE, ENV_PREFIX, ENV_SEPARATOR};
use crate::config::{Formatter, RfsConfig};
use crate::db::DB;
use crate::error::CommandError;
use crate::util;

/// Arguments for the `UserCreate` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct UserCreateCommand {
    /// Username for the new user
    #[arg(short, long)]
    pub username: String,

    /// Password for the new user
    #[arg(short, long)]
    pub password: String,

    /// Optional user ID for the new user
    /// If not provided, the system will assign one automatically.
    #[arg(long = "uid")]
    pub user_id: Option<u32>,

    /// Optional group ID for the new user
    /// If not provided, the user will be assigned to its own group.
    #[arg(long = "gid")]
    pub group_id: Option<u32>,
}

#[async_trait]
impl Executable for UserCreateCommand {
    type Error = CommandError;

    async fn execute_with_db(&self, db: DB) -> Result<(), Self::Error> {
        println!("Creating user: {}", self.username);
        let (uid, gid) = db
            .create_user(&self.username, &self.password, self.user_id, self.group_id)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;
        println!(
            "User '{}' created successfully with user_id {} and group_id {}.",
            self.username, uid, gid
        );
        Ok(())
    }
}

/// Arguments for the `UserChangeUsername` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct UserChangeUsernameCommand {
    /// Current username of the user
    #[arg(short, long)]
    pub current_username: String,

    /// New username for the user
    #[arg(short, long)]
    pub new_username: String,
}

#[async_trait]
impl Executable for UserChangeUsernameCommand {
    type Error = CommandError;

    async fn execute_with_db(&self, db: DB) -> Result<(), Self::Error> {
        let user_opt = db
            .get_user(&self.current_username)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;

        let Some(user) = user_opt else {
            return Err(CommandError::ExecutionFailed(format!(
                "User '{}' not found.",
                self.current_username
            )));
        };

        println!(
            "Changing username from '{}' to '{}'",
            self.current_username, self.new_username
        );
        db.edit_username(user.user_id, &self.new_username)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;
        println!("Username changed successfully to '{}'.", self.new_username);
        Ok(())
    }
}

/// Arguments for the `UserChangePassword` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct UserChangePasswordCommand {
    /// Username of the user whose password is to be changed
    #[arg(short, long)]
    pub username: String,

    /// New password for the user
    #[arg(short, long)]
    pub new_password: String,
}

#[async_trait]
impl Executable for UserChangePasswordCommand {
    type Error = CommandError;

    async fn execute_with_db(&self, db: DB) -> Result<(), Self::Error> {
        let user_opt = db
            .get_user(&self.username)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;

        let Some(user) = user_opt else {
            return Err(CommandError::ExecutionFailed(format!(
                "User '{}' not found.",
                self.username
            )));
        };

        println!("Changing password for user: {}", self.username);
        db.edit_password(user.user_id, &self.new_password)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;
        println!("User '{}' password changed successfully.", self.username);
        Ok(())
    }
}

/// Arguments for the `UserDelete` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct UserDeleteCommand {
    /// Username of the user to delete
    #[arg(short, long)]
    pub username: String,
}

#[async_trait]
impl Executable for UserDeleteCommand {
    type Error = CommandError;

    async fn execute_with_db(&self, db: DB) -> Result<(), Self::Error> {
        let user_opt = db
            .get_user(&self.username)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;

        let Some(user) = user_opt else {
            return Err(CommandError::ExecutionFailed(format!(
                "User '{}' not found.",
                self.username
            )));
        };

        println!("Deleting user: {}", self.username);
        db.delete_user(user.user_id)
            .await
            .map_err(|err| CommandError::ExecutionFailed(err.to_string()))?;
        println!("User '{}' deleted successfully.", self.username);
        Ok(())
    }
}

/// Arguments for the `GenerateConfig` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct TomlConfigGenerator {
    /// Output path for the generated configuration file
    #[arg(short, long, default_value = DEFAULT_CONFIG_FILE)]
    pub output: PathBuf,

    /// Overwrite existing file if it exists
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// Whether to include default values in the generated configuration file
    #[arg(short, long, default_value_t = false)]
    pub default: bool,
}

#[async_trait]
impl Executable for TomlConfigGenerator {
    type Error = CommandError;

    /// Generate a default configuration file at the specified output path.
    /// If the file already exists and `force` is false, an error is returned.
    fn execute(&self) -> Result<(), Self::Error> {
        let mut output = util::normalize_path(&self.output);
        // Ensure the output file has .toml extension
        output = ensure_extension(&output, "toml")?;

        // Check if we can generate the default TOML
        if output.exists() && !self.force {
            return Err(CommandError::ExecutionFailed(format!(
                "File {:?} already exists. Use --force to replace it.",
                output
            )));
        }

        let cfg = RfsConfig::default();

        // Serialize to TOML and write to file
        let config_toml = TomlFormatter.format(&cfg).map_err(|err| {
            CommandError::ExecutionFailed(format!("Failed to generate default TOML: {}", err))
        })?;

        fs::write(&output, config_toml).map_err(|err| {
            CommandError::ExecutionFailed(format!("Failed to write {:?} to file: {}", output, err))
        })?;
        println!(
            "Default configuration successfully generated at: {:?}",
            output
        );
        Ok(())
    }
}

/// Arguments for the `EnvTemplate` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct EnvVarGenerator {
    /// Output path for the generated environment variable template
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Prefix for environment variables
    #[arg(short, long, default_value = ENV_PREFIX)]
    pub prefix: String,

    /// Separator for nested fields in environment variables
    #[arg(short, long, default_value = ENV_SEPARATOR)]
    pub separator: String,
}

impl EnvVarGenerator {
    // Recursive function to navigate the tree
    fn walk(&self, node: &Value, current_path: Option<String>) -> String {
        let mut output = String::new();
        match node {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_path = match current_path {
                        Some(ref path) => Some(format!("{}{}{}", path, self.separator, key)),
                        None => Some(key.to_string()),
                    };
                    output.push_str(&self.walk(val, new_path));
                }
            }
            Value::Array(values) => {
                // For arrays, we can represent them as comma-separated values
                let array_values = values
                    .iter()
                    .map(|v| self.walk(v, None))
                    .collect::<Vec<String>>()
                    .join(",");

                if let Some(array_path) = current_path {
                    output.push_str(&format!("{}={}\n", array_path.to_uppercase(), array_values)); // Array at root
                } else {
                    output.push_str(&array_values); // Array at root without a key
                }
            }
            // If it's a leaf value (String, Number, Bool, or Null), print the variable
            _ => {
                // If it's null, use a placeholder
                let example_val = if node.is_null() {
                    "..."
                } else {
                    node.as_str().unwrap_or("...")
                };

                // Handle numbers/booleans that as_str() would return None for
                let val_string = if node.is_number() || node.is_boolean() {
                    node.to_string()
                } else {
                    example_val.to_string()
                };

                match current_path {
                    Some(path) => {
                        output.push_str(&format!("{}={}\n", path.to_uppercase(), val_string))
                    } // Leaf at root
                    None => output.push_str(&val_string),
                }
            }
        }

        output
    }
}

#[async_trait]
impl Executable for EnvVarGenerator {
    type Error = CommandError;

    fn execute(&self) -> Result<(), Self::Error> {
        let instance = RfsConfig::default();

        // Converts the struct into a generic JSON Value
        let value = serde_json::to_value(instance).unwrap_or(Value::Null);
        let mut output = String::new();

        output.push_str("# Supported Environment Variables:\n");

        // Start the recursion using the base prefix
        output.push_str(&self.walk(&value, Some(self.prefix.to_string())));

        // Output to file or stdout
        if let Some(ref path) = self.output {
            let mut output_path = util::normalize_path(path);
            // Ensure the output file has .env extension
            output_path = ensure_extension(&output_path, "env")?;

            fs::write(&output_path, output).map_err(|err| {
                CommandError::ExecutionFailed(format!(
                    "Failed to write environment template to {:?}: {}",
                    output_path, err
                ))
            })?;
            println!(
                "Environment variable template successfully generated at: {:?}",
                output_path
            );
        } else {
            // Print to stdout
            println!("{}", output);
        }

        Ok(())
    }
}

pub struct TomlFormatter;

impl Formatter for TomlFormatter {
    fn format<T: Serialize>(&self, value: &T) -> std::result::Result<String, String> {
        toml::to_string_pretty(value).map_err(|err| format!("Failed to serialize to TOML: {}", err))
    }
}

/// Helper to ensure the extension of a path is as specified.
/// If the path already has the specified extension, it is returned unchanged.
/// Returns an error if the path has a different extension.
/// Otherwise, the extension is set to the specified one.
fn ensure_extension<P: AsRef<Path>>(path: &P, ext: &str) -> Result<PathBuf, CommandError> {
    let path = path.as_ref();

    if let Some(extension) = path.extension() {
        if extension == ext {
            return Ok(path.to_path_buf());
        } else {
            return Err(CommandError::ExecutionFailed(format!(
                "File {:?} has a different extension ({:?}) than the required one (.{})",
                path, extension, ext
            )));
        }
    }

    // If it doesn't have the extension, append it.
    let mut new_path = path.to_path_buf();
    new_path.set_extension(ext);
    Ok(new_path)
}
