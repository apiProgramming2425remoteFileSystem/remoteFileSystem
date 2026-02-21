use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use serde_json::Value;

use crate::app::Executable;
use crate::config::{DEFAULT_CONFIG_FILE, ENV_PREFIX, ENV_SEPARATOR};
use crate::config::{Formatter, RfsConfig};
use crate::error::CommandError;
use crate::util;

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

#[derive(Debug, Clone, Parser)]
pub struct CliUnmountArgs {
    /// Mount point to unmount
    pub mount_point: PathBuf,
}

impl Executable for CliUnmountArgs {
    type Error = CommandError;

    fn execute(&self) -> Result<(), Self::Error> {
        use std::process::Command;

        let candidates: &[(&str, &[&str])] = &[
            ("umount", &["-l"]),
            ("fusermount3", &["-u", "-z"]),
            ("fusermount", &["-u", "-z"]),
        ];

        for (cmd, args) in candidates {
            let mut command = Command::new(cmd);
            for a in *args {
                command.arg(a);
            }
            command.arg(&self.mount_point);
            if command.spawn().is_ok() {
                return Ok(());
            }
        }

        Err(CommandError::ExecutionFailed(
            "No suitable lazy-unmount command available".to_string(),
        ))
    }
}
