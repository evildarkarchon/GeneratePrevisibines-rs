# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

GeneratePrevisibines-rs is a Rust tool for automating Fallout 4 mod development, specifically the generation of precombine and previs data. It's a port of the original batch script by PJM to Rust.

## Memories

- The original batch file is located in `Original Batch File/`.

## Common Development Commands

```bash
# Build debug version
cargo build

# Build release version (recommended for performance)
cargo build --release

# Run tests
cargo test

# Run linter
cargo clippy

# Format code
cargo fmt

# Check code without building
cargo check

# Run with example arguments
cargo run -- --mode clean MyPlugin.esp
cargo run -- --mode filtered MyPlugin.esp --no-prompt
cargo run -- --mode xbox MyPlugin.esp --fo4edit-path "C:\Path\To\FO4Edit.exe"
```

## Architecture Overview

The project has been refactored from a monolithic single-file architecture into a modular structure with the following organization:

### Module Structure
```
src/
├── main.rs          // Entry point, minimal logic
├── lib.rs           // Module declarations
├── cli.rs           // CLI arguments and configuration
├── paths.rs         // Path management and discovery
├── builder.rs       // PrevisbineBuilder core implementation
├── ui.rs            // User interaction and prompts
├── validation.rs    // Environment and file validation
├── utils.rs         // Common utilities
└── tools/           // External tool integrations
    ├── mod.rs
    ├── creation_kit.rs
    ├── archive.rs
    └── xedit.rs
```

### Core Structures
- **PrevisbineBuilder**: Main orchestrator that manages the entire build process
- **BuildMode**: Enum defining three modes - Clean (full process), Filtered (skips PSG/CDX), Xbox (optimized)
- **BuildStage**: Enum representing the 8-stage build pipeline
- **Paths**: Holds paths to external tools (Creation Kit, FO4Edit, Archive2/BSArch)

### Build Pipeline Stages
1. **GeneratePrecombines**: Uses Creation Kit to generate precombined meshes
2. **MergePrecombines**: Merges PrecombineObjects.esp using FO4Edit
3. **ArchivePrecombines**: Creates BA2 archives from precombines
4. **CompressPsg**: Compresses PSG files (Clean mode only)
5. **BuildCdx**: Builds CDX files (Clean mode only)
6. **GeneratePrevis**: Generates visibility data via Creation Kit
7. **MergePrevis**: Merges Previs.esp using FO4Edit
8. **ArchiveVis**: Adds visibility files to BA2 archive

### External Tool Integration
The tool integrates with:
- **Creation Kit Platform Extended (CKPE)**: For precombine/previs generation
- **FO4Edit/xEdit**: For merging ESP files
- **Archive2/BSArch**: For creating BA2 archives
- **Mod Organizer 2**: Waits for MO2 file processing when detected

### Platform Considerations
- Windows-specific due to registry access for finding game installations
- Uses Windows registry keys to locate Fallout 4 and tools
- Handles file paths with spaces using proper quoting

## Key Implementation Details

- **Modular Architecture**: Well-organized separation of concerns across multiple modules
- **Error Handling**: Comprehensive error messages with context
- **Logging**: Uses env_logger with configurable levels
- **Resume Support**: Can restart from any stage using `--start-stage`
- **File Management**: Temporary files cleaned up unless `--keep-files` is used
- **CKPE Settings**: Automatically configures Creation Kit Platform Extended
- **Process Management**: Proper handling of external tool execution with timeouts
- **Archive Format Support**: Both Archive2 and BSArch are fully supported
- **Cross-platform Compatibility**: Conditional compilation for Windows-specific features

## Implemented Build Stages

All 8 build stages are now fully implemented:

1. **Generate Precombines**: Creates precombined meshes via Creation Kit with handle array error detection
2. **Merge Precombines**: Merges precombine data using FO4Edit scripts with error checking
3. **Archive Precombines**: Creates BA2 archives from precombined meshes (supports both Archive2 and BSArch)
4. **Compress PSG**: Compresses PSG files to CSG format (Clean mode only)
5. **Build CDX**: Creates CDX files for optimized cell loading (Clean mode only)
6. **Generate Previs**: Generates precomputed visibility data via Creation Kit with task completion verification
7. **Merge Previs**: Merges previs data using FO4Edit scripts with completion status checking
8. **Archive Vis**: Adds visibility files to the final BA2 archive with intelligent extraction/re-archiving
```