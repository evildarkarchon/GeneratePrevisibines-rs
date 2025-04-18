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

    /// Use BSArch instead of Archive2
    #[arg(short, long)]
    use_bsarch: bool,
    
    /// BSArch Path (Requires --use-bsarch)
    #[arg(long)]
    bsarch_path: Option<String>,
}

struct Paths {
    fo4edit: PathBuf,
    fallout4: PathBuf,
    creation_kit: PathBuf,
    archive2: PathBuf,
    bsarch: Option<PathBuf>,
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
    /// Creates a new `PrevisbineBuilder` instance by setting up necessary paths, determining plugin details, and configuring related settings.
    ///
    /// # Arguments
    /// * `args` - An instance of `Args` containing input parameters passed to the builder.
    ///
    /// # Returns
    /// A `Result` containing:
    /// * `Ok(Self)` - A successfully constructed `PrevisbineBuilder` instance.
    /// * `Err(String)` - An error message if any of the required paths or configurations cannot be determined.
    ///
    /// # Workflow
    /// 1. **FO4Edit Path**:
    ///     - Uses `args.fo4edit_path` if provided, otherwise attempts to find the path through `Self::find_fo4edit()`.
    ///
    /// 2. **Fallout 4 Path**:
    ///     - Uses `args.fallout4_path` if provided, otherwise attempts to find the path through `Self::find_fallout4()`.
    ///
    /// 3. **Creation Kit and Archive2 Paths**:
    ///     - Automatically resolves paths for `CreationKit.exe` and `Archive2.exe` based on the Fallout 4 installation directory.
    ///
    /// 4. **BSArch Path**:
    ///     - If `args.use_bsarch` is `true`:
    ///         - Uses `args.bsarch_path` if provided.
    ///         - Otherwise, searches for BSArch in common locations (e.g., `tools/BSArch/bsarch.exe`).
    ///         - If not found, logs a warning and sets the path to `None` pending environment verification.
    ///
    /// 5. **Plugin Name and Extension**:
    ///     - Extracts and processes the plugin name from `args.plugin`.
    ///     - If the provided plugin name includes an `.esp`, `.esm`, or `.esl` extension, it is preserved.
    ///     - Otherwise, appends `.esp` as the default extension.
    ///
    /// 6. **Temporary and Log Files**:
    ///     - Configures temporary directory paths for log file storage.
    ///     - Sets up both a general log file and a log specifically for unattended scripts.
    ///
    /// 7. **Creation Kit Platform Extended (CKPE) Settings**:
    ///     - Prepares CKPE-specific settings such as the relevant `.ini` file, handle and log settings, and log file configuration.
    ///
    /// 8. **Path Struct**:
    ///     - Combines resolved paths into a `Paths` struct for easy management of required executables and directories.
    ///
    /// 9. **Plugin Archive**:
    ///     - Constructs the plugin archive filename based on the resolved plugin name.
    ///
    /// # Returns
    /// If all paths and settings are configured successfully, the function initializes and returns a new `PrevisbineBuilder` instance:
    /// * `args` - The input arguments passed to the function.
    /// * `paths` - Resolved and packaged paths for required tools and directories.
    /// * `ckpe_settings` - Configuration for CKPE.
    /// * `plugin_name` - Processed plugin name without file extension.
    /// * `plugin_name_ext` - Full plugin name including extension.
    /// * `plugin_archive` - Archive file name associated with the plugin.
    /// * `logfile` - Path to the primary log file.
    /// * `unattended_logfile` - Path to the unattended script log file.
    ///
    /// # Errors
    /// This function may return an error (`Err(String)`) in the following cases:
    /// * The FO4Edit path cannot be resolved.
    /// * The Fallout 4 path cannot be located successfully.
    ///
    /// # Examples
    /// ```rust
    /// let args = Args {
    ///     fo4edit_path: Some(String::from("path/to/FO4Edit.exe")),
    ///     fallout4_path: Some(String::from("path/to/Fallout4")),
    ///     bsarch_path: None,
    ///     use_bsarch: false,
    ///     plugin: Some(String::from("MyPlugin")),
    /// };
    ///
    /// match PrevisbineBuilder::new(args) {
    ///     Ok(builder) => {
    ///         // Successfully created PrevisbineBuilder instance
    ///         println!("Builder created with plugin: {}", builder.plugin_name);
    ///     }
    ///     Err(err) => {
    ///         // Handle error
    ///         eprintln!("Error: {}", err);
    ///     }
    /// }
    /// ```
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

        // Handle BSArch path
        let bsarch_path = if args.use_bsarch {
            if let Some(path) = &args.bsarch_path {
                Some(PathBuf::from(path))
            } else {
                // Try to find BSArch in common locations
                let possible_paths = [
                    PathBuf::from("tools").join("BSArch").join("bsarch.exe"),
                    PathBuf::from("BSArch").join("bsarch.exe"),
                    PathBuf::from("bsarch.exe"),
                ];

                let found_path = possible_paths.iter()
                    .find(|p| p.exists())
                    .cloned();

                if found_path.is_none() {
                    warn!("BSArch enabled but path not specified and not found in common locations. Will check during environment verification.");
                }

                found_path
            }
        } else {
            None
        };



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
            bsarch: bsarch_path,
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

    /// Executes the bsarch.exe command with the given arguments.
    ///
    /// # Arguments
    /// * `action` - A descriptive string for the action being performed (e.g., "packing", "extracting"). Used for logging.
    /// * `bsarch_args` - A slice of string slices representing the arguments to pass to bsarch.exe.
    ///
    /// # Returns
    /// * `Ok(())` if the command executes successfully (exit code 0).
    /// * `Err(String)` if bsarch path is not configured, the command fails to start, or returns a non-zero exit code.
    fn run_bsarch(&self, action: &str, bsarch_args: &[&str]) -> Result<(), String> {
        // Ensure bsarch path is available
        let bsarch_path = self.paths.bsarch.as_ref()
            .ok_or_else(|| "BSArch path is not configured. Cannot run bsarch.".to_string())?;

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
                        status.code().unwrap_or(-1) // Provide a default if no code available
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


    ///
    /// Prompts the user to input a plugin name if none is specified. Processes the input to extract or assign
    /// a valid plugin name and its corresponding file extension. This function also updates the plugin name,
    /// its extension, and the associated plugin archive property within the object.
    ///
    /// # Returns
    ///
    /// - `Ok(())`: If the user inputs a valid plugin name.
    /// - `Err(String)`: If the user fails to input a plugin name or if there is an error reading input from the user.
    ///
    /// # Behavior
    ///
    /// - If the user provides a valid plugin name that ends with `.esp`, `.esm`, or `.esl`, 
    ///   the function assigns it directly to `self.plugin_name_ext` and stores the name without
    ///   the extension in `self.plugin_name`.
    /// - If the user provides a plugin name without an extension, `.esp` is appended by default
    ///   and assigned to `self.plugin_name_ext`, while the original input becomes `self.plugin_name`.
    /// - Additionally, the function constructs the value for `self.plugin_archive` by formatting 
    ///   the `self.plugin_name` with the suffix `- Main.ba2`.
    ///
    /// # Errors
    ///
    /// - If the input is empty after being trimmed or if there is an error reading from stdin, 
    ///   the function returns an appropriate error message.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut plugin_handler = PluginHandler {
    ///     plugin_name: String::new(),
    ///     plugin_name_ext: String::new(),
    ///     plugin_archive: String::new(),
    /// };
    ///
    /// if let Err(e) = plugin_handler.prompt_for_plugin_name() {
    ///     eprintln!("Failed to get plugin name: {}", e);
    /// } else {
    ///     println!("Plugin Name: {}", plugin_handler.plugin_name);
    ///     println!("Plugin Name with Extension: {}", plugin_handler.plugin_name_ext);
    ///     println!("Plugin Archive: {}", plugin_handler.plugin_archive);
    /// }
    /// ```
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

    
    /// Prompts the user to choose a build stage to start from and returns the selected stage.
    ///
    /// # Parameters
    /// - `self`: The instance of the struct implementing this function.
    /// - `build_mode`: A reference to the current `BuildMode`. This determines which stages are applicable for selection.
    ///
    /// # Returns
    /// Returns a `Result`:
    /// - `Ok(BuildStage)`: The build stage chosen by the user.
    /// - `Err(String)`: An error message if there is an issue with input or if the stage is invalid.
    ///
    /// # Behavior
    /// - Prints a list of build stages (excluding `VerifyEnvironment`).
    /// - Filters out stages based on the provided `build_mode`.
    ///   - For `BuildMode::Clean`, all stages are presented for selection.
    ///   - For other modes:
    ///     - `BuildStage::CompressPsg`
    ///     - `BuildStage::BuildCdx`
    ///     are excluded from the options.
    /// - Prompts the user to input a stage number and validates the input:
    ///   - Displays an error if the input is invalid (non-numeric or out of expected range).
    ///   - Returns a corresponding `BuildStage` instance for a valid input.
    ///
    /// # Input Format
    /// - The user is prompted to enter a number between 1 and 8 (inclusive) corresponding to a stage.
    /// - Non-numeric or non-existent stage numbers will result in a validation error.
    ///
    /// # Errors
    /// This function returns an error in the following cases:
    /// - Failure to read input from stdin.
    /// - The user enters an invalid stage number (non-numeric or unsupported).
    ///
    /// # Example
    /// ```no_run
    /// let build_mode = BuildMode::Clean;
    /// match instance.prompt_for_stage(&build_mode) {
    ///     Ok(stage) => println!("Selected stage: {:?}", stage),
    ///     Err(err) => println!("Error: {}", err),
    /// }
    /// ```
    ///
    /// # Notes
    /// - This function interacts directly with the console (stdout and stdin).
    /// - It uses the `BuildStage` enum and assumes numerical mapping to each build stage (via `from_i32`).
    /// - The `get_stage_description()` function is used to provide a human-readable description for each stage.
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

    
    /// Returns a string description of the specified build stage.
    ///
    /// This function maps each variant of the `BuildStage` enum
    /// to a corresponding static description that explains the purpose
    /// or objective of that stage in human-readable terms.
    ///
    /// # Arguments
    ///
    /// * `stage` - A `BuildStage` enum variant representing the current stage
    ///   of the build process.
    ///
    /// # Returns
    ///
    /// A static string slice (`&'static str`) describing the specified
    /// build stage.
    ///
    /// # Example
    ///
    /// ```rust
    /// let description = some_instance.get_stage_description(BuildStage::VerifyEnvironment);
    /// assert_eq!(description, "Verify Environment");
    /// ```
    ///
    /// # Build Stages Description
    ///
    /// - `BuildStage::VerifyEnvironment`:
    ///   "Verify Environment", meaning the system environment is being validated.
    /// - `BuildStage::GeneratePrecombines`:
    ///   "Generate Precombines Via CK", indicating the generation of precombines using the CK.
    /// - `BuildStage::MergePrecombines`:
    ///   "Merge PrecombineObjects.esp Via FO4Edit", specifies combining precombine ESPs utilizing FO4Edit.
    /// - `BuildStage::ArchivePrecombines`:
    ///   "Create BA2 Archive from Precombines", detailing the creation of BA2 archive files of the precombines.
    /// - `BuildStage::CompressPsg`:
    ///   "Compress PSG Via CK", relating to the compression of PSG files using the CK.
    /// - `BuildStage::BuildCdx`:
    ///   "Build CDX Via CK", describing the construction of CDX files with the CK.
    /// - `BuildStage::GeneratePrevis`:
    ///   "Generate Previs Via CK", referring to generating previs files with the CK.
    /// - `BuildStage::MergePrevis`:
    ///   "Merge Previs.esp Via FO4Edit", outlines merging previs ESP files using FO4Edit.
    /// - `BuildStage::ArchiveVis`:
    ///   "Add vis files to BA2 Archive", entails adding visibility files to a BA2 archive.
    ///
    /// # Note
    ///
    /// This method assumes that each `BuildStage` has a corresponding descriptive
    /// string. Any changes to the `BuildStage` enum will require updating
    /// this function accordingly.
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

    /// Displays the available stages for resuming a process and provides instructions for user input.
    ///
    /// This method outputs:
    /// 1. A header message indicating the available stages to resume from.
    /// 2. A list of stages, retrieved by calling `BuildStage::display_stages`, which is dependent on the mode provided in `self.args.mode`.
    /// 3. A prompt instructing the user to input a stage number (0 to 8) to resume from a specific stage or any other key to exit.
    ///
    /// # Example Output:
    /// ```text
    /// Available stages to resume from:
    /// [List of stages specific to the selected mode]
    /// Enter stage number (0-8) to resume from that stage, or any other key to exit.
    /// ```
    ///
    /// # Note:
    /// - This method does not handle the user's input or the resumption logic.
    /// - It assumes `BuildStage::display_stages` is a function that generates a meaningful, formatted list of stages as a string.
    ///
    /// # Preconditions:
    /// - `self.args.mode` must be properly initialized before calling this function.
    ///
    /// # Usage:
    /// Call this method to inform the user about the stages in a build process and their options for resuming the process.
    fn display_stages(&self) {
        println!("Available stages to resume from:");
        print!("{}", BuildStage::display_stages(&self.args.mode));
        println!("Enter stage number (0-8) to resume from that stage, or any other key to exit.");
    }

    
    /// Checks if the specified directory contains any files with the given file extension.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - A reference to a `PathBuf` that represents the path to the directory to be checked.
    /// * `extension` - A string slice representing the file extension to look for (e.g., `".txt"`) in the directory.
    ///
    /// # Returns
    ///
    /// * `true` if the directory contains at least one file with the specified extension.
    /// * `false` if the directory does not exist, cannot be read, or contains no files with the given extension.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// let directory = PathBuf::from("path/to/directory");
    /// let has_txt_files = directory_has_files(&directory, ".txt");
    ///
    /// if has_txt_files {
    ///     println!("The directory contains .txt files.");
    /// } else {
    ///     println!("The directory does not contain .txt files.");
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// This function performs a case-sensitive check when matching the file extension.
    /// If the directory cannot be read (e.g., due to permissions), the function returns `false`.
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
    /// Validates the prerequisites for various build stages required for a Fallout 4 modding pipeline.
    ///
    /// This function ensures that certain conditions are met before each build stage can proceed. The
    /// validations are specific to the given `BuildStage` and check for the existence of required files,
    /// directories, or configuration settings.
    ///
    /// # Parameters
    /// - `stage: BuildStage`: The build stage to validate prerequisites for.
    ///
    /// # Returns
    /// - `Result<(), String>`: 
    ///     - `Ok(())` if all prerequisites for the specified stage are met.
    ///     - `Err(String)` if any prerequisite is missing or invalid, with an error message describing the issue.
    ///
    /// # Build Stages and Corresponding Validations:
    ///
    /// - `BuildStage::VerifyEnvironment`:
    ///     - **Validation**: Always succeeds. No additional checks required.
    ///
    /// - `BuildStage::GeneratePrecombines`:
    ///     - **Validation**: Ensures the required plugin file `plugin_name_ext` exists in the `Data` directory.
    ///     - **Error**: `"ERROR - Plugin <plugin_name_ext> does not exist"`.
    ///
    /// - `BuildStage::MergePrecombines`:
    ///     - **Validation**: Ensures the `precombined` directory contains `.nif` files (precombined meshes).
    ///     - **Error**: `"ERROR - No precombined meshes found. Run GeneratePrecombines first."`.
    ///
    /// - `BuildStage::ArchivePrecombines`:
    ///     - **Validation**: Same check as `MergePrecombines`. Ensures `.nif` files exist in the `precombined` directory.
    ///     - **Error**: `"ERROR - No precombined meshes found. Run GeneratePrecombines first."`.
    ///
    /// - `BuildStage::CompressPsg`:
    ///     - **Validation**:
    ///         - This stage is only allowed when the `BuildMode` is `Clean`.
    ///         - Ensures the geometry `.psg` file exists in the `Data` directory.
    ///     - **Errors**:
    ///         - `"ERROR - CompressPSG is only available in Clean mode"`.
    ///         - `"ERROR - No Geometry.psg file found. Run GeneratePrecombines first."`.
    ///
    /// - `BuildStage::BuildCdx`:
    ///     - **Validation**: This stage is only allowed when the `BuildMode` is `Clean`.
    ///     - **Error**: `"ERROR - BuildCDX is only available in Clean mode"`.
    ///
    /// - `BuildStage::GeneratePrevis`:
    ///     - **Validation**: Ensures the required plugin file `plugin_name_ext` exists in the `Data` directory.
    ///     - **Error**: `"ERROR - Plugin <plugin_name_ext> does not exist"`.
    ///
    /// - `BuildStage::MergePrevis`:
    ///     - **Validations**:
    ///         - Ensures the `vis` directory contains `.uvd` files (visibility files).
    ///         - Ensures the `Previs.esp` file exists in the `Data` directory.
    ///     - **Errors**:
    ///         - `"ERROR - No visibility files found. Run GeneratePreVisData first."`.
    ///         - `"ERROR - Previs.esp not found. Run GeneratePreVisData first."`.
    ///
    /// - `BuildStage::ArchiveVis`:
    ///     - **Validation**: Same check as `MergePrevis`. Ensures `.uvd` files exist in the `vis` directory.
    ///     - **Error**: `"ERROR - No visibility files found. Run GeneratePreVisData first."`.
    ///
    /// # Notes
    /// - File and directory structure assumptions are based on the expected layout of Fallout 4's `Data` directory.
    /// - This function relies on helper methods like `directory_has_files` to confirm the existence of files
    ///   with specific extensions in directories.
    ///
    /// # Errors
    /// - If any prerequisite is not met, an error message is returned containing detailed information about
    ///   what is missing or misconfigured, along with guidance on how to fix it.
    ///
    /// # Example
    /// ```
    /// let stage = BuildStage::GeneratePrecombines;
    /// match check_stage_prerequisites(stage) {
    ///     Ok(_) => println!("Prerequisites met, proceeding with stage."),
    ///     Err(err) => println!("Failed prerequisites: {}", err),
    /// }
    /// ```
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
    /// Attempts to locate the FO4Edit executable on the user's system.
    ///
    /// The function searches for FO4Edit or its variations (`FO4Edit64.exe`, `xEdit64.exe`, etc.)
    /// in the following order:
    ///
    /// 1. **Current Directory**: Checks if any of the candidate executable files (`FO4Edit64.exe`,
    ///    `xEdit64.exe`, `FO4Edit.exe`, `xEdit.exe`) exist in the current working directory.
    ///
    /// 2. **Windows Registry**: Searches for the path of the executable in the Windows Registry
    ///    under the `HKEY_CLASSES_ROOT\FO4Script\DefaultIcon` key.
    ///
    /// If the function successfully finds the executable in any of these steps, it returns the
    /// path to the executable as a `PathBuf`. If no valid executable is found, it returns an
    /// error message wrapped in a `Result::Err`.
    ///
    /// # Returns
    ///
    /// - `Ok(PathBuf)` containing the path to the FO4Edit executable if found.
    /// - `Err(String)` containing an error message if no executable is found.
    ///
    /// # Errors
    ///
    /// - Returns an error if the current working directory cannot be determined.
    /// - If no candidate executables are found in both the current directory and the system registry,
    ///   an error message is returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// match find_fo4edit() {
    ///     Ok(path) => println!("FO4Edit found at: {}", path.display()),
    ///     Err(err) => println!("Error: {}", err),
    /// }
    /// ```
    ///
    /// # Platform
    ///
    /// This function is designed for Windows systems, as it relies on querying the Windows
    /// Registry to locate the FO4Edit executable.
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

    /// Attempts to locate the installation path of Fallout 4 by querying the Windows registry.
    ///
    /// This function checks the `HKEY_LOCAL_MACHINE\SOFTWARE\Wow6432Node\Bethesda Softworks\Fallout4` 
    /// registry key for the "installed path" value, which should point to the installation directory
    /// of Fallout 4. If the path is successfully found, it returns the corresponding `PathBuf`. 
    /// Otherwise, it returns an error message indicating that the installation could not be located.
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - If the installation path is found in the registry.
    /// * `Err(String)` - If the path is not found or an error occurs while accessing the registry.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The registry key does not exist.
    /// - The "installed path" value does not exist in the key.
    /// - Any other error occurs while accessing or reading from the registry.
    ///
    /// # Notes
    /// If the function fails to locate the Fallout 4 installation path, the user may need to 
    /// specify the path manually using a parameter such as `--fallout4_path`.
    ///
    /// # Example
    /// ```rust
    /// match find_fallout4() {
    ///     Ok(path) => println!("Fallout 4 installation found at: {:?}", path),
    ///     Err(err) => eprintln!("Error: {}", err),
    /// }
    /// ```
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

    /// Validates the environment for required files, directories, and settings necessary for 
    /// the application to function properly. The function performs various checks on the file paths, 
    /// versions, and configurations, including:
    ///
    /// 1. **FO4Edit**: Checks if FO4Edit exists in the expected location.
    /// 2. **Fallout4.exe**: Verifies Fallout4 executable is located in the specified directory.
    /// 3. **Creation Kit**: Ensures Creation Kit (CreationKit.exe) is properly installed.
    /// 4. **CKPE (winhttp.dll)**: Verifies the presence of CKPE for extended patching.
    /// 5. **Archive2.exe**: Checks that the Archive2 tool exists, verifying proper Creation Kit setup.
    /// 6. **CKPE INI File**: Ensures the CKPE configuration file exists with correct settings.
    /// 7. **Required xEdit Scripts**: Validates the presence of essential xEdit scripts and ensures
    ///    their versions meet the required minimum.
    /// 8. **CK Logging Configuration**: Confirms that the Creation Kit is properly configured 
    ///    for log redirection to a specified log file.
    /// 9. **Increased Reference Limit Setting**: Checks if the handle limit setting is enabled in 
    ///    the CKPE INI file, warning the user if it's missing to prevent potential failures.
    /// 10. **BSArch**: If BSArch is enabled, validates the presence of BSArch at the specified path.
    ///
    /// ### Parameters
    /// None.
    ///
    /// ### Returns
    /// - `Ok(())` if the environment is valid and all checks pass.
    /// - `Err(String)` if any required file, directory, configuration, or version is missing or invalid, 
    ///   with a detailed error message describing the issue.
    ///
    /// ### Error Details
    /// - **Missing Files**: Returns an error if any expected file or directory is not found.
    /// - **Invalid Versions**: Returns an error if the detected script version is older 
    ///   than the required minimum.
    /// - **Configuration Issues**: Returns an error if essential INI file settings are missing 
    ///   or misconfigured.
    /// - **BSArch Missing**: Returns an error if BSArch is enabled but its path is invalid or absent.
    ///
    /// ### Notes
    /// - If the specified CKPE INI file does not exist, but a fallback configuration (`fallout4_test.ini`) 
    ///   exists, the function automatically switches to the fallback.
    /// - Warnings may be issued in non-blocking cases, such as disabled handle limits, for 
    ///   better user guidance.
    ///
    /// ### Dependencies
    /// - The function relies on filesystem operations and regex-based parsing to validate scripts
    ///   and INI file contents.
    /// - External libraries such as `regex` and file-processing utilities (`File`, `BufReader`) 
    ///   are used.
    ///
    /// ### Example Usage
    /// ```rust
    /// let mut environment = Environment::new();
    /// match environment.verify_environment() {
    ///     Ok(_) => println!("Environment verified successfully."),
    ///     Err(e) => eprintln!("Environment verification failed: {}", e),
    /// }
    /// ```
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

        // Check for BSArch if enabled
        if self.args.use_bsarch {
            if let Some(bsarch_path) = &self.paths.bsarch {
                if !bsarch_path.exists() {
                    return Err(format!("BSArch enabled but not found at specified path: {}", bsarch_path.display()));
                }
            } else {
                return Err("BSArch enabled but no path specified. Use --bsarch-path to specify the location.".to_string());
            }
        }


        Ok(())
    }

    /// Checks the specified plugin and performs the necessary actions to ensure its existence in the correct location.
    ///
    /// # Details
    /// - Verifies if the plugin file exists in the expected directory (`Data` folder of the Fallout 4 paths).
    /// - Ensures there is no pre-existing archive associated with the plugin. If an archive exists, returns an error.
    /// - If the plugin does not exist:
    ///   - Attempts to use a fallback file named `xPrevisPatch.esp` as a seed for the plugin.
    ///   - If the fallback file `xPrevisPatch.esp` also does not exist, returns an error.
    ///   - If the `--no-prompt` argument is set, automatically fails with an error if the plugin is missing.
    ///   - Otherwise, prompts the user for confirmation to rename `xPrevisPatch.esp` to the expected plugin name. 
    ///     If the user agrees, attempts to copy the fallback file to create the plugin.
    ///   - Waits briefly to allow MO2 (Mod Organizer 2) to process the new file.
    ///   - Verifies if the plugin file was successfully created, otherwise returns an error.
    ///
    /// # Return
    /// Returns `Ok(())` if the plugin is successfully validated or created. Otherwise, returns a `String` error indicating
    /// the reason for failure.
    ///
    /// # Errors
    /// The function can return a variety of errors, including:
    /// - If the plugin already has an associated archive file.
    /// - If neither the plugin file nor the fallback `xPrevisPatch.esp` file exists.
    /// - If user chooses not to rename `xPrevisPatch.esp` during the prompt.
    /// - If there are file system issues (e.g., copying the seed file fails).
    /// - If MO2 fails to properly process the newly created plugin file.
    ///
    /// # Examples
    /// ```rust
    /// let mut obj = StructName {
    ///     plugin_name_ext: "example.esp".to_string(),
    ///     plugin_archive: "example.ba2".to_string(),
    ///     paths: Paths {
    ///         fallout4: PathBuf::from("C:/Games/Fallout4"),
    ///     },
    ///     args: Arguments { no_prompt: false },
    /// };
    ///
    /// let result = obj.check_plugin();
    /// match result {
    ///     Ok(_) => println!("Plugin check completed successfully."),
    ///     Err(err) => eprintln!("Error occurred: {}", err),
    /// }
    /// ```
    ///
    /// # Notes
    /// - This function interacts with the file system to check and manipulate plugin files, so adequate permissions are
    ///   required to avoid errors.
    /// - Make sure that the Fallout 4 game path and related file structures are correctly configured in `self.paths` beforehand.
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

    /// Executes a specified action using the Creation Kit (CK) with given parameters and handles various related tasks.
    ///
    /// This function performs the following operations:
    /// 1. Disables certain DLL files (e.g., ENB/ReShade-related) to avoid potential conflicts.
    /// 2. Deletes an existing CK log file if it exists, ensuring a clean log output for this operation.
    /// 3. Logs the current action into the program's own log file.
    /// 4. Constructs and executes the Creation Kit command with the specified options and arguments.
    /// 5. Waits briefly to allow file operations by MO2 to complete post CK execution.
    /// 6. Appends the CK log contents to the program's own log file, if a valid CK log exists.
    /// 7. Verifies if the expected output file was created by the Creation Kit, returning an error if the file is missing.
    /// 8. Handles and logs the Creation Kit's exit status to identify any potential issues.
    /// 9. Re-enables the previously disabled DLLs after the operation is complete.
    ///
    /// # Arguments
    /// - `action`: The CK action to execute (e.g., `Export` or `Compile`).
    /// - `output_file`: The expected name of the output file to verify after CK execution.
    /// - `args`: Additional command-line arguments to pass to the Creation Kit.
    ///
    /// # Returns
    /// - `Ok(())`: If the operation completes successfully.
    /// - `Err(String)`: If an error occurs at any stage during the execution.
    ///
    /// # Errors
    /// Errors may occur during the following steps:
    /// - If disabling or re-enabling DLLs fails due to file system operations.
    /// - If the specified CK log file cannot be deleted, read, or appended to the program's log.
    /// - If executing the Creation Kit fails.
    /// - If the expected output file is not created successfully.
    ///
    /// # Dependencies
    /// This function relies on the following components/paths being properly set:
    /// - `self.paths.creation_kit`: The path to the Creation Kit executable.
    /// - `self.paths.fallout4`: The Fallout 4 directory where relevant files are located.
    /// - `self.plugin_name_ext`: The plugin name or extension used for the CK operation.
    /// - `self.ckpe_settings.log_file`: Path to the CK-specific log file to monitor CK output.
    /// - `self.logfile`: Path to the program's log file for recording the operation.
    ///
    /// # Example
    /// ```rust
    /// let result = my_instance.run_creation_kit("Export", "MyFile.esp", "-SomeCommandLineArgs");
    /// if let Err(e) = result {
    ///     eprintln!("An error occurred: {}", e);
    /// }
    /// ```
    ///
    /// # Notes
    /// - The function logs warnings instead of failing if the Creation Kit exits with a non-zero status but generates the expected output.
    /// - A short delay (`sleep`) is introduced after CK execution to ensure post-processing by external tools like MO2 completes.
    /// - Proper error handling ensures all file system operations are safely executed, with informative error messages in case of failures.
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

    /// Executes Archive2.exe to create a BA2 archive with the given folders.
    ///
    /// # Arguments
    /// * `folders` - A comma-separated list of folders to include in the archive
    /// * `qualifiers` - Additional qualifiers to pass to archive2 (e.g., compression type)
    ///
    /// # Returns
    /// * `Ok(())` if the archive is created successfully
    /// * `Err(String)` if the command fails
    fn run_archive(&self, folders: &str, qualifiers: &str) -> Result<(), String> {
        let data_dir = self.paths.fallout4.join("Data");
        let archive_path = data_dir.join(&self.plugin_archive);

        info!("Creating archive: {} with folders: {}", self.plugin_archive, folders);

        if self.args.use_bsarch {
            // BSArch command format
            let format = if self.args.mode == BuildMode::Xbox { "Xbox" } else { "General" };

            let data_dir_str = data_dir.to_string_lossy();
            let archive_path_str = archive_path.to_string_lossy();

            let bsarch_args = vec![
                "pack",
                &data_dir_str,
                &archive_path_str,
                format,
                "--include",
                folders
            ];

            // Add any additional BSArch-specific arguments here

            self.run_bsarch("packing", &bsarch_args.iter().map(|s| *s).collect::<Vec<&str>>())
        } else {
            // Original Archive2 implementation
            let archive2_exe = &self.paths.archive2;
            let mut command = Command::new(archive2_exe);
            command.current_dir(&data_dir)
                .arg(folders)
                .arg("-c=".to_owned() + &self.plugin_archive)
                .arg(qualifiers)
                .arg("-f=General")
                .arg("-q");

            // Execute and check result
            match command.output() {
                Ok(output) => {
                    if output.status.success() {
                        if !archive_path.exists() {
                            return Err(format!("Archive was not created: {}", self.plugin_archive));
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
    }


    /// Extracts the plugin's archive to the Data directory.
    ///
    /// # Returns
    /// * `Ok(())` if the extraction is successful
    /// * `Err(String)` if the command fails
    fn extract_archive(&self) -> Result<(), String> {
        let data_dir = self.paths.fallout4.join("Data");
        let archive_path = data_dir.join(&self.plugin_archive);

        if !archive_path.exists() {
            return Err(format!("Archive does not exist: {}", self.plugin_archive));
        }

        info!("Extracting archive: {}", self.plugin_archive);

        if self.args.use_bsarch {
            // BSArch extraction command
            let archive_path_str = archive_path.to_string_lossy();
            let data_dir_str = data_dir.to_string_lossy();

            let bsarch_args = vec![
                "extract",
                &archive_path_str,
                &data_dir_str
            ];

            self.run_bsarch("extracting", &bsarch_args.iter().map(|s| *s).collect::<Vec<&str>>())
        } else {
            // Original Archive2 implementation
            let archive2_exe = &self.paths.archive2;
            let mut command = Command::new(archive2_exe);
            command.current_dir(&data_dir)
                .arg(&self.plugin_archive)
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
    }


    /// Adds files from the specified folder to the plugin's archive.
    /// Since Archive2 doesn't support adding to existing archives, this extracts
    /// the archive first, then creates a new one with the combined content.
    ///
    /// # Arguments
    /// * `folder` - The folder to add to the archive (e.g., "vis")
    ///
    /// # Returns
    /// * `Ok(())` if successful
    /// * `Err(String)` if any command fails
    fn add_to_archive(&self, folder: &str) -> Result<(), String> {
        let data_dir = self.paths.fallout4.join("Data");
        let archive_path = data_dir.join(&self.plugin_archive);
        let precombined_dir = data_dir.join("meshes").join("precombined");

        if !archive_path.exists() {
            return self.run_archive(folder, self.get_archive_qualifiers());
        }

        // Extract existing archive
        self.extract_archive()?;

        // Small delay to ensure files are extracted
        sleep(Duration::from_secs(5));

        // Remove the existing archive
        if let Err(e) = fs::remove_file(&archive_path) {
            return Err(format!("Failed to remove existing archive: {}", e));
        }

        // Check if we have precombined meshes extracted
        let has_precombined = self.directory_has_files(&precombined_dir, ".nif");

        if has_precombined {
            // Archive both directories
            if self.args.use_bsarch {
                // BSArch can handle multiple includes in a single operation
                let folders = format!("meshes\\precombined,{}", folder);
                let format = if self.args.mode == BuildMode::Xbox { "Xbox" } else { "General" };

                let data_dir_str = data_dir.to_string_lossy();
                let archive_path_str = archive_path.to_string_lossy();

                let bsarch_args = vec![
                    "pack",
                    &data_dir_str,
                    &archive_path_str,
                    format,
                    "--include",
                    &folders
                ];

                self.run_bsarch("packing combined folders", &bsarch_args.iter().map(|s| *s).collect::<Vec<&str>>())?;
            } else {
                // Original Archive2 implementation
                self.run_archive(
                    &format!("meshes\\precombined,{}", folder),
                    self.get_archive_qualifiers(),
                )?;
            }

            // Clean up precombined directory
            fs::remove_dir_all(precombined_dir)
                .map_err(|e| format!("Error removing precombined directory: {}", e))?;
        } else {
            // Archive the new folder
            self.run_archive(folder, self.get_archive_qualifiers())?;
        }

        Ok(())
    }


    /// Retrieves the appropriate archive qualifiers based on the build mode.
    ///
    /// # Returns
    ///
    /// A `&'static str` representing the additional qualifiers for archiving:
    /// - If the build mode is `BuildMode::Xbox`, returns `"-compression=XBox"`.
    /// - For all other build modes, returns an empty string (`""`).
    ///
    /// # Example
    ///
    /// ```rust
    /// let qualifiers = instance.get_archive_qualifiers();
    /// println!("{}", qualifiers);
    /// ```
    ///
    /// # Notes
    /// - The function relies on the `self.args.mode` field to determine the build mode.
    /// - The returned string is static, i.e., it has a `'static` lifetime.
    fn get_archive_qualifiers(&self) -> &'static str {
        match self.args.mode {
            BuildMode::Xbox => "-compression=XBox",
            _ => "",
        }
    }

    /// Runs an xEdit script against two specified plugin files and logs the results.
    ///
    /// This method automates the process of executing an xEdit script with given parameters,
    /// captures logs, and ensures the expected results by checking for completion messages.
    ///
    /// # Parameters
    ///
    /// - `script`: The name of the xEdit script to be executed. It is assumed to reside
    ///   in the `Edit Scripts` directory under the xEdit path.
    /// - `plugin1`: The name of the primary plugin file for the script to process.
    /// - `plugin2`: The name of the secondary plugin file, if applicable, for the script.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the script executed successfully and produced the expected results.
    /// Returns `Err(String)` with an error message if any step of the execution fails:
    /// - Issues with file I/O operations (e.g., log files and plugin list file).
    /// - Errors starting or interacting with the xEdit process.
    /// - Failure to detect or parse the xEdit log file.
    ///
    /// # Behavior
    ///
    /// 1. Logs the start of the script execution to the main log file.
    /// 2. Creates a temporary `Plugins.txt` file listing the plugin files to be processed by xEdit.
    /// 3. Removes any existing unattended log file from previous runs.
    /// 4. Starts the xEdit process with the provided parameters (including script, plugins, etc.).
    /// 5. Waits for xEdit to generate a log file and exits.
    /// 6. Appends the xEdit log content to the main log file.
    /// 7. Verifies the xEdit log file for a successful completion message.
    ///
    /// # Error Handling
    ///
    /// - File operations (e.g., opening, creating, writing, or reading files) are error-checked.
    /// - If the xEdit process fails to start, terminates prematurely, or does not produce a log file,
    ///   an error message is returned.
    /// - An additional check is performed on the xEdit log file to ensure the script executed successfully;
    ///   failure results in an error.
    ///
    /// # Assumptions
    ///
    /// - The `self.logfile` is a valid writable file path for logging script execution details.
    /// - The `self.unattended_logfile` path is used to store xEdit-generated logs.
    /// - The `self.paths.fo4edit` points to the executable path of xEdit.
    ///
    /// # Limitations
    ///
    /// - The method simulates keypresses or waits for the xEdit process without directly interacting 
    ///   with its GUI (this requires external libraries like `winapi` for precise automation).
    /// - Relies on wait durations (`sleep`) to ensure the process completes, which may not be optimal 
    ///   for all environments.
    ///
    /// # Example Usage
    ///
    /// ```rust
    /// let result = instance.run_xedit_script("MyScript.pas", "Plugin1.esp", "Plugin2.esp");
    /// match result {
    ///     Ok(_) => println!("Script ran successfully."),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - Ensure xEdit and any required files are properly set up and accessible in the specified paths.
    /// - Proper error propagation ensures this function can be used reliably for automated workflows.
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

    /// Prompts the user with a yes/no question and returns their response.
    ///
    /// If the `no_prompt` flag in `self.args` is set, the function will 
    /// automatically return `Ok(true)` without prompting the user. 
    /// Otherwise, it displays the provided message, waits for user input, and 
    /// interprets it as a "yes" (if it starts with 'y' or 'Y') or "no" otherwise. 
    ///
    /// # Arguments
    ///
    /// * `message` - The question or message to display to the user.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if the user input starts with 'y' or if `no_prompt` is enabled.
    /// * `Ok(false)` if the user input doesn't start with 'y'.
    /// * `Err(String)` if there was an error reading input from standard input.
    ///
    /// # Example
    ///
    /// ```rust
    /// let result = prompt_yes_no("Do you want to continue?");
    /// match result {
    ///     Ok(true) => println!("User selected 'Yes'."),
    ///     Ok(false) => println!("User selected 'No'."),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error string if there is an issue reading input from standard input.
    ///
    /// # Note
    ///
    /// The user input is trimmed and converted to lowercase before evaluation, 
    /// and only the starting character is checked to determine the response.
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
            self.prompt_for_plugin_name()?;
            let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
            if plugin_path.exists() {
                // Plugin already exists, prompt for stage
                self.prompt_for_stage(&self.args.mode)?
            } else {
                BuildStage::VerifyEnvironment
            }
        } else {
            // Plugin specified but check if it already exists
            let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
            if plugin_path.exists() {
                // Plugin already exists, prompt for stage
                self.prompt_for_stage(&self.args.mode)?
            } else {
                BuildStage::VerifyEnvironment
            }
        };

        // Get stage as integer for comparisons
        let start_stage_val = start_stage as i32;

        self.verify_environment()?;
        self.check_plugin()?;

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
