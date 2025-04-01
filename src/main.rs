// generate_previsbines.rs
//
// Rust implementation of GeneratePrevisibines.bat
// A tool to build precombine/Previs using prompted Plugin "Seed"
//
// Original Author: PJM V2.6 Mar 2025
// Rust Port: March 2025

use clap::{Parser, ValueEnum};
use log::{error, info, warn};
use regex::Regex;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum BuildMode {
    Clean,
    Filtered,
    Xbox,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum BuildStage {
    VerifyEnvironment = 0,
    GeneratePrecombines = 1,
    MergePrecombines = 2,
    ArchivePrecombines = 3,
    CompressPsg = 4,
    BuildCdx = 5,
    GeneratePrevis = 6,
    MergePrevis = 7,
    ArchiveVis = 8,
}

impl BuildStage {
    fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::VerifyEnvironment),
            1 => Some(Self::GeneratePrecombines),
            2 => Some(Self::MergePrecombines),
            3 => Some(Self::ArchivePrecombines),
            4 => Some(Self::CompressPsg),
            5 => Some(Self::BuildCdx),
            6 => Some(Self::GeneratePrevis),
            7 => Some(Self::MergePrevis),
            8 => Some(Self::ArchiveVis),
            _ => None,
        }
    }

    fn display_stages(build_mode: &BuildMode) -> String {
        let mut result = String::new();
        result.push_str("[0] Verify Environment\n");
        result.push_str("[1] Generate Precombines Via CK\n");
        result.push_str("[2] Merge PrecombineObjects.esp Via FO4Edit\n");
        result.push_str("[3] Create BA2 Archive from Precombines\n");

        if *build_mode == BuildMode::Clean {
            result.push_str("[4] Compress PSG Via CK\n");
            result.push_str("[5] Build CDX Via CK\n");
        }

        result.push_str("[6] Generate Previs Via CK\n");
        result.push_str("[7] Merge Previs.esp Via FO4Edit\n");
        result.push_str("[8] Add vis files to BA2 Archive\n");

        result
    }
}

impl std::fmt::Display for BuildMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildMode::Clean => write!(f, "clean"),
            BuildMode::Filtered => write!(f, "filtered"),
            BuildMode::Xbox => write!(f, "xbox"),
        }
    }
}

// Command-line arguments
#[derive(Parser, Debug)]
#[command(
    name = "generate_previsbines",
    about = "Automatic Previsbine Builder for Fallout 4\nBased on the original batch script by PJM",
    version = "2.6"
)]
struct Args {
    /// Build mode: clean, filtered, or xbox
    #[arg(value_enum)]
    #[arg(short, long, default_value = "clean")]
    mode: BuildMode,

    /// The plugin to generate previsbines for
    #[arg(value_name = "PLUGIN")]
    plugin: Option<String>,

    /// Path to FO4Edit/xEdit executable
    #[arg(long)]
    fo4edit_path: Option<String>,

    /// Path to Fallout 4 installation directory
    #[arg(long)]
    fallout4_path: Option<String>,

    #[arg(long)]
    /// Stage of the process to start from
    start_stage: Option<i32>,

    /// Don't prompt for confirmation, just execute
    #[arg(short, long)]
    no_prompt: bool,

    /// Specify files to keep after completion
    #[arg(short, long)]
    keep_files: bool,
}

struct Paths {
    fo4edit: PathBuf,
    fallout4: PathBuf,
    creation_kit: PathBuf,
    archive2: PathBuf,
}

struct CkpeSettings {
    ini_file: String,
    handle_setting: String,
    log_setting: String,
    log_file: Option<PathBuf>,
}

struct PrevisbineBuilder {
    args: Args,
    paths: Paths,
    ckpe_settings: CkpeSettings,
    plugin_name: String,
    plugin_name_ext: String,
    plugin_archive: String,
    logfile: PathBuf,
    unattended_logfile: PathBuf,
}

impl PrevisbineBuilder {
    fn new(args: Args) -> Result<Self, String> {
        // Find path to FO4Edit
        let fo4edit_path = if let Some(path) = args.fo4edit_path.clone() {
            PathBuf::from(path)
        } else {
            Self::find_fo4edit()?
        };

        // Find path to Fallout 4
        let fallout4_path = if let Some(path) = args.fallout4_path.clone() {
            PathBuf::from(path)
        } else {
            Self::find_fallout4()?
        };

        // Prepare other paths
        let creation_kit_path = fallout4_path.join("CreationKit.exe");
        let archive2_path = fallout4_path
            .join("tools")
            .join("archive2")
            .join("archive2.exe");

        // Extract plugin name
        let (plugin_name, plugin_name_ext) = if let Some(plugin) = args.plugin.clone() {
            let plugin_lowercase = plugin.to_lowercase();
            if plugin_lowercase.ends_with(".esp")
                || plugin_lowercase.ends_with(".esm")
                || plugin_lowercase.ends_with(".esl")
            {
                let name = plugin.clone();
                let name_without_ext = name
                    .rfind('.')
                    .map(|i| &name[0..i])
                    .unwrap_or(&name)
                    .to_string();
                (name_without_ext, plugin)
            } else {
                (plugin.clone(), format!("{}.esp", plugin))
            }
        } else {
            (String::new(), String::new())
        };

        // Setup temp and log files
        let temp_dir = env::temp_dir();
        let logfile = temp_dir.join(format!("{}.log", plugin_name));
        let unattended_logfile = temp_dir.join("UnattendedScript.log");

        // CKPE settings
        let ckpe_settings = CkpeSettings {
            ini_file: "CreationKitPlatformExtended.ini".to_string(),
            handle_setting: "bBSPointerHandleExtremly".to_string(),
            log_setting: "sOutputFile".to_string(),
            log_file: None,
        };

        let paths = Paths {
            fo4edit: fo4edit_path,
            fallout4: fallout4_path,
            creation_kit: creation_kit_path,
            archive2: archive2_path,
        };

        // Create plugin_archive string before moving plugin_name
        let plugin_archive = format!("{} - Main.ba2", &plugin_name);

        Ok(PrevisbineBuilder {
            args,
            paths,
            ckpe_settings,
            plugin_name,
            plugin_name_ext,
            plugin_archive,
            logfile,
            unattended_logfile,
        })
    }

    fn prompt_for_plugin_name(&mut self) -> Result<(), String> {
        println!("No plugin specified. Please enter a plugin name:");
        print!("Enter plugin name: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Error reading input: {}", e))?;

        let plugin_name = input.trim().to_string();
        if plugin_name.is_empty() {
            return Err("No plugin name entered".to_string());
        }

        // Extract plugin name and extension
        if plugin_name.to_lowercase().ends_with(".esp")
            || plugin_name.to_lowercase().ends_with(".esm")
            || plugin_name.to_lowercase().ends_with(".esl")
        {
            let name_without_ext = plugin_name
                .rfind('.')
                .map(|i| &plugin_name[0..i])
                .unwrap_or(&plugin_name)
                .to_string();
            self.plugin_name = name_without_ext;
            self.plugin_name_ext = plugin_name;
        } else {
            self.plugin_name = plugin_name.clone(); // Clone it before moving
            self.plugin_name_ext = format!("{}.esp", plugin_name);
        }

        // Update plugin_archive as well
        self.plugin_archive = format!("{} - Main.ba2", &self.plugin_name);

        Ok(())
    }

    // Add this new method to prompt for stage number
    fn prompt_for_stage(&self, build_mode: &BuildMode) -> Result<BuildStage, String> {
        println!("Plugin already exists. Choose a stage to start from:");

        // Print all stages except VerifyEnvironment (0)
        for stage in 1..=8 {
            if let Some(build_stage) = BuildStage::from_i32(stage) {
                // Skip stages that aren't applicable for the current build mode
                if (build_mode != &BuildMode::Clean)
                    && (build_stage == BuildStage::CompressPsg
                        || build_stage == BuildStage::BuildCdx)
                {
                    continue;
                }
                println!("[{}] {}", stage, self.get_stage_description(build_stage));
            }
        }

        print!("Enter stage number (1-8): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Error reading input: {}", e))?;

        let stage_num = input
            .trim()
            .parse::<i32>()
            .map_err(|_| "Invalid stage number".to_string())?;

        BuildStage::from_i32(stage_num)
            .ok_or_else(|| format!("Invalid stage number: {}", stage_num))
    }

    // Helper method to get descriptive stage names
    fn get_stage_description(&self, stage: BuildStage) -> &'static str {
        match stage {
            BuildStage::VerifyEnvironment => "Verify Environment",
            BuildStage::GeneratePrecombines => "Generate Precombines Via CK",
            BuildStage::MergePrecombines => "Merge PrecombineObjects.esp Via FO4Edit",
            BuildStage::ArchivePrecombines => "Create BA2 Archive from Precombines",
            BuildStage::CompressPsg => "Compress PSG Via CK",
            BuildStage::BuildCdx => "Build CDX Via CK",
            BuildStage::GeneratePrevis => "Generate Previs Via CK",
            BuildStage::MergePrevis => "Merge Previs.esp Via FO4Edit",
            BuildStage::ArchiveVis => "Add vis files to BA2 Archive",
        }
    }

    fn display_stages(&self) {
        println!("Available stages to resume from:");
        print!("{}", BuildStage::display_stages(&self.args.mode));
        println!("Enter stage number (0-8) to resume from that stage, or any other key to exit.");
    }

    // This method checks if a directory has files matching a pattern
    fn directory_has_files(&self, dir_path: &PathBuf, extension: &str) -> bool {
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
    fn check_stage_prerequisites(&self, stage: BuildStage) -> Result<(), String> {
        match stage {
            BuildStage::VerifyEnvironment => Ok(()),

            BuildStage::GeneratePrecombines => {
                // Check if plugin exists
                let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
                if !plugin_path.exists() {
                    return Err(format!(
                        "ERROR - Plugin {} does not exist",
                        self.plugin_name_ext
                    ));
                }
                Ok(())
            }

            BuildStage::MergePrecombines => {
                // Check if precombined meshes exist
                let precombined_dir = self
                    .paths
                    .fallout4
                    .join("Data")
                    .join("meshes")
                    .join("precombined");
                if !self.directory_has_files(&precombined_dir, ".nif") {
                    return Err(
                        "ERROR - No precombined meshes found. Run GeneratePrecombines first."
                            .to_string(),
                    );
                }
                Ok(())
            }

            BuildStage::ArchivePrecombines => {
                // Check if precombined meshes exist
                let precombined_dir = self
                    .paths
                    .fallout4
                    .join("Data")
                    .join("meshes")
                    .join("precombined");
                if !self.directory_has_files(&precombined_dir, ".nif") {
                    return Err(
                        "ERROR - No precombined meshes found. Run GeneratePrecombines first."
                            .to_string(),
                    );
                }
                Ok(())
            }

            BuildStage::CompressPsg => {
                if self.args.mode != BuildMode::Clean {
                    return Err("ERROR - CompressPSG is only available in Clean mode".to_string());
                }

                // Check if PSG file exists
                let psg_path = self
                    .paths
                    .fallout4
                    .join("Data")
                    .join(format!("{} - Geometry.psg", self.plugin_name));
                if !psg_path.exists() {
                    return Err(
                        "ERROR - No Geometry.psg file found. Run GeneratePrecombines first."
                            .to_string(),
                    );
                }
                Ok(())
            }

            BuildStage::BuildCdx => {
                if self.args.mode != BuildMode::Clean {
                    return Err("ERROR - BuildCDX is only available in Clean mode".to_string());
                }
                Ok(())
            }

            BuildStage::GeneratePrevis => {
                // Check if plugin exists
                let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
                if !plugin_path.exists() {
                    return Err(format!(
                        "ERROR - Plugin {} does not exist",
                        self.plugin_name_ext
                    ));
                }
                Ok(())
            }

            BuildStage::MergePrevis => {
                // Check if vis files exist
                let vis_dir = self.paths.fallout4.join("Data").join("vis");
                if !self.directory_has_files(&vis_dir, ".uvd") {
                    return Err(
                        "ERROR - No visibility files found. Run GeneratePreVisData first."
                            .to_string(),
                    );
                }

                // Check if Previs.esp exists
                let previs_path = self.paths.fallout4.join("Data").join("Previs.esp");
                if !previs_path.exists() {
                    return Err(
                        "ERROR - Previs.esp not found. Run GeneratePreVisData first.".to_string(),
                    );
                }

                Ok(())
            }

            BuildStage::ArchiveVis => {
                // Check if vis files exist
                let vis_dir = self.paths.fallout4.join("Data").join("vis");
                if !self.directory_has_files(&vis_dir, ".uvd") {
                    return Err(
                        "ERROR - No visibility files found. Run GeneratePreVisData first."
                            .to_string(),
                    );
                }
                Ok(())
            }
        }
    }
    fn find_fo4edit() -> Result<PathBuf, String> {
        // First check current directory
        let current_dir =
            env::current_dir().map_err(|e| format!("Error getting current directory: {}", e))?;

        let candidates = [
            current_dir.join("FO4Edit64.exe"),
            current_dir.join("xEdit64.exe"),
            current_dir.join("FO4Edit.exe"),
            current_dir.join("xEdit.exe"),
        ];

        for candidate in candidates.iter() {
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        // Try registry
        let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
        match hkcr.open_subkey("FO4Script\\DefaultIcon") {
            Ok(subkey) => {
                match subkey.get_value::<String, _>("") {
                    Ok(value) => {
                        // Registry stores with quotes
                        let path = value.replace("\"", "");
                        return Ok(PathBuf::from(path));
                    }
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }

        Err("FO4Edit/xEdit not found. Please specify path with --fo4edit_path".to_string())
    }

    fn find_fallout4() -> Result<PathBuf, String> {
        // Try registry
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(subkey) = hklm.open_subkey("SOFTWARE\\Wow6432Node\\Bethesda Softworks\\Fallout4")
        {
            if let Ok(path) = subkey.get_value::<String, _>("installed path") {
                return Ok(PathBuf::from(path));
            }
        }

        Err(
            "Fallout 4 installation not found. Please specify path with --fallout4_path"
                .to_string(),
        )
    }

    fn verify_environment(&mut self) -> Result<(), String> {
        // Check FO4Edit
        if !self.paths.fo4edit.exists() {
            return Err(format!(
                "ERROR - {} not found",
                self.paths.fo4edit.display()
            ));
        }

        // Check Fallout4.exe
        let fallout4_exe = self.paths.fallout4.join("Fallout4.exe");
        if !fallout4_exe.exists() {
            return Err(format!(
                "ERROR - Fallout4.exe not found at {}",
                fallout4_exe.display()
            ));
        }

        // Check Creation Kit
        if !self.paths.creation_kit.exists() {
            return Err(
                "ERROR - CreationKit.exe not found. Creation Kit must be installed".to_string(),
            );
        }

        // Check CKPE (winhttp.dll)
        let winhttp_dll = self.paths.fallout4.join("winhttp.dll");
        if !winhttp_dll.exists() {
            return Err(
                "ERROR - CKPE not installed. You may not get a successful Patch without it"
                    .to_string(),
            );
        }

        // Check Archive2.exe
        if !self.paths.archive2.exists() {
            return Err(
                "ERROR - Archive2.exe not found. Creation Kit not properly installed".to_string(),
            );
        }

        // Check CKPE ini
        let ckpe_ini_path = self.paths.fallout4.join(&self.ckpe_settings.ini_file);
        if !ckpe_ini_path.exists() {
            let fallout4_test_ini = self.paths.fallout4.join("fallout4_test.ini");
            if !fallout4_test_ini.exists() {
                return Err(
                    "ERROR - CKPE not installed properly. No settings file found".to_string(),
                );
            }
            self.ckpe_settings.ini_file = "fallout4_test.ini".to_string();
            self.ckpe_settings.handle_setting = "BSHandleRefObjectPatch".to_string();
            self.ckpe_settings.log_setting = "OutputFile".to_string();
        }

        // Check for required scripts
        let edit_scripts_dir = self.paths.fo4edit.parent().unwrap().join("Edit Scripts");
        let script_paths = [
            (
                edit_scripts_dir.join("Batch_FO4MergePrevisandCleanRefr.pas"),
                "V2.2",
            ),
            (
                edit_scripts_dir.join("Batch_FO4MergeCombinedObjectsAndCheck.pas"),
                "V1.5",
            ),
        ];

        for (script_path, version) in &script_paths {
            if !script_path.exists() {
                return Err(format!(
                    "ERROR - Required xEdit Script {} missing",
                    script_path.display()
                ));
            }

            // Check script version
            let mut file = File::open(script_path)
                .map_err(|e| format!("Error opening {}: {}", script_path.display(), e))?;
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| format!("Error reading {}: {}", script_path.display(), e))?;

            // Extract actual version from the script with regex
            let re = Regex::new(r"V(\d+\.\d+)").unwrap();
            let script_version = re
                .captures(&content)
                .and_then(|cap| cap.get(1))
                .map(|m| m.as_str())
                .ok_or_else(|| {
                    format!(
                        "ERROR - Cannot determine version of {}",
                        script_path.display()
                    )
                })?;

            // Extract required version number
            let required_version_num = version.trim_start_matches('V');

            // Compare versions (simple string comparison works for X.Y format)
            if script_version < required_version_num {
                return Err(format!(
                    "ERROR - Old Script {} found (V{}), V{} or newer required",
                    script_path.display(),
                    script_version,
                    required_version_num
                ));
            }
        }

        // Check CK log redirection
        let ckpe_ini_path = self.paths.fallout4.join(&self.ckpe_settings.ini_file);
        let mut log_file_path: Option<PathBuf> = None;

        let file = File::open(&ckpe_ini_path)
            .map_err(|e| format!("Error opening {}: {}", ckpe_ini_path.display(), e))?;
        let reader = BufReader::new(file);

        let log_setting_regex =
            Regex::new(&format!(r"{}=(.+)", self.ckpe_settings.log_setting)).unwrap();

        for line in reader.lines() {
            let line =
                line.map_err(|e| format!("Error reading {}: {}", ckpe_ini_path.display(), e))?;
            if let Some(captures) = log_setting_regex.captures(&line) {
                if let Some(log_file) = captures.get(1) {
                    log_file_path = Some(self.paths.fallout4.join(log_file.as_str()));
                    break;
                }
            }
        }

        if log_file_path.is_none() {
            return Err(format!(
                "ERROR - CK Logging not set in this ini. To fix, set {}=CK.log in it.",
                self.ckpe_settings.log_setting
            ));
        }

        self.ckpe_settings.log_file = log_file_path;

        // Check handle limit setting
        let file = File::open(&ckpe_ini_path)
            .map_err(|e| format!("Error opening {}: {}", ckpe_ini_path.display(), e))?;
        let reader = BufReader::new(file);

        let handle_setting_regex =
            Regex::new(&format!(r"{}=true", self.ckpe_settings.handle_setting)).unwrap();
        let mut handle_setting_enabled = false;

        for line in reader.lines() {
            let line =
                line.map_err(|e| format!("Error reading {}: {}", ckpe_ini_path.display(), e))?;
            if handle_setting_regex.is_match(&line) {
                handle_setting_enabled = true;
                break;
            }
        }

        if !handle_setting_enabled {
            warn!("Increased Reference Limit not enabled, Precombine Phase may fail.");
            warn!(
                "To fix, set {}=true in {}.",
                self.ckpe_settings.handle_setting, self.ckpe_settings.ini_file
            );
        }

        Ok(())
    }

    fn check_plugin(&mut self) -> Result<(), String> {
        info!("Checking plugin: {}", self.plugin_name_ext);

        let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
        let archive_path = self.paths.fallout4.join("Data").join(&self.plugin_archive);

        if archive_path.exists() {
            return Err(format!(
                "ERROR - This Plugin already has an Archive: {}",
                self.plugin_archive
            ));
        }

        if !plugin_path.exists() {
            // Plugin doesn't exist, try to use xPrevisPatch.esp as seed
            let seed_path = self.paths.fallout4.join("Data").join("xPrevisPatch.esp");

            if !seed_path.exists() {
                return Err("ERROR - Specified Plugin or xPrevisPatch does not exist".to_string());
            }

            if self.args.no_prompt {
                return Err(format!(
                    "ERROR - Plugin {} does not exist",
                    self.plugin_name_ext
                ));
            }

            if !self.prompt_yes_no(
                &"Plugin does not exist, Rename xPrevisPatch.esp to this? [Y/N]".to_string(),
            )? {
                return Err("User cancelled".to_string());
            }

            fs::copy(&seed_path, &plugin_path)
                .map_err(|e| format!("ERROR - Copy of seed to plugin failed: {}", e))?;

            // Wait a bit to ensure MO2 has finished processing
            sleep(Duration::from_secs(5));

            if !plugin_path.exists() {
                return Err("ERROR - Copy of seed to plugin failed".to_string());
            }
        }

        Ok(())
    }

    fn run_creation_kit(&self, action: &str, output_file: &str, args: &str) -> Result<(), String> {
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
            let dll_path = self.paths.fallout4.join(dll);
            if dll_path.exists() {
                let disabled_path = self.paths.fallout4.join(format!("{}-PJMdisabled", dll));
                fs::rename(&dll_path, &disabled_path)
                    .map_err(|e| format!("Error disabling {}: {}", dll, e))?;
            }
        }

        // Delete previous log if it exists
        if let Some(log_file) = &self.ckpe_settings.log_file {
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
            .open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;

        writeln!(log_file, "Running CK option {}:", action)
            .map_err(|e| format!("Error writing to log file: {}", e))?;
        writeln!(log_file, "====================================")
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        // Build command line
        let cmd_args = format!("-{}:\"{}\" {}", action, self.plugin_name_ext, args);

        // Run CreationKit
        let output = Command::new(&self.paths.creation_kit)
            .current_dir(&self.paths.fallout4)
            .args(cmd_args.split_whitespace())
            .output()
            .map_err(|e| format!("Error executing Creation Kit: {}", e))?;

        let exit_code = output.status.code().unwrap_or(-1);

        // Wait for MO2 to process files
        sleep(Duration::from_secs(5));

        // Append CK log to our log if it exists
        if let Some(log_file_path) = &self.ckpe_settings.log_file {
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
        let output_path = self.paths.fallout4.join("Data").join(output_file);
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
            let disabled_path = self.paths.fallout4.join(format!("{}-PJMdisabled", dll));
            if disabled_path.exists() {
                let dll_path = self.paths.fallout4.join(dll);
                fs::rename(&disabled_path, &dll_path)
                    .map_err(|e| format!("Error re-enabling {}: {}", dll, e))?;
            }
        }

        Ok(())
    }

    fn run_archive(&self, folders: &str, qualifiers: &str) -> Result<(), String> {
        info!("Creating archive {} for {}", self.plugin_archive, folders);

        let mut log_file = File::options()
            .append(true)
            .create(true)
            .open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;

        writeln!(
            log_file,
            "Creating {} Archive {} of {}:",
            qualifiers, self.plugin_archive, folders
        )
        .map_err(|e| format!("Error writing to log file: {}", e))?;
        writeln!(log_file, "====================================")
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        let args = format!(
            "{} -c=\"{}\" {} -f=General -q",
            folders, self.plugin_archive, qualifiers
        );

        let output = Command::new(&self.paths.archive2)
            .current_dir(self.paths.fallout4.join("Data"))
            .args(args.split_whitespace())
            .output()
            .map_err(|e| format!("Error executing Archive2: {}", e))?;

        let exit_code = output.status.code().unwrap_or(-1);

        writeln!(log_file, "{}", String::from_utf8_lossy(&output.stdout))
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        if exit_code != 0 {
            return Err(format!("ERROR - Archive2 failed with error {}", exit_code));
        }

        let archive_path = self.paths.fallout4.join("Data").join(&self.plugin_archive);
        if !archive_path.exists() {
            return Err("ERROR - No plugin archive Created".to_string());
        }

        Ok(())
    }

    fn extract_archive(&self) -> Result<(), String> {
        info!("Extracting archive {}", self.plugin_archive);

        let mut log_file = File::options()
            .append(true)
            .create(true)
            .open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;

        writeln!(log_file, "Extracting Archive {}:", self.plugin_archive)
            .map_err(|e| format!("Error writing to log file: {}", e))?;
        writeln!(log_file, "====================================")
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        let args = format!("{} -e=. -q", self.plugin_archive);

        let output = Command::new(&self.paths.archive2)
            .current_dir(self.paths.fallout4.join("Data"))
            .args(args.split_whitespace())
            .output()
            .map_err(|e| format!("Error executing Archive2: {}", e))?;

        let exit_code = output.status.code().unwrap_or(-1);

        writeln!(log_file, "{}", String::from_utf8_lossy(&output.stdout))
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        if exit_code != 0 {
            return Err(format!(
                "ERROR - Archive2 Extract failed with error {}",
                exit_code
            ));
        }

        Ok(())
    }

    fn add_to_archive(&self, folder: &str) -> Result<(), String> {
        let archive_path = self.paths.fallout4.join("Data").join(&self.plugin_archive);

        if !archive_path.exists() {
            return self.run_archive(folder, self.get_archive_qualifiers());
        }

        // Extract existing archive
        self.extract_archive()?;

        // Wait a bit
        sleep(Duration::from_secs(5));

        // Delete the existing archive
        fs::remove_file(archive_path)
            .map_err(|e| format!("Error removing existing archive: {}", e))?;

        // Check if we have precombined meshes
        let precombined_dir = self
            .paths
            .fallout4
            .join("Data")
            .join("meshes")
            .join("precombined");
        let has_precombined = precombined_dir.exists()
            && fs::read_dir(&precombined_dir)
                .map(|entries| entries.count() > 0)
                .unwrap_or(false);

        if has_precombined {
            // Archive both directories
            self.run_archive(
                &format!("meshes\\precombined,{}", folder),
                self.get_archive_qualifiers(),
            )?;

            // Clean up
            fs::remove_dir_all(precombined_dir)
                .map_err(|e| format!("Error removing precombined directory: {}", e))?;
        } else {
            // Just archive the new folder
            self.run_archive(folder, self.get_archive_qualifiers())?;
        }

        Ok(())
    }

    fn get_archive_qualifiers(&self) -> &'static str {
        match self.args.mode {
            BuildMode::Xbox => "-compression=XBox",
            _ => "",
        }
    }

    fn run_xedit_script(&self, script: &str, plugin1: &str, plugin2: &str) -> Result<(), String> {
        info!("Running xEdit script {} against {}", script, plugin1);

        let mut log_file = File::options()
            .append(true)
            .create(true)
            .open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;

        writeln!(
            log_file,
            "Running xEdit script {} against {}",
            script, plugin1
        )
        .map_err(|e| format!("Error writing to log file: {}", e))?;
        writeln!(log_file, "====================================")
            .map_err(|e| format!("Error writing to log file: {}", e))?;

        // Create plugins list
        let plugins_file = env::temp_dir().join("Plugins.txt");
        let mut file = File::create(&plugins_file)
            .map_err(|e| format!("Error creating plugins file: {}", e))?;

        writeln!(file, "*{}", plugin1)
            .map_err(|e| format!("Error writing to plugins file: {}", e))?;
        writeln!(file, "*{}", plugin2)
            .map_err(|e| format!("Error writing to plugins file: {}", e))?;

        // Delete previous log if it exists
        if self.unattended_logfile.exists() {
            fs::remove_file(&self.unattended_logfile)
                .map_err(|e| format!("Error removing unattended log file: {}", e))?;
        }

        // Start xEdit process
        let _script_path = format!(
            "{}\\Edit Scripts\\{}",
            self.paths.fo4edit.parent().unwrap().display(),
            script
        );

        let mut xedit_process = Command::new(&self.paths.fo4edit)
            .args(&[
                "-fo4",
                "-autoexit",
                format!("-P:{}", plugins_file.display()).as_str(),
                format!("-Script:{}", script).as_str(),
                format!("-Mod:{}", plugin1).as_str(),
                format!("-log:{}", self.unattended_logfile.display()).as_str(),
            ])
            .spawn()
            .map_err(|e| format!("Error starting xEdit: {}", e))?;

        // Wait for xEdit to start processing
        sleep(Duration::from_secs(5));

        // This part is tricky in Rust - we need to simulate keypresses to activate xEdit
        // In a proper implementation, we'd use the winapi or similar to send keys
        // For now, we'll just wait for the process to exit on its own

        // Wait for script to finish and create log
        while !self.unattended_logfile.exists() {
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
        if self.unattended_logfile.exists() {
            let xedit_log = fs::read_to_string(&self.unattended_logfile)
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

    fn prompt_yes_no(&self, message: &str) -> Result<bool, String> {
        if self.args.no_prompt {
            return Ok(true);
        }

        println!("{}", message);
        print!("[Y/N]? ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Error reading input: {}", e))?;

        Ok(input.trim().to_lowercase().starts_with('y'))
    }

    fn run(&mut self) -> Result<(), String> {
        // Initialize log file
        if let Ok(mut file) = File::create(&self.logfile) {
            writeln!(
                file,
                "Starting Previsbine Builder for plugin {}",
                self.plugin_name_ext
            )
            .map_err(|e| format!("Error writing to log file: {}", e))?;
        }

        // Show header
        println!("=================================================================");
        println!("Automatic Previsbine Builder (V2.6 Rust port Mar 2025)");
        println!("If you use MO2 then this must be run from within MO2");
        println!();

        // Always verify environment first - this is critical
        self.verify_environment()?;

        // Determine starting stage
        let start_stage = if let Some(stage) = self.args.start_stage {
            match BuildStage::from_i32(stage) {
                Some(stage) => {
                    // Check prerequisites for this stage
                    self.check_stage_prerequisites(stage)?;
                    stage
                }
                None => {
                    return Err(format!("ERROR - Invalid stage number: {}", stage));
                }
            }
        } else if self.plugin_name.is_empty() {
            // No plugin specified on command line
            if !self.args.no_prompt {
                self.prompt_for_plugin_name()?;
            } else {
                return Err("ERROR - No plugin specified and --no-prompt was used".to_string());
            }
            BuildStage::VerifyEnvironment
        } else {
            // Plugin specified but check if it already exists
            let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
            if plugin_path.exists() && !self.args.no_prompt {
                // Plugin already exists, prompt for stage
                self.prompt_for_stage(&self.args.mode)?
            } else {
                BuildStage::VerifyEnvironment
            }
        };

        // Get stage as integer for comparisons
        let start_stage_val = start_stage as i32;

        // Always verify environment unless skipped
        if start_stage_val <= BuildStage::VerifyEnvironment as i32 {
            self.verify_environment()?;
            self.check_plugin()?;
        }

        // Precombine phase
        if start_stage_val <= BuildStage::GeneratePrecombines as i32 {
            let precombined_dir = self
                .paths
                .fallout4
                .join("Data")
                .join("meshes")
                .join("precombined");
            let has_precombined = self.directory_has_files(&precombined_dir, ".nif");

            if has_precombined {
                return Err(
                    "ERROR - Precombine directory (Data\\meshes\\precombined) not empty"
                        .to_string(),
                );
            }

            let vis_dir = self.paths.fallout4.join("Data").join("vis");
            let has_vis = self.directory_has_files(&vis_dir, ".uvd");

            if has_vis {
                return Err("ERROR - Previs directory (Data\\vis) not empty".to_string());
            }

            // Delete working files if they exist
            let combined_objects_esp = self.paths.fallout4.join("Data").join("CombinedObjects.esp");
            if combined_objects_esp.exists() {
                fs::remove_file(&combined_objects_esp)
                    .map_err(|e| format!("Error removing CombinedObjects.esp: {}", e))?;
            }

            let geometry_psg_path = self
                .paths
                .fallout4
                .join("Data")
                .join(format!("{} - Geometry.psg", self.plugin_name));
            if geometry_psg_path.exists() {
                fs::remove_file(&geometry_psg_path)
                    .map_err(|e| format!("Error removing Geometry.psg: {}", e))?;
            }

            // Generate precombined
            if self.args.mode == BuildMode::Clean {
                self.run_creation_kit("GeneratePrecombined", "CombinedObjects.esp", "clean all")?;

                // Check PSG was created
                if !self
                    .paths
                    .fallout4
                    .join("Data")
                    .join(format!("{} - Geometry.psg", self.plugin_name))
                    .exists()
                {
                    return Err("ERROR - GeneratePrecombined failed to create psg file".to_string());
                }
            } else {
                self.run_creation_kit(
                    "GeneratePrecombined",
                    "CombinedObjects.esp",
                    "filtered all",
                )?;
            }

            // Check if any precombines were created
            let new_has_precombined = self.directory_has_files(&precombined_dir, ".nif");

            if !new_has_precombined {
                return Err(
                    "ERROR - GeneratePrecombined failed to create any Precombines".to_string(),
                );
            }

            // Check for handle array error
            if let Some(log_file) = &self.ckpe_settings.log_file {
                if log_file.exists() {
                    let log_content = fs::read_to_string(log_file)
                        .map_err(|e| format!("Error reading CK log file: {}", e))?;

                    if log_content.contains("DEFAULT: OUT OF HANDLE ARRAY ENTRIES") {
                        return Err(
                            "ERROR - GeneratePrecombined ran out of Reference Handles".to_string()
                        );
                    }
                }
            }
        }

        // Merge combined objects
        if start_stage_val <= BuildStage::MergePrecombines as i32 {
            self.run_xedit_script(
                "Batch_FO4MergeCombinedObjectsAndCheck.pas",
                &self.plugin_name_ext,
                "CombinedObjects.esp",
            )?;

            // Check for errors in log
            if self.unattended_logfile.exists() {
                let log_content = fs::read_to_string(&self.unattended_logfile)
                    .map_err(|e| format!("Error reading unattended log file: {}", e))?;

                if log_content.contains("Error: ") {
                    warn!("WARNING - Merge Precombines had errors");
                }
            }
        }

        // Archive precombines
        if start_stage_val <= BuildStage::ArchivePrecombines as i32 {
            let precombined_dir = self
                .paths
                .fallout4
                .join("Data")
                .join("meshes")
                .join("precombined");
            let has_precombined = self.directory_has_files(&precombined_dir, ".nif");

            if has_precombined {
                self.run_archive("meshes\\precombined", self.get_archive_qualifiers())?;

                // Clean up
                fs::remove_dir_all(precombined_dir)
                    .map_err(|e| format!("Error removing precombined directory: {}", e))?;
            }
        }

        // Compress PSG (if in clean mode)
        if start_stage_val <= BuildStage::CompressPsg as i32 && self.args.mode == BuildMode::Clean {
            let geometry_psg_path = self
                .paths
                .fallout4
                .join("Data")
                .join(format!("{} - Geometry.psg", self.plugin_name));

            if !geometry_psg_path.exists() {
                return Err("ERROR - No Geometry file to Compress".to_string());
            }

            let geometry_csg = format!("{} - Geometry.csg", self.plugin_name);
            self.run_creation_kit("CompressPSG", &geometry_csg, "")?;

            // Clean up PSG
            fs::remove_file(&geometry_psg_path)
                .map_err(|e| format!("Error removing Geometry.psg: {}", e))?;
        }

        // Build CDX (if in clean mode)
        if start_stage_val <= BuildStage::BuildCdx as i32 && self.args.mode == BuildMode::Clean {
            let cdx_file = format!("{}.cdx", self.plugin_name);
            self.run_creation_kit("BuildCDX", &cdx_file, "")?;
        }

        // Start previs phase
        if start_stage_val <= BuildStage::GeneratePrevis as i32 {
            let previs_esp_path = self.paths.fallout4.join("Data").join("Previs.esp");
            if previs_esp_path.exists() {
                fs::remove_file(&previs_esp_path)
                    .map_err(|e| format!("Error removing Previs.esp: {}", e))?;
            }

            // Generate previs data
            self.run_creation_kit("GeneratePreVisData", "Previs.esp", "clean all")?;

            // Check for visibility task errors
            if let Some(log_file) = &self.ckpe_settings.log_file {
                if log_file.exists() {
                    let log_content = fs::read_to_string(log_file)
                        .map_err(|e| format!("Error reading CK log file: {}", e))?;

                    if log_content.contains("ERROR: visibility task did not complete.") {
                        warn!(
                            "WARNING - GeneratePreVisData failed to build at least one Cluster uvd"
                        );
                    }
                }
            }

            // Check if any vis files were created
            let vis_dir = self.paths.fallout4.join("Data").join("vis");
            let has_vis = self.directory_has_files(&vis_dir, ".uvd");

            if !has_vis {
                return Err("ERROR - No Visibility files Generated".to_string());
            }

            if !self.paths.fallout4.join("Data").join("Previs.esp").exists() {
                return Err("ERROR - No Previs.esp Generated".to_string());
            }
        }

        // Merge previs
        if start_stage_val <= BuildStage::MergePrevis as i32 {
            self.run_xedit_script(
                "Batch_FO4MergePrevisandCleanRefr.pas",
                &self.plugin_name_ext,
                "Previs.esp",
            )?;

            // Check for errors in log
            if self.unattended_logfile.exists() {
                let log_content = fs::read_to_string(&self.unattended_logfile)
                    .map_err(|e| format!("Error reading unattended log file: {}", e))?;

                if !log_content.contains("Completed: No Errors.") {
                    warn!("WARNING - Merge Previs had errors");
                }
            }
        }

        // Archive vis files
        if start_stage_val <= BuildStage::ArchiveVis as i32 {
            let vis_dir = self.paths.fallout4.join("Data").join("vis");
            let has_vis = self.directory_has_files(&vis_dir, ".uvd");

            if !has_vis {
                warn!("WARNING - No Visibility files found to archive");
            } else {
                self.add_to_archive("vis")?;

                // Clean up
                fs::remove_dir_all(vis_dir)
                    .map_err(|e| format!("Error removing vis directory: {}", e))?;
            }
        }

        // Show completion info
        println!("Build of Patch {} Complete.", self.plugin_name);
        println!("=====================================================");
        println!("Patch Files created:");
        println!("    {}", self.plugin_name_ext);

        if self.args.mode == BuildMode::Clean {
            println!("    {} - Geometry.csg", self.plugin_name);
            println!("    {}.cdx", self.plugin_name);
        }

        println!("    {}", self.plugin_archive);
        println!();
        println!("Move ALL these files into a zip/7z archive and install it");
        println!("=====================================================");

        // Clean up
        if !self.args.keep_files {
            if !self.args.no_prompt && !self.prompt_yes_no("Remove working files [Y]?")? {
                return Ok(());
            }

            let combined_objects_esp = self.paths.fallout4.join("Data").join("CombinedObjects.esp");
            if combined_objects_esp.exists() {
                fs::remove_file(&combined_objects_esp)
                    .map_err(|e| format!("Error removing CombinedObjects.esp: {}", e))?;
            }

            let previs_esp = self.paths.fallout4.join("Data").join("Previs.esp");
            if previs_esp.exists() {
                fs::remove_file(&previs_esp)
                    .map_err(|e| format!("Error removing Previs.esp: {}", e))?;
            }
        }

        // Re-enable any ENB/ReShade DLLs that might have been disabled
        let dlls_to_reenable = [
            "d3d11.dll",
            "d3d10.dll",
            "d3d9.dll",
            "dxgi.dll",
            "enbimgui.dll",
            "d3dcompiler_46e.dll",
        ];

        for dll in dlls_to_reenable.iter() {
            let disabled_path = self.paths.fallout4.join(format!("{}-PJMdisabled", dll));
            if disabled_path.exists() {
                let dll_path = self.paths.fallout4.join(dll);
                fs::rename(&disabled_path, &dll_path)
                    .map_err(|e| format!("Error re-enabling {}: {}", dll, e))?;
            }
        }

        info!("See log at {}", self.logfile.display());

        Ok(())
    }
}

fn main() {
    // Initialize logging
    env_logger::init();

    // Parse command line arguments
    let args = Args::parse();

    // Create builder
    match PrevisbineBuilder::new(args) {
        Ok(mut builder) => {
            // If plugin name is empty but there's a start_stage, display available stages
            if builder.plugin_name.is_empty() && builder.args.start_stage.is_some() {
                builder.display_stages();
                std::process::exit(0);
            }

            if let Err(e) = builder.run() {
                error!("{}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("{}", e);
            std::process::exit(1);
        }
    }
}
