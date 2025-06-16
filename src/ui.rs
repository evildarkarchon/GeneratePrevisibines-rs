use std::io::{self, Write};
use crate::cli::{BuildMode, BuildStage};

/// Prompts the user to input a plugin name if none is specified.
///
/// # Returns
/// A tuple containing:
/// - The plugin name without extension
/// - The plugin name with extension
/// - The plugin archive name
///
/// # Errors
/// Returns an error if:
/// - There's an error reading input
/// - No plugin name is entered
pub fn prompt_for_plugin_name() -> Result<(String, String, String), String> {
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
    let (plugin_name_no_ext, plugin_name_ext) = if plugin_name.to_lowercase().ends_with(".esp")
        || plugin_name.to_lowercase().ends_with(".esm")
        || plugin_name.to_lowercase().ends_with(".esl")
    {
        let name_without_ext = plugin_name
            .rfind('.')
            .map(|i| &plugin_name[0..i])
            .unwrap_or(&plugin_name)
            .to_string();
        (name_without_ext, plugin_name)
    } else {
        (plugin_name.clone(), format!("{}.esp", plugin_name))
    };

    let plugin_archive = format!("{} - Main.ba2", &plugin_name_no_ext);

    Ok((plugin_name_no_ext, plugin_name_ext, plugin_archive))
}

/// Prompts the user to choose a build stage to start from.
///
/// # Parameters
/// - `build_mode`: The current build mode, which determines which stages are available
///
/// # Returns
/// The selected build stage
///
/// # Errors
/// Returns an error if:
/// - There's an error reading input
/// - The user enters an invalid stage number
pub fn prompt_for_stage(build_mode: &BuildMode) -> Result<BuildStage, String> {
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
            println!("[{}] {}", stage, build_stage.description());
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

/// Prompts the user with a yes/no question and returns their response.
///
/// # Arguments
/// * `message` - The question or message to display to the user
/// * `no_prompt` - If true, automatically returns Ok(true) without prompting
///
/// # Returns
/// * `Ok(true)` if the user input starts with 'y' or if `no_prompt` is true
/// * `Ok(false)` if the user input doesn't start with 'y'
/// * `Err(String)` if there was an error reading input
pub fn prompt_yes_no(message: &str, no_prompt: bool) -> Result<bool, String> {
    if no_prompt {
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

/// Displays the available stages for the given build mode
pub fn display_stages(build_mode: &BuildMode) {
    println!("Available stages to resume from:");
    print!("{}", BuildStage::display_stages(build_mode));
    println!("Enter stage number (0-8) to resume from that stage, or any other key to exit.");
}