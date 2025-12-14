fn path_to_str<S: AsRef<OsStr>>(path: S) -> Result<String> {
    path.as_ref()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| NetworkError::Other(anyhow::format_err!("Path is not valid UTF-8")))
}
