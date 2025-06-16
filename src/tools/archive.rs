use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use log::{info, error};
use crate::cli::BuildMode;

/// Executes Archive2.exe to create a BA2 archive with the given folders.
///
/// # Arguments
/// * `archive2_path` - Path to Archive2.exe
/// * `data_dir` - Fallout 4 Data directory
/// * `plugin_archive` - Archive file name
/// * `folders` - A comma-separated list of folders to include in the archive
/// * `qualifiers` - Additional qualifiers to pass to archive2 (e.g., compression type)
///
/// # Returns
/// * `Ok(())` if the archive is created successfully
/// * `Err(String)` if the command fails
pub fn run_archive(
    archive2_path: &PathBuf,
    data_dir: &PathBuf,
    plugin_archive: &str,
    folders: &str,
    qualifiers: &str,
) -> Result<(), String> {
    let archive_path = data_dir.join(plugin_archive);

    info!("Creating archive: {} with folders: {}", plugin_archive, folders);

    let mut command = Command::new(archive2_path);
    command.current_dir(data_dir)
        .arg(folders)
        .arg(format!("-c={}", plugin_archive))
        .arg(qualifiers)
        .arg("-f=General")
        .arg("-q");

    // Execute and check result
    match command.output() {
        Ok(output) => {
            if output.status.success() {
                if !archive_path.exists() {
                    return Err(format!("Archive was not created: {}", plugin_archive));
                }
                Ok(())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Archive2 failed: {}", error))
            }
        }
        Err(e) => Err(format!("Failed to execute Archive2: {}", e))
    }
}

/// Extracts the plugin's archive to the Data directory.
///
/// # Arguments
/// * `archive2_path` - Path to Archive2.exe
/// * `data_dir` - Fallout 4 Data directory
/// * `plugin_archive` - Archive file name
///
/// # Returns
/// * `Ok(())` if the extraction is successful
/// * `Err(String)` if the command fails
pub fn extract_archive(
    archive2_path: &PathBuf,
    data_dir: &PathBuf,
    plugin_archive: &str,
) -> Result<(), String> {
    let archive_path = data_dir.join(plugin_archive);

    if !archive_path.exists() {
        return Err(format!("Archive does not exist: {}", plugin_archive));
    }

    info!("Extracting archive: {}", plugin_archive);

    let mut command = Command::new(archive2_path);
    command.current_dir(data_dir)
        .arg(plugin_archive)
        .arg("-e=.")
        .arg("-q");

    // Execute and check result
    match command.output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(format!("Archive2 extraction failed: {}", error))
            }
        }
        Err(e) => Err(format!("Failed to execute Archive2 extraction: {}", e))
    }
}

/// Adds files from the specified folder to the plugin's archive.
/// Since Archive2 doesn't support adding to existing archives, this extracts
/// the archive first, then creates a new one with the combined content.
///
/// # Arguments
/// * `archive2_path` - Path to Archive2.exe
/// * `data_dir` - Fallout 4 Data directory
/// * `plugin_archive` - Archive file name
/// * `folder` - The folder to add to the archive (e.g., "vis")
/// * `qualifiers` - Archive qualifiers
/// * `has_files_fn` - Function to check if directory has files
///
/// # Returns
/// * `Ok(())` if successful
/// * `Err(String)` if any command fails
pub fn add_to_archive<F>(
    archive2_path: &PathBuf,
    data_dir: &PathBuf,
    plugin_archive: &str,
    folder: &str,
    qualifiers: &str,
    has_files_fn: F,
) -> Result<(), String>
where
    F: Fn(&PathBuf, &str) -> bool,
{
    let archive_path = data_dir.join(plugin_archive);
    let precombined_dir = data_dir.join("meshes").join("precombined");

    if !archive_path.exists() {
        return run_archive(archive2_path, data_dir, plugin_archive, folder, qualifiers);
    }

    // Extract existing archive
    extract_archive(archive2_path, data_dir, plugin_archive)?;

    // Small delay to ensure files are extracted
    sleep(Duration::from_secs(5));

    // Remove the existing archive
    if let Err(e) = fs::remove_file(&archive_path) {
        return Err(format!("Failed to remove existing archive: {}", e));
    }

    // Check if we have precombined meshes extracted
    let has_precombined = has_files_fn(&precombined_dir, ".nif");

    if has_precombined {
        // Archive both directories
        run_archive(
            archive2_path,
            data_dir,
            plugin_archive,
            &format!("meshes\\precombined,{}", folder),
            qualifiers,
        )?;

        // Clean up precombined directory
        fs::remove_dir_all(precombined_dir)
            .map_err(|e| format!("Error removing precombined directory: {}", e))?;
    } else {
        // Archive the new folder
        run_archive(archive2_path, data_dir, plugin_archive, folder, qualifiers)?;
    }

    Ok(())
}

/// Runs BSArch to perform archiving operations
///
/// # Arguments
/// * `bsarch_path` - Path to BSArch executable
/// * `action` - Description of the action being performed
/// * `bsarch_args` - Arguments to pass to BSArch
///
/// # Returns
/// * `Ok(())` if successful
/// * `Err(String)` if the command fails
pub fn run_bsarch(
    bsarch_path: &PathBuf,
    action: &str,
    bsarch_args: &[&str],
) -> Result<(), String> {
    info!("Running BSArch to perform action: '{}' with args: {:?}", action, bsarch_args);
    info!("Executing: {} {}", bsarch_path.display(), bsarch_args.join(" "));

    let mut command = Command::new(bsarch_path);
    command.args(bsarch_args);

    // Execute the command and capture status
    match command.status() {
        Ok(status) => {
            if status.success() {
                info!("BSArch action '{}' completed successfully.", action);
                Ok(())
            } else {
                let error_msg = format!(
                    "BSArch action '{}' failed with exit code: {:?}.",
                    action,
                    status.code().unwrap_or(-1)
                );
                error!("{}", error_msg);
                Err(error_msg)
            }
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to execute BSArch for action '{}': {}. Path: {}",
                action,
                e,
                bsarch_path.display()
            );
            error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

/// Retrieves the appropriate archive qualifiers based on the build mode.
///
/// # Arguments
/// * `build_mode` - The current build mode
///
/// # Returns
/// A `&'static str` representing the additional qualifiers for archiving:
/// - If the build mode is `BuildMode::Xbox`, returns `"-compression=XBox"`.
/// - For all other build modes, returns an empty string (`""`).
pub fn get_archive_qualifiers(build_mode: &BuildMode) -> &'static str {
    match build_mode {
        BuildMode::Xbox => "-compression=XBox",
        _ => "",
    }
}