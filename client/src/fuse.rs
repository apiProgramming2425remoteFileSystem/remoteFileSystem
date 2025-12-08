#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use crate::fs_model::FileSystem;

pub struct Fs {
    fs: FileSystem,
}

impl Fs {
    pub fn new(base_url: &str) -> Self {
        Self {
            fs: FileSystem::new(base_url),
        }
    }
}

/*
/// pub async fn template_fn(&self, args) -> Result<> {
///     1. convert args to fs_model structures
///     2. call the needed self.fs function
///     3. converts the result
///     4. do other necessary operations
///     5. return the correct result
/// }
pub async fn template_fn(&self, args) -> Result<> {
    // 1. convert args to fs_model structures

    // 2. call the needed self.fs function
    let res_fs_model = self.fs.template_fn(converted_args).await?;

    // 3. converts the result
    let converted_res = ;

    // 4. return result
    Ok(converted_res)
}
*/
