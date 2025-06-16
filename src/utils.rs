use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Appends a message to a log file
pub fn append_to_log<P: AsRef<Path>>(log_path: P, message: &str) -> Result<(), String> {
    let mut log_file = File::options()
        .append(true)
        .create(true)
        .open(log_path.as_ref())
        .map_err(|e| format!("Error opening log file {}: {}", log_path.as_ref().display(), e))?;

    writeln!(log_file, "{}", message)
        .map_err(|e| format!("Error writing to log file: {}", e))?;

    Ok(())
}

/// Creates a directory if it doesn't exist
pub fn ensure_directory_exists<P: AsRef<Path>>(path: P) -> Result<(), String> {
    if !path.as_ref().exists() {
        fs::create_dir_all(path.as_ref())
            .map_err(|e| format!("Error creating directory {}: {}", path.as_ref().display(), e))?;
    }
    Ok(())
}

/// Removes a file if it exists
pub fn remove_file_if_exists<P: AsRef<Path>>(path: P) -> Result<(), String> {
    if path.as_ref().exists() {
        fs::remove_file(path.as_ref())
            .map_err(|e| format!("Error removing file {}: {}", path.as_ref().display(), e))?;
    }
    Ok(())
}

/// Removes a directory and all its contents if it exists
pub fn remove_dir_all_if_exists<P: AsRef<Path>>(path: P) -> Result<(), String> {
    if path.as_ref().exists() {
        fs::remove_dir_all(path.as_ref())
            .map_err(|e| format!("Error removing directory {}: {}", path.as_ref().display(), e))?;
    }
    Ok(())
}