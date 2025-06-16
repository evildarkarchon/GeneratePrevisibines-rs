use std::fs;
use std::path::PathBuf;
use std::io::{BufRead, BufReader};
use regex::Regex;
use log::{info, warn};
use crate::cli::{BuildMode, BuildStage};
use crate::paths::Paths;
use crate::tools::creation_kit::CkpeSettings;

/// Validates the environment for required files, directories, and settings
pub fn verify_environment(
    paths: &Paths,
    ckpe_settings: &mut CkpeSettings,
    _plugin_name: &str,
    use_bsarch: bool,
) -> Result<(), String> {
    // Check FO4Edit
    if !paths.fo4edit.exists() {
        return Err(format!(
            "ERROR - {} not found",
            paths.fo4edit.display()
        ));
    }

    // Check Fallout4.exe
    let fallout4_exe = paths.fallout4.join("Fallout4.exe");
    if !fallout4_exe.exists() {
        return Err(format!(
            "ERROR - Fallout4.exe not found at {}",
            fallout4_exe.display()
        ));
    }

    // Check Creation Kit
    if !paths.creation_kit.exists() {
        return Err(
            "ERROR - CreationKit.exe not found. Creation Kit must be installed".to_string(),
        );
    }

    // Check CKPE (winhttp.dll)
    let winhttp_dll = paths.fallout4.join("winhttp.dll");
    if !winhttp_dll.exists() {
        return Err(
            "ERROR - CKPE not installed. You may not get a successful Patch without it"
                .to_string(),
        );
    }

    // Check Archive2.exe
    if !paths.archive2.exists() {
        return Err(
            "ERROR - Archive2.exe not found. Creation Kit not properly installed".to_string(),
        );
    }

    // Check CKPE ini
    let ckpe_ini_path = paths.fallout4.join(&ckpe_settings.ini_file);
    if !ckpe_ini_path.exists() {
        let fallout4_test_ini = paths.fallout4.join("fallout4_test.ini");
        if !fallout4_test_ini.exists() {
            return Err(
                "ERROR - CKPE not installed properly. No settings file found".to_string(),
            );
        }
        ckpe_settings.ini_file = "fallout4_test.ini".to_string();
        ckpe_settings.handle_setting = "BSHandleRefObjectPatch".to_string();
        ckpe_settings.log_setting = "OutputFile".to_string();
    }

    // Check xEdit scripts exist
    let scripts = [
        "Batch_FO4MergeCombinedObjectsAndCheck.pas",
        "Batch_FO4MergePreVisAndAutoUpdateRefr.pas",
    ];

    let xedit_scripts_dir = paths.fo4edit.parent().unwrap().join("Edit Scripts");
    for script in scripts.iter() {
        let script_path = xedit_scripts_dir.join(script);
        if !script_path.exists() {
            return Err(format!(
                "ERROR - FO4Edit Script {} not found",
                script_path.display()
            ));
        }

        // Check script version
        if let Ok(file) = fs::File::open(&script_path) {
            let reader = BufReader::new(file);
            let mut version_found = false;

            let version_regex = Regex::new(r"BatchVersion[\s]*=[\s]*(\d+)").unwrap();

            for line in reader.lines() {
                if let Ok(line_content) = line {
                    if let Some(captures) = version_regex.captures(&line_content) {
                        if let Ok(version) = captures[1].parse::<i32>() {
                            if version < 10 {
                                return Err(format!(
                                    "ERROR - FO4Edit Script {} is outdated (version {}). Please update.",
                                    script, version
                                ));
                            }
                            version_found = true;
                            break;
                        }
                    }
                }
            }

            if !version_found {
                return Err(format!(
                    "ERROR - Could not determine version of FO4Edit Script {}",
                    script
                ));
            }
        }
    }

    // Check CK is set for logging
    let ckpe_ini_path = paths.fallout4.join(&ckpe_settings.ini_file);
    if let Ok(file) = fs::File::open(&ckpe_ini_path) {
        let reader = BufReader::new(file);
        let log_regex = Regex::new(&format!(
            r"{}[\s]*=[\s]*(.+)",
            regex::escape(&ckpe_settings.log_setting)
        ))
        .unwrap();

        for line in reader.lines() {
            if let Ok(line_content) = line {
                if let Some(captures) = log_regex.captures(&line_content) {
                    let log_file_str = captures[1].trim();
                    if !log_file_str.is_empty() {
                        ckpe_settings.log_file = Some(paths.fallout4.join(log_file_str));
                    } else {
                        return Err(format!(
                            "ERROR - CK not set for logging redirection. {} is empty in {}",
                            ckpe_settings.log_setting, ckpe_settings.ini_file
                        ));
                    }
                    break;
                }
            }
        }

        if ckpe_settings.log_file.is_none() {
            return Err(format!(
                "ERROR - CK not set for logging redirection. {} not found in {}",
                ckpe_settings.log_setting, ckpe_settings.ini_file
            ));
        }
    }

    // Check if increased handles are enabled (non-blocking warning)
    let ckpe_ini_path = paths.fallout4.join(&ckpe_settings.ini_file);
    if let Ok(file) = fs::File::open(&ckpe_ini_path) {
        let reader = BufReader::new(file);
        let handle_regex = Regex::new(&format!(
            r"{}[\s]*=[\s]*(\d+)",
            regex::escape(&ckpe_settings.handle_setting)
        ))
        .unwrap();

        let mut found_setting = false;
        for line in reader.lines() {
            if let Ok(line_content) = line {
                if let Some(captures) = handle_regex.captures(&line_content) {
                    if let Ok(value) = captures[1].parse::<i32>() {
                        if value == 0 {
                            warn!(
                                "WARNING - {} is not enabled. You may run out of Reference Handles.",
                                ckpe_settings.handle_setting
                            );
                        }
                    }
                    found_setting = true;
                    break;
                }
            }
        }

        if !found_setting {
            warn!(
                "WARNING - {} not found. You may run out of Reference Handles.",
                ckpe_settings.handle_setting
            );
        }
    }

    // Check BSArch if enabled
    if use_bsarch {
        if let Some(bsarch_path) = &paths.bsarch {
            if !bsarch_path.exists() {
                return Err(format!(
                    "ERROR - BSArch enabled but not found at {}",
                    bsarch_path.display()
                ));
            }
        } else {
            return Err("ERROR - BSArch enabled but path not specified".to_string());
        }
    }

    info!("Environment verified successfully");
    Ok(())
}

/// Checks if the plugin and archive are valid for processing
pub fn check_plugin(
    paths: &Paths,
    plugin_name_ext: &str,
    plugin_archive: &str,
    no_prompt: bool,
    prompt_fn: impl Fn(&str) -> Result<bool, String>,
) -> Result<(), String> {
    info!("Checking plugin: {}", plugin_name_ext);

    let plugin_path = paths.fallout4.join("Data").join(plugin_name_ext);
    let archive_path = paths.fallout4.join("Data").join(plugin_archive);

    if archive_path.exists() {
        return Err(format!(
            "ERROR - This Plugin already has an Archive: {}",
            plugin_archive
        ));
    }

    if !plugin_path.exists() {
        // Plugin doesn't exist, try to use xPrevisPatch.esp as seed
        let seed_path = paths.fallout4.join("Data").join("xPrevisPatch.esp");

        if !seed_path.exists() {
            return Err("ERROR - Specified Plugin or xPrevisPatch does not exist".to_string());
        }

        if no_prompt {
            return Err(format!(
                "ERROR - Plugin {} does not exist",
                plugin_name_ext
            ));
        }

        if !prompt_fn("Plugin does not exist, Rename xPrevisPatch.esp to this? [Y/N]")? {
            return Err("Aborted by user".to_string());
        }

        // Rename xPrevisPatch.esp to the plugin name
        fs::rename(&seed_path, &plugin_path)
            .map_err(|e| format!("Error renaming xPrevisPatch.esp: {}", e))?;

        info!("Renamed xPrevisPatch.esp to {}", plugin_name_ext);
    }

    Ok(())
}

/// Checks if the specified directory contains any files with the given file extension
pub fn directory_has_files(dir_path: &PathBuf, extension: &str) -> bool {
    if !dir_path.exists() {
        return false;
    }

    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.ends_with(extension) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Validates the prerequisites for various build stages
pub fn check_stage_prerequisites(
    stage: BuildStage,
    paths: &Paths,
    plugin_name_ext: &str,
    plugin_name: &str,
    build_mode: &BuildMode,
    has_files_fn: impl Fn(&PathBuf, &str) -> bool,
) -> Result<(), String> {
    match stage {
        BuildStage::VerifyEnvironment => Ok(()),

        BuildStage::GeneratePrecombines => {
            // Check if plugin exists
            let plugin_path = paths.fallout4.join("Data").join(plugin_name_ext);
            if !plugin_path.exists() {
                return Err(format!(
                    "ERROR - Plugin {} does not exist",
                    plugin_name_ext
                ));
            }
            Ok(())
        }

        BuildStage::MergePrecombines => {
            // Check if precombined meshes exist
            let precombined_dir = paths
                .fallout4
                .join("Data")
                .join("meshes")
                .join("precombined");
            if !has_files_fn(&precombined_dir, ".nif") {
                return Err(
                    "ERROR - No precombined meshes found. Run GeneratePrecombines first."
                        .to_string(),
                );
            }
            Ok(())
        }

        BuildStage::ArchivePrecombines => {
            // Check if precombined meshes exist
            let precombined_dir = paths
                .fallout4
                .join("Data")
                .join("meshes")
                .join("precombined");
            if !has_files_fn(&precombined_dir, ".nif") {
                return Err(
                    "ERROR - No precombined meshes found. Run GeneratePrecombines first."
                        .to_string(),
                );
            }
            Ok(())
        }

        BuildStage::CompressPsg => {
            if build_mode != &BuildMode::Clean {
                return Err("ERROR - CompressPSG is only available in Clean mode".to_string());
            }

            // Check if PSG file exists
            let psg_path = paths
                .fallout4
                .join("Data")
                .join(format!("{} - Geometry.psg", plugin_name));
            if !psg_path.exists() {
                return Err(
                    "ERROR - No Geometry.psg file found. Run GeneratePrecombines first."
                        .to_string(),
                );
            }
            Ok(())
        }

        BuildStage::BuildCdx => {
            if build_mode != &BuildMode::Clean {
                return Err("ERROR - BuildCDX is only available in Clean mode".to_string());
            }
            Ok(())
        }

        BuildStage::GeneratePrevis => {
            // Check if plugin exists
            let plugin_path = paths.fallout4.join("Data").join(plugin_name_ext);
            if !plugin_path.exists() {
                return Err(format!(
                    "ERROR - Plugin {} does not exist",
                    plugin_name_ext
                ));
            }
            Ok(())
        }

        BuildStage::MergePrevis => {
            // Check if vis files exist
            let vis_dir = paths.fallout4.join("Data").join("vis");
            if !has_files_fn(&vis_dir, ".uvd") {
                return Err(
                    "ERROR - No visibility files found. Run GeneratePreVisData first."
                        .to_string(),
                );
            }

            // Check if Previs.esp exists
            let previs_path = paths.fallout4.join("Data").join("Previs.esp");
            if !previs_path.exists() {
                return Err(
                    "ERROR - Previs.esp not found. Run GeneratePreVisData first.".to_string(),
                );
            }

            Ok(())
        }

        BuildStage::ArchiveVis => {
            // Check if vis files exist
            let vis_dir = paths.fallout4.join("Data").join("vis");
            if !has_files_fn(&vis_dir, ".uvd") {
                return Err(
                    "ERROR - No visibility files found. Run GeneratePreVisData first."
                        .to_string(),
                );
            }
            Ok(())
        }
    }
}