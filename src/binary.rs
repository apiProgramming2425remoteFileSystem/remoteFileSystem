use escargot::CargoBuild;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

pub fn get_bin(name: &str) -> PathBuf {
    let time_start = std::time::Instant::now();

    // Search for existing binary first (target/debug/name or target/release/name)
    if let Some(bin_path) = find_existing_binary(name) {
        // Find the source directory of the crate by name
        // This assumes the name of the crate matches the folder name
        let src_path = find_source_dir(name);

        if !is_binary_outdated(&bin_path, &src_path) {
            println!("Found binary {} in {:?}", name, time_start.elapsed());

            // It's up-to-date! Return the direct path
            return bin_path.to_path_buf();
        } else {
            println!("🔄 Changes detected in '{}', rebuilding...", name);
        }
    }

    // Fallback: Se non esiste o è vecchio, usiamo escargot (Lento ma necessario)
    let p = CargoBuild::new()
        .package(name)
        .bin(name)
        .current_target()
        .run()
        .expect("Failed to build binary")
        .path()
        .to_path_buf();
    println!("Built binary {} in {:?}", name, time_start.elapsed());
    p
}

/// Tries to find an existing built binary in target/debug or target/release
/// Returns None if not found
fn find_existing_binary(name: &str) -> Option<PathBuf> {
    // Start from the manifest directory
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Check type of build (debug/release)
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let mut path = workspace_root.join("target").join(profile).join(name);
    println!("Looking for binary at {:?}", path);

    // Gestione estensione .exe per Windows
    #[cfg(windows)]
    path.set_extension("exe");

    if path.exists() { Some(path) } else { None }
}

fn find_source_dir(name: &str) -> PathBuf {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    workspace_root.join(name).join("src")
}

/// Checks if a binary is outdated compared to source files
fn is_binary_outdated(bin_path: &Path, src_dir: &Path) -> bool {
    let Ok(bin_meta) = fs::metadata(bin_path) else {
        return true;
    };
    let Ok(bin_time) = bin_meta.modified() else {
        return true;
    };

    // Recursively scan the src directory for recently modified .rs or .toml files
    let walker = WalkDir::new(src_dir).into_iter();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "toml")
            && let Ok(meta) = fs::metadata(path)
            && let Ok(src_time) = meta.modified()
            && src_time > bin_time
        {
            return true;
        }
    }

    false
}

#[derive(Clone, Debug)]
/// Manages command-line arguments and environment variables for a binary.
pub struct BinaryBuilder {
    /// CLI arguments
    args: Vec<String>,
    /// Environment variables
    envs: Vec<(String, String)>,
    /// Whether to enable this binary or not
    enabled: bool,
}

impl BinaryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables this binary to be started
    pub fn enable(&mut self) -> &mut Self {
        self.enabled = true;
        self
    }

    /// Disables this binary from being started
    pub fn disable(&mut self) -> &mut Self {
        self.enabled = false;
        self
    }

    /// Checks if this binary is enabled
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Adds a positional CLI argument or flag
    pub fn arg(&mut self, val: &str) -> &mut Self {
        self.args.push(val.to_string());
        self
    }

    /// Adds a key-value pair CLI argument
    pub fn arg_pair(&mut self, key: &str, val: &str) -> &mut Self {
        self.args.push(key.to_string());
        self.args.push(val.to_string());
        self
    }

    /// Sets an environment variable
    pub fn env(&mut self, key: &str, val: &str) -> &mut Self {
        self.envs.push((key.to_string(), val.to_string()));
        self
    }

    /// Applies the stored arguments and environment variables to a Command
    // NOTE: this consumes self
    pub fn apply_to(self, cmd: &mut Command) {
        cmd.args(self.args);
        cmd.envs(self.envs);
    }

    /// Checks if a flag or argument has already been added by the user.
    /// It checks: "-arg val", "--arg=val" and "--flag" (no value).
    pub fn has_arg(&self, key: &str) -> bool {
        self.args
            .iter()
            .any(|arg| arg == key || arg.starts_with(&format!("{}=", key)))
    }
}

impl Default for BinaryBuilder {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            envs: Vec::new(),
            enabled: true,
        }
    }
}
