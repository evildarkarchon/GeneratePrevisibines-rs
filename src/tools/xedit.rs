use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use log::info;

/// Runs an xEdit script against two specified plugin files and logs the results.
///
/// This function automates the process of executing an xEdit script with given parameters,
/// captures logs, and ensures the expected results by checking for completion messages.
///
/// # Parameters
///
/// - `fo4edit_path`: Path to the FO4Edit/xEdit executable
/// - `script`: The name of the xEdit script to be executed. It is assumed to reside
///   in the `Edit Scripts` directory under the xEdit path.
/// - `plugin1`: The name of the primary plugin file for the script to process.
/// - `plugin2`: The name of the secondary plugin file, if applicable, for the script.
/// - `logfile`: Path to the main log file
/// - `unattended_logfile`: Path to the unattended script log file
///
/// # Returns
///
/// Returns `Ok(())` if the script executed successfully and produced the expected results.
/// Returns `Err(String)` with an error message if any step of the execution fails.
pub fn run_xedit_script(
    fo4edit_path: &PathBuf,
    script: &str,
    plugin1: &str,
    plugin2: &str,
    logfile: &PathBuf,
    unattended_logfile: &PathBuf,
) -> Result<(), String> {
    info!("Running xEdit script {} against {}", script, plugin1);

    let mut log_file = File::options()
        .append(true)
        .create(true)
        .open(logfile)
        .map_err(|e| format!("Error opening log file {}: {}", logfile.display(), e))?;

    writeln!(
        log_file,
        "Running xEdit script {} against {}",
        script, plugin1
    )
    .map_err(|e| format!("Error writing to log file: {}", e))?;
    writeln!(log_file, "====================================")
        .map_err(|e| format!("Error writing to log file: {}", e))?;

    // Create plugins list
    let plugins_file = std::env::temp_dir().join("Plugins.txt");
    let mut file = File::create(&plugins_file)
        .map_err(|e| format!("Error creating plugins file: {}", e))?;

    writeln!(file, "*{}", plugin1)
        .map_err(|e| format!("Error writing to plugins file: {}", e))?;
    writeln!(file, "*{}", plugin2)
        .map_err(|e| format!("Error writing to plugins file: {}", e))?;

    // Delete previous log if it exists
    if unattended_logfile.exists() {
        fs::remove_file(unattended_logfile)
            .map_err(|e| format!("Error removing unattended log file: {}", e))?;
    }

    // Start xEdit process
    let _script_path = format!(
        "{}\\Edit Scripts\\{}",
        fo4edit_path.parent().unwrap().display(),
        script
    );

    let mut xedit_process = Command::new(fo4edit_path)
        .args(&[
            "-fo4",
            "-autoexit",
            format!("-P:{}", plugins_file.display()).as_str(),
            format!("-Script:{}", script).as_str(),
            format!("-Mod:{}", plugin1).as_str(),
            format!("-log:{}", unattended_logfile.display()).as_str(),
        ])
        .spawn()
        .map_err(|e| format!("Error starting xEdit: {}", e))?;

    // Wait for xEdit to start processing
    sleep(Duration::from_secs(5));

    // This part is tricky in Rust - we need to simulate keypresses to activate xEdit
    // In a proper implementation, we'd use the winapi or similar to send keys
    // For now, we'll just wait for the process to exit on its own

    // Wait for script to finish and create log
    while !unattended_logfile.exists() {
        sleep(Duration::from_secs(5));
    }

    // Wait a bit more for xEdit to finish processing
    sleep(Duration::from_secs(10));

    // Try to terminate xEdit process - this is a simplified approach
    let _ = xedit_process.kill();
    let _ = xedit_process.wait();

    // Wait for MO2 to process files
    sleep(Duration::from_secs(5));

    // Append xEdit log to our log
    if unattended_logfile.exists() {
        let xedit_log = fs::read_to_string(unattended_logfile)
            .map_err(|e| format!("Error reading xEdit log file: {}", e))?;

        writeln!(log_file, "{}", xedit_log)
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        // Check for completion message
        if !xedit_log.contains("Completed: ") {
            return Err(format!("ERROR - FO4Edit script {} failed", script));
        }
    } else {
        return Err(format!(
            "ERROR - FO4Edit script {} did not produce a log file",
            script
        ));
    }

    Ok(())
}