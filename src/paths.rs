use std::env;
use std::path::PathBuf;
use log::warn;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

pub struct Paths {
    pub fo4edit: PathBuf,
    pub fallout4: PathBuf,
    pub creation_kit: PathBuf,
    pub archive2: PathBuf,
    pub bsarch: Option<PathBuf>,
}

impl Paths {
    pub fn new(
        fo4edit_path: Option<String>,
        fallout4_path: Option<String>,
        use_bsarch: bool,
        bsarch_path: Option<String>,
    ) -> Result<Self, String> {
        // Find path to FO4Edit
        let fo4edit = if let Some(path) = fo4edit_path {
            PathBuf::from(path)
        } else {
            find_fo4edit()?
        };

        // Find path to Fallout 4
        let fallout4 = if let Some(path) = fallout4_path {
            PathBuf::from(path)
        } else {
            find_fallout4()?
        };

        // Prepare other paths
        let creation_kit = fallout4.join("CreationKit.exe");
        let archive2 = fallout4
            .join("tools")
            .join("archive2")
            .join("archive2.exe");

        // Handle BSArch path
        let bsarch = if use_bsarch {
            if let Some(path) = bsarch_path {
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

        Ok(Paths {
            fo4edit,
            fallout4,
            creation_kit,
            archive2,
            bsarch,
        })
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

    // Try registry on Windows
    #[cfg(windows)]
    {
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
    // Try registry on Windows
    #[cfg(windows)]
    {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(subkey) = hklm.open_subkey("SOFTWARE\\Wow6432Node\\Bethesda Softworks\\Fallout4")
        {
            if let Ok(path) = subkey.get_value::<String, _>("installed path") {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(
        "Fallout 4 installation not found. Please specify path with --fallout4_path"
            .to_string(),
    )
}