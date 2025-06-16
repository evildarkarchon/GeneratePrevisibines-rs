use clap::{Parser, ValueEnum};
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum BuildMode {
    Clean,
    Filtered,
    Xbox,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum BuildStage {
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
    pub fn from_i32(value: i32) -> Option<Self> {
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

    pub fn display_stages(build_mode: &BuildMode) -> String {
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

    pub fn description(&self) -> &'static str {
        match self {
            Self::VerifyEnvironment => "Verify Environment",
            Self::GeneratePrecombines => "Generate Precombines",
            Self::MergePrecombines => "Merge Precombines",
            Self::ArchivePrecombines => "Archive Precombines",
            Self::CompressPsg => "Compress PSG",
            Self::BuildCdx => "Build CDX",
            Self::GeneratePrevis => "Generate Previs",
            Self::MergePrevis => "Merge Previs",
            Self::ArchiveVis => "Archive Vis",
        }
    }
}

impl fmt::Display for BuildMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildMode::Clean => write!(f, "clean"),
            BuildMode::Filtered => write!(f, "filtered"),
            BuildMode::Xbox => write!(f, "xbox"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "generate_previsbines",
    about = "Automatic Previsbine Builder for Fallout 4\nBased on the original batch script by PJM",
    version = "2.6"
)]
pub struct Args {
    /// Build mode: clean, filtered, or xbox
    #[arg(value_enum)]
    #[arg(short, long, default_value = "clean")]
    pub mode: BuildMode,

    /// The plugin to generate previsbines for
    #[arg(value_name = "PLUGIN")]
    pub plugin: Option<String>,

    /// Path to FO4Edit/xEdit executable
    #[arg(long)]
    pub fo4edit_path: Option<String>,

    /// Path to Fallout 4 installation directory
    #[arg(long)]
    pub fallout4_path: Option<String>,

    #[arg(long)]
    /// Stage of the process to start from
    pub start_stage: Option<i32>,

    /// Don't prompt for confirmation, just execute
    #[arg(short, long)]
    pub no_prompt: bool,

    /// Specify files to keep after completion
    #[arg(short, long)]
    pub keep_files: bool,

    /// Use BSArch instead of Archive2
    #[arg(short, long)]
    pub use_bsarch: bool,
    
    /// BSArch Path (Requires --use-bsarch)
    #[arg(long)]
    pub bsarch_path: Option<String>,
}