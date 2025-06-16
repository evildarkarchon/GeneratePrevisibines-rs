use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use log::{info, warn};

use crate::cli::{Args, BuildMode, BuildStage};
use crate::paths::Paths;
use crate::tools::creation_kit::{CkpeSettings, run_creation_kit};
use crate::tools::archive::{run_archive, extract_archive, run_bsarch, get_archive_qualifiers};
use crate::tools::xedit::run_xedit_script;
use crate::ui::{prompt_for_plugin_name, prompt_for_stage, prompt_yes_no};
use crate::validation::{verify_environment, check_plugin, directory_has_files, check_stage_prerequisites};
use crate::utils::{remove_file_if_exists, remove_dir_all_if_exists};

pub struct PrevisbineBuilder {
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
    /// Creates a new `PrevisbineBuilder` instance
    pub fn new(args: Args) -> Result<Self, String> {
        // Initialize paths
        let paths = Paths::new(
            args.fo4edit_path.clone(),
            args.fallout4_path.clone(),
            args.use_bsarch,
            args.bsarch_path.clone(),
        )?;

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

        let plugin_archive = format!("{} - Main.ba2", plugin_name);

        Ok(Self {
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

    /// Main entry point to run the builder
    pub fn run(&mut self) -> Result<(), String> {
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
        let start_stage = self.determine_starting_stage()?;

        // Get stage as integer for comparisons
        let start_stage_val = start_stage as i32;

        // Verify environment
        verify_environment(&self.paths, &mut self.ckpe_settings, &self.plugin_name, self.args.use_bsarch)?;
        
        // Check plugin
        check_plugin(
            &self.paths,
            &self.plugin_name_ext,
            &self.plugin_archive,
            self.args.no_prompt,
            |msg| prompt_yes_no(msg, self.args.no_prompt),
        )?;

        // Execute stages
        if start_stage_val <= BuildStage::GeneratePrecombines as i32 {
            self.stage_generate_precombines()?;
        }

        if start_stage_val <= BuildStage::MergePrecombines as i32 {
            self.stage_merge_precombines()?;
        }

        if start_stage_val <= BuildStage::ArchivePrecombines as i32 {
            self.stage_archive_precombines()?;
        }

        if self.args.mode == BuildMode::Clean {
            if start_stage_val <= BuildStage::CompressPsg as i32 {
                self.stage_compress_psg()?;
            }

            if start_stage_val <= BuildStage::BuildCdx as i32 {
                self.stage_build_cdx()?;
            }
        }

        if start_stage_val <= BuildStage::GeneratePrevis as i32 {
            self.stage_generate_previs()?;
        }

        if start_stage_val <= BuildStage::MergePrevis as i32 {
            self.stage_merge_previs()?;
        }

        if start_stage_val <= BuildStage::ArchiveVis as i32 {
            self.stage_archive_vis()?;
        }

        // Cleanup
        self.cleanup()?;

        println!("\nBuild complete!");
        Ok(())
    }

    fn determine_starting_stage(&mut self) -> Result<BuildStage, String> {
        if let Some(stage) = self.args.start_stage {
            match BuildStage::from_i32(stage) {
                Some(stage) => {
                    // Check prerequisites for this stage
                    check_stage_prerequisites(
                        stage,
                        &self.paths,
                        &self.plugin_name_ext,
                        &self.plugin_name,
                        &self.args.mode,
                        directory_has_files,
                    )?;
                    Ok(stage)
                }
                None => {
                    return Err(format!("ERROR - Invalid stage number: {}", stage));
                }
            }
        } else if self.plugin_name.is_empty() {
            // No plugin specified on command line
            let (plugin_name, plugin_name_ext, plugin_archive) = prompt_for_plugin_name()?;
            self.plugin_name = plugin_name;
            self.plugin_name_ext = plugin_name_ext;
            self.plugin_archive = plugin_archive;
            
            let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
            if plugin_path.exists() {
                // Plugin already exists, prompt for stage
                prompt_for_stage(&self.args.mode)
            } else {
                Ok(BuildStage::VerifyEnvironment)
            }
        } else {
            // Plugin specified but check if it already exists
            let plugin_path = self.paths.fallout4.join("Data").join(&self.plugin_name_ext);
            if plugin_path.exists() {
                // Plugin already exists, prompt for stage
                prompt_for_stage(&self.args.mode)
            } else {
                Ok(BuildStage::VerifyEnvironment)
            }
        }
    }

    // Stage implementations would go here...
    // For brevity, I'll just include stubs for now

    fn stage_generate_precombines(&self) -> Result<(), String> {
        info!("Stage: Generate Precombines");
        
        let precombined_dir = self.paths.fallout4.join("Data").join("meshes").join("precombined");
        let has_precombined = directory_has_files(&precombined_dir, ".nif");

        if has_precombined {
            return Err(
                "ERROR - Precombine directory (Data\\meshes\\precombined) not empty"
                    .to_string(),
            );
        }

        let vis_dir = self.paths.fallout4.join("Data").join("vis");
        let has_vis = directory_has_files(&vis_dir, ".uvd");

        if has_vis {
            return Err("ERROR - Previs directory (Data\\vis) not empty".to_string());
        }

        // Delete working files if they exist
        let data_dir = self.paths.fallout4.join("Data");
        let combined_objects_esp = data_dir.join("CombinedObjects.esp");
        if combined_objects_esp.exists() {
            fs::remove_file(&combined_objects_esp)
                .map_err(|e| format!("Error removing CombinedObjects.esp: {}", e))?;
        }

        let geometry_psg_path = data_dir.join(format!("{} - Geometry.psg", self.plugin_name));
        if geometry_psg_path.exists() {
            fs::remove_file(&geometry_psg_path)
                .map_err(|e| format!("Error removing Geometry.psg: {}", e))?;
        }

        // Generate precombined
        let (action, args) = if self.args.mode == BuildMode::Clean {
            ("GeneratePrecombined", "clean all")
        } else {
            ("GeneratePrecombined", "filtered all")
        };
        
        run_creation_kit(
            &self.paths.creation_kit,
            &self.paths.fallout4,
            &self.plugin_name_ext,
            action,
            "CombinedObjects.esp",
            args,
            &self.ckpe_settings,
            &self.logfile,
        )?;

        // Check if any precombines were created
        let new_has_precombined = directory_has_files(&precombined_dir, ".nif");
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

        // Check PSG was created in clean mode
        if self.args.mode == BuildMode::Clean {
            if !geometry_psg_path.exists() {
                return Err("ERROR - GeneratePrecombined failed to create psg file".to_string());
            }
        }

        Ok(())
    }

    fn stage_merge_precombines(&self) -> Result<(), String> {
        info!("Stage: Merge Precombines");
        
        run_xedit_script(
            &self.paths.fo4edit,
            "Batch_FO4MergeCombinedObjectsAndCheck.pas",
            &self.plugin_name_ext,
            "CombinedObjects.esp",
            &self.logfile,
            &self.unattended_logfile,
        )?;

        // Check for errors in log
        if self.unattended_logfile.exists() {
            let log_content = fs::read_to_string(&self.unattended_logfile)
                .map_err(|e| format!("Error reading unattended log file: {}", e))?;

            if log_content.contains("Error: ") {
                warn!("WARNING - Merge Precombines had errors");
            }
        }

        Ok(())
    }

    fn stage_archive_precombines(&self) -> Result<(), String> {
        info!("Stage: Archive Precombines");
        
        let data_dir = self.paths.fallout4.join("Data");
        let qualifiers = get_archive_qualifiers(&self.args.mode);
        
        if self.args.use_bsarch {
            // BSArch implementation
            let format = if self.args.mode == BuildMode::Xbox { "Xbox" } else { "General" };
            let data_dir_str = data_dir.to_string_lossy();
            let archive_path = data_dir.join(&self.plugin_archive);
            let archive_path_str = archive_path.to_string_lossy();

            let bsarch_args = vec![
                "pack",
                &data_dir_str,
                &archive_path_str,
                format,
                "--include",
                "meshes\\precombined"
            ];

            if let Some(bsarch_path) = &self.paths.bsarch {
                run_bsarch(bsarch_path, "archiving precombines", &bsarch_args)?;
            } else {
                return Err("BSArch path not configured".to_string());
            }
        } else {
            // Archive2 implementation
            run_archive(
                &self.paths.archive2,
                &data_dir,
                &self.plugin_archive,
                "meshes\\precombined",
                qualifiers,
            )?;
        }

        // Clean up precombined directory (Archive2 only)
        if !self.args.use_bsarch {
            let precombined_dir = data_dir.join("meshes").join("precombined");
            if precombined_dir.exists() {
                fs::remove_dir_all(&precombined_dir)
                    .map_err(|e| format!("Error removing precombined directory: {}", e))?;
            }
        }

        Ok(())
    }

    fn stage_compress_psg(&self) -> Result<(), String> {
        info!("Stage: Compress PSG");
        
        let data_dir = self.paths.fallout4.join("Data");
        let psg_file = format!("{} - Geometry.psg", self.plugin_name);
        let csg_file = format!("{} - Geometry.csg", self.plugin_name);
        
        // Check if PSG file exists
        let psg_path = data_dir.join(&psg_file);
        if !psg_path.exists() {
            return Err("ERROR - No Geometry.psg file found. Run GeneratePrecombines first.".to_string());
        }
        
        run_creation_kit(
            &self.paths.creation_kit,
            &self.paths.fallout4,
            &self.plugin_name_ext,
            "CompressPSG",
            &csg_file,
            "",
            &self.ckpe_settings,
            &self.logfile,
        )?;
        
        // Delete the original PSG file after successful compression
        let csg_path = data_dir.join(&csg_file);
        if csg_path.exists() {
            fs::remove_file(&psg_path)
                .map_err(|e| format!("Error removing PSG file: {}", e))?;
        } else {
            return Err("ERROR - CompressPSG failed to create CSG file".to_string());
        }

        Ok(())
    }

    fn stage_build_cdx(&self) -> Result<(), String> {
        info!("Stage: Build CDX");
        
        let cdx_file = format!("{}.cdx", self.plugin_name);
        
        run_creation_kit(
            &self.paths.creation_kit,
            &self.paths.fallout4,
            &self.plugin_name_ext,
            "BuildCDX",
            &cdx_file,
            "",
            &self.ckpe_settings,
            &self.logfile,
        )?;

        Ok(())
    }

    fn stage_generate_previs(&self) -> Result<(), String> {
        info!("Stage: Generate Previs");
        
        let data_dir = self.paths.fallout4.join("Data");
        let vis_dir = data_dir.join("vis");
        
        // Check if vis directory is empty
        if directory_has_files(&vis_dir, ".uvd") {
            return Err("ERROR - Previs directory (Data\\vis) not empty".to_string());
        }
        
        // Delete Previs.esp if it exists
        let previs_esp = data_dir.join("Previs.esp");
        if previs_esp.exists() {
            fs::remove_file(&previs_esp)
                .map_err(|e| format!("Error removing Previs.esp: {}", e))?;
        }
        
        run_creation_kit(
            &self.paths.creation_kit,
            &self.paths.fallout4,
            &self.plugin_name_ext,
            "GeneratePreVisData",
            "Previs.esp",
            "clean all",
            &self.ckpe_settings,
            &self.logfile,
        )?;
        
        // Check if visibility files were created
        if !directory_has_files(&vis_dir, ".uvd") {
            return Err("ERROR - GeneratePreVisData failed to create visibility files".to_string());
        }
        
        // Check if Previs.esp was created
        if !previs_esp.exists() {
            return Err("ERROR - GeneratePreVisData failed to create Previs.esp".to_string());
        }
        
        // Check for specific error in logs
        if let Some(log_file) = &self.ckpe_settings.log_file {
            if log_file.exists() {
                let log_content = fs::read_to_string(log_file)
                    .map_err(|e| format!("Error reading CK log file: {}", e))?;

                if log_content.contains("ERROR: visibility task did not complete.") {
                    return Err("ERROR - GeneratePreVisData visibility task did not complete".to_string());
                }
            }
        }

        Ok(())
    }

    fn stage_merge_previs(&self) -> Result<(), String> {
        info!("Stage: Merge Previs");
        
        run_xedit_script(
            &self.paths.fo4edit,
            "Batch_FO4MergePreVisAndAutoUpdateRefr.pas",
            &self.plugin_name_ext,
            "Previs.esp",
            &self.logfile,
            &self.unattended_logfile,
        )?;

        // Check for completion in log
        if self.unattended_logfile.exists() {
            let log_content = fs::read_to_string(&self.unattended_logfile)
                .map_err(|e| format!("Error reading unattended log file: {}", e))?;

            if !log_content.contains("Completed: No Errors.") {
                return Err("ERROR - Merge Previs script did not complete successfully".to_string());
            }
        } else {
            return Err("ERROR - Merge Previs script did not produce a log file".to_string());
        }

        Ok(())
    }

    fn stage_archive_vis(&self) -> Result<(), String> {
        info!("Stage: Archive Vis");
        
        let data_dir = self.paths.fallout4.join("Data");
        let archive_path = data_dir.join(&self.plugin_archive);
        let vis_dir = data_dir.join("vis");
        let precombined_dir = data_dir.join("meshes").join("precombined");
        
        if self.args.use_bsarch {
            // BSArch implementation - can pack multiple directories
            let format = if self.args.mode == BuildMode::Xbox { "Xbox" } else { "General" };
            let data_dir_str = data_dir.to_string_lossy();
            let archive_path_str = archive_path.to_string_lossy();
            
            // Check if we have both precombined and vis files
            let has_precombined = directory_has_files(&precombined_dir, ".nif");
            let folders = if has_precombined {
                "meshes\\precombined,vis"
            } else {
                "vis"
            };

            let bsarch_args = vec![
                "pack",
                &data_dir_str,
                &archive_path_str,
                format,
                "--include",
                folders
            ];

            if let Some(bsarch_path) = &self.paths.bsarch {
                run_bsarch(bsarch_path, "archiving vis files", &bsarch_args)?;
            } else {
                return Err("BSArch path not configured".to_string());
            }
        } else {
            // Archive2 implementation - need to extract and re-archive
            if archive_path.exists() {
                // Extract existing archive
                extract_archive(&self.paths.archive2, &data_dir, &self.plugin_archive)?;
                
                // Small delay to ensure files are extracted
                sleep(Duration::from_secs(5));
                
                // Remove the existing archive
                fs::remove_file(&archive_path)
                    .map_err(|e| format!("Failed to remove existing archive: {}", e))?;
            }
            
            let qualifiers = get_archive_qualifiers(&self.args.mode);
            
            // Check if we have precombined meshes extracted
            let has_precombined = directory_has_files(&precombined_dir, ".nif");
            
            if has_precombined {
                // Archive both directories
                run_archive(
                    &self.paths.archive2,
                    &data_dir,
                    &self.plugin_archive,
                    "meshes\\precombined,vis",
                    qualifiers,
                )?;
                
                // Clean up precombined directory
                fs::remove_dir_all(&precombined_dir)
                    .map_err(|e| format!("Error removing precombined directory: {}", e))?;
            } else {
                // Archive just the vis folder
                run_archive(
                    &self.paths.archive2,
                    &data_dir,
                    &self.plugin_archive,
                    "vis",
                    qualifiers,
                )?;
            }
        }
        
        // Clean up vis directory (Archive2 only)
        if !self.args.use_bsarch && vis_dir.exists() {
            fs::remove_dir_all(&vis_dir)
                .map_err(|e| format!("Error removing vis directory: {}", e))?;
        }

        Ok(())
    }

    fn cleanup(&self) -> Result<(), String> {
        info!("Performing cleanup");
        
        if !self.args.keep_files {
            // Clean up temporary files
            let data_dir = self.paths.fallout4.join("Data");
            
            // Remove CombinedObjects.esp
            remove_file_if_exists(data_dir.join("CombinedObjects.esp"))?;
            
            // Remove Previs.esp
            remove_file_if_exists(data_dir.join("Previs.esp"))?;
            
            // Remove vis directory
            remove_dir_all_if_exists(data_dir.join("vis"))?;
        }
        
        Ok(())
    }
}