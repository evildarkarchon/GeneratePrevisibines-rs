use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use log::{info, warn};

#[derive(Clone)]
pub struct CkpeSettings {
    pub ini_file: String,
    pub handle_setting: String,
    pub log_setting: String,
    pub log_file: Option<PathBuf>,
}

/// Executes the Creation Kit with specified action and parameters for building precombines/previs.
///
/// # Arguments
/// * `creation_kit_path` - Path to the Creation Kit executable
/// * `fallout4_path` - Path to the Fallout 4 installation directory
/// * `plugin_name_ext` - The plugin file name with extension
/// * `action` - The Creation Kit action to perform (e.g., "GeneratePrecombined", "GeneratePrevisData")
/// * `output_file` - The expected output file that should be created by the action
/// * `args` - Additional command-line arguments for the Creation Kit
/// * `ckpe_settings` - CKPE configuration settings
/// * `logfile` - Path to the main log file
///
/// # Returns
/// * `Ok(())` if the Creation Kit runs successfully and produces the expected output
/// * `Err(String)` if the command fails or doesn't produce the expected output
pub fn run_creation_kit(
    creation_kit_path: &PathBuf,
    fallout4_path: &PathBuf,
    plugin_name_ext: &str,
    action: &str,
    output_file: &str,
    args: &str,
    ckpe_settings: &CkpeSettings,
    logfile: &PathBuf,
) -> Result<(), String> {
    info!("Running CK option {}", action);

    // Disable ENB/ReShade DLLs
    let dlls_to_disable = [
        "d3d11.dll",
        "d3d10.dll",
        "d3d9.dll",
        "dxgi.dll",
        "enbimgui.dll",
        "d3dcompiler_46e.dll",
    ];

    for dll in dlls_to_disable.iter() {
        let dll_path = fallout4_path.join(dll);
        if dll_path.exists() {
            let disabled_path = fallout4_path.join(format!("{}-PJMdisabled", dll));
            fs::rename(&dll_path, &disabled_path)
                .map_err(|e| format!("Error disabling {}: {}", dll, e))?;
        }
    }

    // Delete previous log if it exists
    if let Some(log_file) = &ckpe_settings.log_file {
        if log_file.exists() {
            fs::remove_file(log_file).map_err(|e| {
                format!("Error removing log file {}: {}", log_file.display(), e)
            })?;
        }
    }

    // Log to our logfile
    let mut log_file = File::options()
        .append(true)
        .create(true)
        .open(logfile)
        .map_err(|e| format!("Error opening log file {}: {}", logfile.display(), e))?;

    writeln!(log_file, "Running CK option {}:", action)
        .map_err(|e| format!("Error writing to log file: {}", e))?;
    writeln!(log_file, "====================================")
        .map_err(|e| format!("Error writing to log file: {}", e))?;

    // Build command line
    let cmd_args = format!("-{}:\"{}\" {}", action, plugin_name_ext, args);

    // Run CreationKit
    let output = Command::new(creation_kit_path)
        .current_dir(fallout4_path)
        .args(cmd_args.split_whitespace())
        .output()
        .map_err(|e| format!("Error executing Creation Kit: {}", e))?;

    let exit_code = output.status.code().unwrap_or(-1);

    // Wait for MO2 to process files
    sleep(Duration::from_secs(5));

    // Append CK log to our log if it exists
    if let Some(log_file_path) = &ckpe_settings.log_file {
        if log_file_path.exists() {
            let ck_log = fs::read_to_string(log_file_path).map_err(|e| {
                format!(
                    "Error reading CK log file {}: {}",
                    log_file_path.display(),
                    e
                )
            })?;

            writeln!(log_file, "{}", ck_log)
                .map_err(|e| format!("Error writing to log file: {}", e))?;
        }
    }

    // Check if output file was created
    let output_path = fallout4_path.join("Data").join(output_file);
    if !output_path.exists() {
        return Err(format!(
            "ERROR - {} failed to create file {} with exit status {}",
            action, output_file, exit_code
        ));
    }

    if exit_code != 0 {
        warn!(
            "WARNING - {} ended with error {} but seemed to finish so error ignored.",
            action, exit_code
        );
    }

    // Re-enable ENB/ReShade DLLs
    for dll in dlls_to_disable.iter() {
        let disabled_path = fallout4_path.join(format!("{}-PJMdisabled", dll));
        if disabled_path.exists() {
            let dll_path = fallout4_path.join(dll);
            fs::rename(&disabled_path, &dll_path)
                .map_err(|e| format!("Error re-enabling {}: {}", dll, e))?;
        }
    }

    Ok(())
}