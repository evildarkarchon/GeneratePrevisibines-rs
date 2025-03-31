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
        let archive2_path = fallout4_path.join("tools").join("archive2").join("archive2.exe");

        // Extract plugin name
        let (plugin_name, plugin_name_ext) = if let Some(plugin) = args.plugin.clone() {
            let plugin_lowercase = plugin.to_lowercase();
            if plugin_lowercase.ends_with(".esp") || plugin_lowercase.ends_with(".esm") || plugin_lowercase.ends_with(".esl") {
                let name = plugin.clone();
                let name_without_ext = name.rfind('.').map(|i| &name[0..i]).unwrap_or(&name).to_string();
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

    fn find_fo4edit() -> Result<PathBuf, String> {
        // First check current directory
        let current_dir = env::current_dir().map_err(|e| format!("Error getting current directory: {}", e))?;
        
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
        if let Ok(subkey) = hklm.open_subkey("SOFTWARE\\Wow6432Node\\Bethesda Softworks\\Fallout4") {
            if let Ok(path) = subkey.get_value::<String, _>("installed path") {
                return Ok(PathBuf::from(path));
            }
        }

        Err("Fallout 4 installation not found. Please specify path with --fallout4_path".to_string())
    }

    fn verify_environment(&mut self) -> Result<(), String> {
        // Check FO4Edit
        if !self.paths.fo4edit.exists() {
            return Err(format!("ERROR - {} not found", self.paths.fo4edit.display()));
        }

        // Check Fallout4.exe
        let fallout4_exe = self.paths.fallout4.join("Fallout4.exe");
        if !fallout4_exe.exists() {
            return Err(format!("ERROR - Fallout4.exe not found at {}", fallout4_exe.display()));
        }

        // Check Creation Kit
        if !self.paths.creation_kit.exists() {
            return Err("ERROR - CreationKit.exe not found. Creation Kit must be installed".to_string());
        }

        // Check CKPE (winhttp.dll)
        let winhttp_dll = self.paths.fallout4.join("winhttp.dll");
        if !winhttp_dll.exists() {
            return Err("ERROR - CKPE not installed. You may not get a successful Patch without it".to_string());
        }

        // Check Archive2.exe
        if !self.paths.archive2.exists() {
            return Err("ERROR - Archive2.exe not found. Creation Kit not properly installed".to_string());
        }

        // Check CKPE ini
        let ckpe_ini_path = self.paths.fallout4.join(&self.ckpe_settings.ini_file);
        if !ckpe_ini_path.exists() {
            let fallout4_test_ini = self.paths.fallout4.join("fallout4_test.ini");
            if !fallout4_test_ini.exists() {
                return Err("ERROR - CKPE not installed properly. No settings file found".to_string());
            }
            self.ckpe_settings.ini_file = "fallout4_test.ini".to_string();
            self.ckpe_settings.handle_setting = "BSHandleRefObjectPatch".to_string();
            self.ckpe_settings.log_setting = "OutputFile".to_string();
        }

        // Check for required scripts
        let edit_scripts_dir = self.paths.fo4edit.parent().unwrap().join("Edit Scripts");
        let script_paths = [
            (edit_scripts_dir.join("Batch_FO4MergePrevisandCleanRefr.pas"), "V2.2"),
            (edit_scripts_dir.join("Batch_FO4MergeCombinedObjectsAndCheck.pas"), "V1.5"),
        ];

        for (script_path, version) in &script_paths {
            if !script_path.exists() {
                return Err(format!("ERROR - Required xEdit Script {} missing", script_path.display()));
            }

            // Check script version
            let mut file = File::open(script_path).map_err(|e| format!("Error opening {}: {}", script_path.display(), e))?;
            let mut content = String::new();
            file.read_to_string(&mut content).map_err(|e| format!("Error reading {}: {}", script_path.display(), e))?;
            
            if !content.contains(version) {
                return Err(format!("ERROR - Old Script {} found, {} required", script_path.display(), version));
            }
        }

        // Check CK log redirection
        let ckpe_ini_path = self.paths.fallout4.join(&self.ckpe_settings.ini_file);
        let mut log_file_path: Option<PathBuf> = None;
        
        let file = File::open(&ckpe_ini_path).map_err(|e| format!("Error opening {}: {}", ckpe_ini_path.display(), e))?;
        let reader = BufReader::new(file);
        
        let log_setting_regex = Regex::new(&format!(r"{}=(.+)", self.ckpe_settings.log_setting)).unwrap();
        
        for line in reader.lines() {
            let line = line.map_err(|e| format!("Error reading {}: {}", ckpe_ini_path.display(), e))?;
            if let Some(captures) = log_setting_regex.captures(&line) {
                if let Some(log_file) = captures.get(1) {
                    log_file_path = Some(self.paths.fallout4.join(log_file.as_str()));
                    break;
                }
            }
        }
        
        if log_file_path.is_none() {
            return Err(format!("ERROR - CK Logging not set in this ini. To fix, set {}=CK.log in it.", self.ckpe_settings.log_setting));
        }
        
        self.ckpe_settings.log_file = log_file_path;
        
        // Check handle limit setting
        let file = File::open(&ckpe_ini_path).map_err(|e| format!("Error opening {}: {}", ckpe_ini_path.display(), e))?;
        let reader = BufReader::new(file);
        
        let handle_setting_regex = Regex::new(&format!(r"{}=true", self.ckpe_settings.handle_setting)).unwrap();
        let mut handle_setting_enabled = false;
        
        for line in reader.lines() {
            let line = line.map_err(|e| format!("Error reading {}: {}", ckpe_ini_path.display(), e))?;
            if handle_setting_regex.is_match(&line) {
                handle_setting_enabled = true;
                break;
            }
        }
        
        if !handle_setting_enabled {
            warn!("Increased Reference Limit not enabled, Precombine Phase may fail.");
            warn!("To fix, set {}=true in {}.", self.ckpe_settings.handle_setting, self.ckpe_settings.ini_file);
        }

        Ok(())
    }

    fn check_plugin(&mut self) -> Result<(), String> {
        info!("Checking plugin: {}", self.plugin_name_ext);
        
        let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
        let archive_path = self.paths.fallout4.join("Data").join(&self.plugin_archive);
        
        if archive_path.exists() {
            return Err(format!("ERROR - This Plugin already has an Archive: {}", self.plugin_archive));
        }
        
        if !plugin_path.exists() {
            // Plugin doesn't exist, try to use xPrevisPatch.esp as seed
            let seed_path = self.paths.fallout4.join("Data").join("xPrevisPatch.esp");
            
            if !seed_path.exists() {
                return Err("ERROR - Specified Plugin or xPrevisPatch does not exist".to_string());
            }
            
            if self.args.no_prompt {
                return Err(format!("ERROR - Plugin {} does not exist", self.plugin_name_ext));
            }
            
            if !self.prompt_yes_no(&format!("Plugin does not exist, Rename xPrevisPatch.esp to this? [Y/N]"))? {
                return Err("User cancelled".to_string());
            }
            
            fs::copy(&seed_path, &plugin_path).map_err(|e| format!("ERROR - Copy of seed to plugin failed: {}", e))?;
            
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
            "d3d11.dll", "d3d10.dll", "d3d9.dll", "dxgi.dll", 
            "enbimgui.dll", "d3dcompiler_46e.dll"
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
                fs::remove_file(log_file).map_err(|e| format!("Error removing log file {}: {}", log_file.display(), e))?;
            }
        }
        
        // Log to our logfile
        let mut log_file = File::options().append(true).create(true).open(&self.logfile)
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
                let ck_log = fs::read_to_string(log_file_path)
                    .map_err(|e| format!("Error reading CK log file {}: {}", log_file_path.display(), e))?;
                    
                writeln!(log_file, "{}", ck_log)
                    .map_err(|e| format!("Error writing to log file: {}", e))?;
            }
        }
        
        // Check if output file was created
        let output_path = self.paths.fallout4.join("Data").join(output_file);
        if !output_path.exists() {
            return Err(format!("ERROR - {} failed to create file {} with exit status {}", 
                action, output_file, exit_code));
        }
        
        if exit_code != 0 {
            warn!("WARNING - {} ended with error {} but seemed to finish so error ignored.", action, exit_code);
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
        
        let mut log_file = File::options().append(true).create(true).open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;
            
        writeln!(log_file, "Creating {} Archive {} of {}:", qualifiers, self.plugin_archive, folders)
            .map_err(|e| format!("Error writing to log file: {}", e))?;
        writeln!(log_file, "====================================")
            .map_err(|e| format!("Error writing to log file: {}", e))?;
            
        let args = format!("{} -c=\"{}\" {} -f=General -q", folders, self.plugin_archive, qualifiers);
        
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
        
        let mut log_file = File::options().append(true).create(true).open(&self.logfile)
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
            return Err(format!("ERROR - Archive2 Extract failed with error {}", exit_code));
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
        let precombined_dir = self.paths.fallout4.join("Data").join("meshes").join("precombined");
        let has_precombined = precombined_dir.exists() && 
            fs::read_dir(&precombined_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if has_precombined {
            // Archive both directories
            self.run_archive(&format!("meshes\\precombined,{}", folder), self.get_archive_qualifiers())?;
            
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
        
        let mut log_file = File::options().append(true).create(true).open(&self.logfile)
            .map_err(|e| format!("Error opening log file {}: {}", self.logfile.display(), e))?;
            
        writeln!(log_file, "Running xEdit script {} against {}", script, plugin1)
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
        let _script_path = format!("{}\\Edit Scripts\\{}", 
            self.paths.fo4edit.parent().unwrap().display(), script);
            
        let mut xedit_process = Command::new(&self.paths.fo4edit)
            .args(&[
                "-fo4", 
                "-autoexit", 
                format!("-P:{}", plugins_file.display()).as_str(),
                format!("-Script:{}", script).as_str(),
                format!("-Mod:{}", plugin1).as_str(),
                format!("-log:{}", self.unattended_logfile.display()).as_str()
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
            return Err(format!("ERROR - FO4Edit script {} did not produce a log file", script));
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
        io::stdin().read_line(&mut input).map_err(|e| format!("Error reading input: {}", e))?;
        
        Ok(input.trim().to_lowercase().starts_with('y'))
    }
    
    fn run(&mut self) -> Result<(), String> {
        // Initialize log file
        if let Ok(mut file) = File::create(&self.logfile) {
            writeln!(file, "Starting Previsbine Builder for plugin {}", self.plugin_name_ext)
                .map_err(|e| format!("Error writing to log file: {}", e))?;
        }
        
        // Show header
        println!("=================================================================");
        println!("Automatic Previsbine Builder (V2.6 Rust port Mar 2025)");
        println!("If you use MO2 then this must be run from within MO2");
        println!();
        
        // Verify environment
        self.verify_environment()?;
        
        // Check plugin
        self.check_plugin()?;
        
        // Precombine phase
        let precombined_dir = self.paths.fallout4.join("Data").join("meshes").join("precombined");
        let has_precombined = precombined_dir.exists() && 
            fs::read_dir(&precombined_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if has_precombined {
            return Err("ERROR - Precombine directory (Data\\meshes\\precombined) not empty".to_string());
        }
        
        let vis_dir = self.paths.fallout4.join("Data").join("vis");
        let has_vis = vis_dir.exists() && 
            fs::read_dir(&vis_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if has_vis {
            return Err("ERROR - Previs directory (Data\\vis) not empty".to_string());
        }
        
        // Delete working files if they exist
        let combined_objects_esp = self.paths.fallout4.join("Data").join("CombinedObjects.esp");
        if combined_objects_esp.exists() {
            fs::remove_file(&combined_objects_esp)
                .map_err(|e| format!("Error removing CombinedObjects.esp: {}", e))?;
        }
        
        let geometry_psg_path = self.paths.fallout4.join("Data")
            .join(format!("{} - Geometry.psg", self.plugin_name));
        if geometry_psg_path.exists() {
            fs::remove_file(&geometry_psg_path)
                .map_err(|e| format!("Error removing Geometry.psg: {}", e))?;
        }
        
        // Generate precombined
        if self.args.mode == BuildMode::Clean {
            self.run_creation_kit("GeneratePrecombined", "CombinedObjects.esp", "clean all")?;
            
            // Check PSG was created
            if !self.paths.fallout4.join("Data")
                .join(format!("{} - Geometry.psg", self.plugin_name)).exists() {
                return Err("ERROR - GeneratePrecombined failed to create psg file".to_string());
            }
        } else {
            self.run_creation_kit("GeneratePrecombined", "CombinedObjects.esp", "filtered all")?;
        }
        
        // Check if any precombines were created
        let new_has_precombined = precombined_dir.exists() && 
            fs::read_dir(&precombined_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if !new_has_precombined {
            return Err("ERROR - GeneratePrecombined failed to create any Precombines".to_string());
        }
        
        // Check for handle array error
        if let Some(log_file) = &self.ckpe_settings.log_file {
            if log_file.exists() {
                let log_content = fs::read_to_string(log_file)
                    .map_err(|e| format!("Error reading CK log file: {}", e))?;
                    
                if log_content.contains("DEFAULT: OUT OF HANDLE ARRAY ENTRIES") {
                    return Err("ERROR - GeneratePrecombined ran out of Reference Handles".to_string());
                }
            }
        }
        
        // Merge combined objects
        self.run_xedit_script("Batch_FO4MergeCombinedObjectsAndCheck.pas", 
            &self.plugin_name_ext, "CombinedObjects.esp")?;
            
        // Check for errors in log
        if self.unattended_logfile.exists() {
            let log_content = fs::read_to_string(&self.unattended_logfile)
                .map_err(|e| format!("Error reading unattended log file: {}", e))?;
                
            if log_content.contains("Error: ") {
                warn!("WARNING - Merge Precombines had errors");
            }
        }
        
        // Archive precombines
        let new_new_has_precombined = precombined_dir.exists() && 
            fs::read_dir(&precombined_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if new_new_has_precombined {
            self.run_archive("meshes\\precombined", self.get_archive_qualifiers())?;
            
            // Clean up
            fs::remove_dir_all(precombined_dir)
                .map_err(|e| format!("Error removing precombined directory: {}", e))?;
        }
        
        // Compress PSG (if in clean mode)
        if self.args.mode == BuildMode::Clean {
            let geometry_psg_path = self.paths.fallout4.join("Data")
                .join(format!("{} - Geometry.psg", self.plugin_name));
                
            if !geometry_psg_path.exists() {
                return Err("ERROR - No Geometry file to Compress".to_string());
            }
            
            let geometry_csg = format!("{} - Geometry.csg", self.plugin_name);
            self.run_creation_kit("CompressPSG", &geometry_csg, "")?;
            
            // Clean up PSG
            fs::remove_file(&geometry_psg_path)
                .map_err(|e| format!("Error removing Geometry.psg: {}", e))?;
                
            // Build CDX
            let cdx_file = format!("{}.cdx", self.plugin_name);
            self.run_creation_kit("BuildCDX", &cdx_file, "")?;
        }
        
        // Start previs phase
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
                    warn!("WARNING - GeneratePreVisData failed to build at least one Cluster uvd");
                }
            }
        }
        
        // Check if any vis files were created
        let has_new_vis = vis_dir.exists() && 
            fs::read_dir(&vis_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if !has_new_vis {
            return Err("ERROR - No Visibility files Generated".to_string());
        }
        
        if !self.paths.fallout4.join("Data").join("Previs.esp").exists() {
            return Err("ERROR - No Previs.esp Generated".to_string());
        }
        
        // Merge previs
        self.run_xedit_script("Batch_FO4MergePrevisandCleanRefr.pas", 
            &self.plugin_name_ext, "Previs.esp")?;
            
        // Check for errors in log
        if self.unattended_logfile.exists() {
            let log_content = fs::read_to_string(&self.unattended_logfile)
                .map_err(|e| format!("Error reading unattended log file: {}", e))?;
                
            if !log_content.contains("Completed: No Errors.") {
                warn!("WARNING - Merge Previs had errors");
            }
        }
        
        // Archive vis files
        let final_has_vis = vis_dir.exists() && 
            fs::read_dir(&vis_dir).map(|entries| entries.count() > 0).unwrap_or(false);
            
        if !final_has_vis {
            warn!("WARNING - No Visibility files found to archive");
        } else {
            self.add_to_archive("vis")?;
            
            // Clean up
            fs::remove_dir_all(vis_dir)
                .map_err(|e| format!("Error removing vis directory: {}", e))?;
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
            if !self.args.no_prompt && 
               !self.prompt_yes_no("Remove working files [Y]?")? {
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
            "d3d11.dll", "d3d10.dll", "d3d9.dll", "dxgi.dll", 
            "enbimgui.dll", "d3dcompiler_46e.dll"
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