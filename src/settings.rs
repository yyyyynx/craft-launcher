use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub check_on_startup: bool,
    pub check_launcher_on_startup: bool,
    pub close_to_tray: bool,
    pub beta_updates: bool,
}
impl Default for Settings {
    fn default() -> Self { Self { check_on_startup: true, check_launcher_on_startup: true, close_to_tray: true, beta_updates: true } }
}
impl Settings {
    pub fn load(root: &Path) -> Self {
        fs::read(root.join("settings.json")).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        fs::write(root.join("settings.json"), serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| format!("Couldn't save Settings: {e}"))
    }
}
