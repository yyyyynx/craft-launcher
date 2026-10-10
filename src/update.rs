use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use crate::transfer::{self, Progress};

use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Image,
    Video,
    Design,
    Documents,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Image => "Image",
            Group::Video => "Video",
            Group::Design => "Design",
            Group::Documents => "Documents",
        }
    }
}

pub struct CraftApp {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub group: Group,
}

pub const APPS: &[CraftApp] = &[
    CraftApp { id: "photocraft", name: "PhotoCraft", description: "Photo editing", group: Group::Image },
    CraftApp { id: "vectorcraft", name: "VectorCraft", description: "Vector illustration", group: Group::Design },
    CraftApp { id: "filmcraft", name: "FilmCraft", description: "Video editing", group: Group::Video },
    CraftApp { id: "lightcraft", name: "LightCraft", description: "RAW photo development", group: Group::Image },
    CraftApp { id: "pdfcraft", name: "PdfCraft", description: "PDF tools", group: Group::Documents },
    CraftApp { id: "effectcraft", name: "EffectCraft", description: "Motion graphics", group: Group::Video },
    CraftApp { id: "designcraft", name: "DesignCraft", description: "Layout and publishing", group: Group::Design },
];

#[derive(Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub at: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone)]
pub struct RemoteInfo {
    pub tag: String,
    pub title: String,
    pub body: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub fn artcraft_root() -> PathBuf {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("AppData").join("Local")))
        .expect("Windows user data directory is unavailable");
    local.join("CraftLauncher")
}

pub fn installed_versions(root: &Path) -> Vec<(String, String)> {
    APPS.iter()
        .map(|app| (app.id.to_string(), read_version(root, app.id)))
        .collect()
}

pub fn read_version(root: &Path, id: &str) -> String {
    if !exe_path(root, id).is_file() {
        return String::new();
    }
    fs::read_to_string(root.join("apps").join(format!("{id}.version")))
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn load_log(root: &Path, id: &str) -> Vec<LogEntry> {
    let Ok(text) = fs::read_to_string(log_path(root, id)) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn load_recent(root: &Path) -> Vec<String> {
    let path = logs_dir(root).join("recent.json");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn remember_recent(root: &Path, id: &str) -> Vec<String> {
    let mut recent = load_recent(root);
    recent.retain(|item| item != id);
    recent.insert(0, id.to_string());
    recent.truncate(7);
    if let Ok(()) = fs::create_dir_all(logs_dir(root)) {
        let _ = fs::write(
            logs_dir(root).join("recent.json"),
            serde_json::to_string_pretty(&recent).unwrap_or_else(|_| "[]".into()),
        );
    }
    recent
}

pub fn load_favorites(root: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("favorites.json")) else { return Vec::new() };
    let stored: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
    APPS.iter().filter(|app| stored.iter().any(|id| id == app.id))
        .map(|app| app.id.to_string()).collect()
}

pub fn save_favorites(root: &Path, favorites: &[String]) -> Result<()> {
    fs::create_dir_all(root).map_err(|error| format!("Couldn't save Favorites: {error}"))?;
    let json = serde_json::to_string_pretty(favorites).map_err(|error| error.to_string())?;
    fs::write(root.join("favorites.json"), json)
        .map_err(|error| format!("Couldn't save Favorites. Check folder permissions and free disk space. Details: {error}"))
}

pub fn ensure_baseline(root: &Path) {
    for app in APPS {
        if log_path(root, app.id).is_file() {
            continue;
        }
        let version = read_version(root, app.id);
        if version.is_empty() {
            let _ = push_log(
                root,
                app.id,
                "Not installed",
                "Update to download the latest build from GitHub",
            );
        } else {
            let _ = push_log(
                root,
                app.id,
                &format!("Installed {version}"),
                "This log starts from the build already on disk. Later entries are what the launcher updates.",
            );
        }
    }
}

pub fn fetch_remote(id: &str) -> Result<RemoteInfo> {
    let release = fetch_release(id)?;
    Ok(RemoteInfo {
        tag: release.tag_name,
        title: release.name.unwrap_or_default(),
        body: trim_chars(release.body.unwrap_or_default().trim(), 2500),
    })
}

pub struct UpdateOutcome {
    pub status: String,
}

pub fn update_app(root: &Path, id: &str, repair: bool, cancel: &AtomicBool, on_status: &mut dyn FnMut(String), progress: &mut dyn FnMut(Progress)) -> Result<UpdateOutcome> {
    let app = APPS.iter().find(|app| app.id == id).ok_or_else(|| format!("unknown app {id}"))?;
    let mut notes = Vec::new();

    transfer::check(cancel)?;
    recover_interrupted_installs(root)?;
    on_status(format!("{}: checking the release...", app.name));
    match update_binary_controlled(root, app, repair, cancel, on_status, progress) {
        Ok(Some(entry)) => {
            notes.push(entry.title.clone());
            let _ = push_log(root, app.id, &entry.title, &entry.body);
        }
        Ok(None) => {}
        Err(error) => {
            if error == transfer::CANCELLED { return Err(error); }
            let _ = push_log(root, app.id, "App update failed", &error);
            return Err(format!("Couldn't install or update {}. Check your connection and free disk space, close the app, then try again. Details: {error}", app.name));
        }
    }

    let status = if notes.is_empty() {
        let version = read_version(root, app.id);
        format!("{name} is already up to date ({version})", name = app.name)
    } else {
        format!("{}: {}", app.name, notes.join(" · "))
    };
    Ok(UpdateOutcome { status })
}

pub fn open_app(root: &Path, id: &str) -> Result<()> {
    let exe = exe_path(root, id);
    if !exe.is_file() {
        return Err("App executable is missing. Click Install to download it again.".into());
    }
    let dir = exe.parent().unwrap_or(root).to_path_buf();
    Command::new(&exe)
        .current_dir(dir)
        .spawn()
        .map_err(|error| format!("Couldn't open the app. Check that its files are still present and Windows has permission to run it. Details: {error}"))?;
    Ok(())
}

pub fn is_installed(root: &Path, id: &str) -> bool {
    APPS.iter().any(|app| app.id == id) && exe_path(root, id).is_file()
}

// Refuse links/junctions so uninstall can only remove the managed app directory.
fn check_uninstall_tree(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Cannot uninstall a folder containing links or junctions.".into());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err("Cannot uninstall a folder containing links or junctions.".into());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            check_uninstall_tree(&entry.map_err(|error| error.to_string())?.path())?;
        }
    }
    Ok(())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))] { let _ = metadata; false }
}

fn relative_files(root: &Path) -> Result<Vec<String>> {
    fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() { collect(root, &path, files)?; }
            else { files.push(path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/")); }
        }
        Ok(())
    }
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    Ok(files)
}

fn commit_install(apps: &Path, id: &str, prepared: &Path, tag: &str, files: &[String], backup: &Path) -> Result<()> {
    let dest = apps.join(id);
    let version = apps.join(format!("{id}.version"));
    let manifest = apps.join(format!("{id}.files.json"));
    for path in [&version, &manifest] {
        if path.exists() && (!path.is_file() || fs::symlink_metadata(path).map_err(|e| e.to_string())?.file_type().is_symlink() || is_reparse_point(&fs::symlink_metadata(path).map_err(|e| e.to_string())?)) {
            return Err("Install metadata is not a regular file. Your existing app was kept.".into());
        }
    }
    let parent = backup.parent().ok_or("Invalid staging folder")?;
    let next_version = parent.join("next.version");
    let next_manifest = parent.join("next.files.json");
    fs::write(&next_version, tag).map_err(|e| e.to_string())?;
    fs::write(&next_manifest, serde_json::to_vec(files).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let old_version = parent.join("old.version");
    let old_manifest = parent.join("old.files.json");
    let journal = parent.join("transaction.json");
    let state = serde_json::json!({"id": id, "had_app": dest.exists(), "had_version": version.exists(), "had_manifest": manifest.exists(), "committed": false});
    fs::write(&journal, state.to_string()).map_err(|e| e.to_string())?;
    let mut moved_app = false;
    let mut new_app = false;
    let mut moved_version = false;
    let mut moved_manifest = false;
    let mut new_version = false;
    let mut new_manifest = false;
    let result = (|| -> std::io::Result<()> {
        if dest.exists() { fs::rename(&dest, backup)?; moved_app = true; }
        if version.exists() { fs::rename(&version, &old_version)?; moved_version = true; }
        if manifest.exists() { fs::rename(&manifest, &old_manifest)?; moved_manifest = true; }
        fs::rename(prepared, &dest)?; new_app = true;
        fs::rename(&next_version, &version)?; new_version = true;
        fs::rename(&next_manifest, &manifest)?; new_manifest = true;
        let mut done = state.clone(); done["committed"] = true.into();
        fs::write(&journal, done.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = (|| -> std::io::Result<()> {
            if new_manifest { fs::remove_file(&manifest)?; }
            if new_version { fs::remove_file(&version)?; }
            if moved_manifest { fs::rename(&old_manifest, &manifest)?; }
            if moved_version { fs::rename(&old_version, &version)?; }
            if new_app { fs::rename(&dest, prepared)?; }
            if moved_app { fs::rename(backup, &dest)?; }
            Ok(())
        })();
        return Err(match rollback { Ok(()) => { let _ = fs::remove_file(&journal); format!("Couldn't finish installation. Previous app restored: {error}") }, Err(restore) => format!("Installation failed: {error}. Recovery needed: {restore}") });
    }
    Ok(())
}

pub fn recover_interrupted_installs(root: &Path) -> Result<()> {
    let apps = root.join("apps");
    if !apps.exists() { return Ok(()); }
    if is_reparse_point(&fs::symlink_metadata(&apps).map_err(|e| e.to_string())?) { return Err("App storage is a junction. Recovery stopped.".into()); }
    for entry in fs::read_dir(&apps).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_name().to_string_lossy().starts_with(".craft-stage-") { continue; }
        let stage = entry.path(); let journal = stage.join("transaction.json");
        if !journal.is_file() { continue; }
        check_uninstall_tree(&stage)?;
        let state: serde_json::Value = serde_json::from_slice(&fs::read(&journal).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if state["committed"] == true { continue; }
        let id = state["id"].as_str().ok_or("Invalid install recovery record")?;
        if !APPS.iter().any(|app| app.id == id) { return Err("Unknown app in recovery record".into()); }
        let dest = apps.join(id); let backup = stage.join("previous");
        if backup.exists() || state["had_app"] == false {
            if dest.exists() { check_uninstall_tree(&dest)?; fs::rename(&dest, stage.join("interrupted-new-app")).map_err(|e| e.to_string())?; }
            if backup.exists() { fs::rename(&backup, &dest).map_err(|e| e.to_string())?; }
        }
        for (suffix, old, field) in [("version", "old.version", "had_version"), ("files.json", "old.files.json", "had_manifest")] {
            let path = apps.join(format!("{id}.{suffix}")); let previous = stage.join(old);
            if previous.exists() || state[field] == false {
                if path.exists() { check_uninstall_tree(&path)?; fs::remove_file(&path).map_err(|e| e.to_string())?; }
                if previous.exists() { fs::rename(previous, path).map_err(|e| e.to_string())?; }
            }
        }
        fs::remove_file(journal).map_err(|e| e.to_string())?;
        let _ = push_log(root, id, "Interrupted update recovered", "Previous app restored. You can retry the update.");
    }
    Ok(())
}

pub fn app_folder(root: &Path, id: &str) -> PathBuf { root.join("apps").join(id) }

pub fn app_disk_usage(root: &Path, id: &str) -> Result<u64> {
    let dir = app_folder(root, id);
    if !dir.exists() { return Ok(0); }
    check_uninstall_tree(&dir)?;
    relative_files(&dir)?.iter().try_fold(0_u64, |total, file| {
        fs::metadata(dir.join(file)).map(|m| total.saturating_add(m.len())).map_err(|e| e.to_string())
    })
}

pub fn open_folder(path: &Path) -> Result<()> {
    if !path.is_dir() { return Err("Folder does not exist yet. Install the app first.".into()); }
    command("explorer.exe").arg(path).spawn().map_err(|e| format!("Couldn't open folder: {e}"))?;
    Ok(())
}

#[cfg(test)]
pub fn uninstall_app(root: &Path, id: &str) -> Result<UpdateOutcome> {
    uninstall_app_with_data(root, id, true)
}

pub fn uninstall_app_with_data(root: &Path, id: &str, delete_data: bool) -> Result<UpdateOutcome> {
    let app = APPS.iter().find(|app| app.id == id).ok_or_else(|| format!("Unknown app {id}"))?;
    if app_is_running(id)? {
        return Err(format!("{} is open. Close it, then uninstall.", app.name));
    }
    let apps = root.join("apps");
    let dir = apps.join(id);
    let version = apps.join(format!("{id}.version"));
    if apps.exists() {
        let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
        let canonical_apps = apps.canonicalize().map_err(|error| error.to_string())?;
        if canonical_apps.parent() != Some(canonical_root.as_path()) {
            return Err("App storage points outside the launcher data folder.".into());
        }
        if fs::symlink_metadata(&dir).is_ok() {
            check_uninstall_tree(&dir)?;
            let canonical_dir = dir.canonicalize().map_err(|error| error.to_string())?;
            if canonical_dir.parent() != Some(canonical_apps.as_path()) {
                return Err("App folder points outside app storage.".into());
            }
        }
        if fs::symlink_metadata(&version).is_ok() {
            check_uninstall_tree(&version)?;
        }
        if dir.exists() {
            if delete_data {
                fs::remove_dir_all(&dir).map_err(|error| format!("Couldn't uninstall {}: {error}", app.name))?;
            } else {
                let manifest = apps.join(format!("{id}.files.json"));
                let files: Vec<String> = serde_json::from_slice(&fs::read(&manifest).map_err(|_| "This older install has no file list. Use Repair before uninstalling with Keep data, or choose Delete portable data.".to_string())?).map_err(|e| e.to_string())?;
                for file in &files {
                    let path = Path::new(file);
                    if path.is_absolute() || file.contains(':') || file.split(['/', '\\']).any(|part| part == "..") { return Err("App file list contains an unsafe path.".into()); }
                }
                for file in files {
                    let path = dir.join(file);
                    if path.is_file() { fs::remove_file(path).map_err(|e| e.to_string())?; }
                }
            }
        }
        if version.exists() {
            fs::remove_file(&version).map_err(|error| error.to_string())?;
        }
    }
    let mut recent = load_recent(root);
    recent.retain(|item| item != id);
    fs::create_dir_all(logs_dir(root)).map_err(|error| error.to_string())?;
    fs::write(logs_dir(root).join("recent.json"), serde_json::to_string_pretty(&recent).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let manifest = apps.join(format!("{id}.files.json"));
    if manifest.is_file() { fs::remove_file(manifest).map_err(|e| e.to_string())?; }
    push_log(root, id, "Uninstalled", if delete_data { "Removed the app and its portable data. Install to download it again." } else { "Removed app files. Portable settings and projects were kept. Install to use them again." })?;
    Ok(UpdateOutcome { status: format!("{} uninstalled", app.name) })
}

#[cfg(test)]
fn update_binary(root: &Path, app: &CraftApp, on_status: &mut dyn FnMut(String)) -> Result<Option<LogEntry>> {
    update_binary_controlled(root, app, false, &AtomicBool::new(false), on_status, &mut |_| {})
}

fn update_binary_controlled(root: &Path, app: &CraftApp, repair: bool, cancel: &AtomicBool, on_status: &mut dyn FnMut(String), progress: &mut dyn FnMut(Progress)) -> Result<Option<LogEntry>> {
    transfer::check(cancel)?;
    let release = fetch_release(app.id)?;
    transfer::check(cancel)?;
    let installed = read_version(root, app.id);
    let exe = exe_path(root, app.id);
    if !repair && installed == release.tag_name && exe.is_file() {
        ensure_portable_marker(&exe, app.name);
        return Ok(None);
    }
    if exe.is_file() && app_is_running(app.id)? {
        return Err(format!("{} is open. Close it, then update.", app.name));
    }

    let arch = win_arch();
    let asset_name = asset_name(app.id, &release.tag_name, arch);
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("Missing file {asset_name}"))?;

    on_status(format!("{}: downloading {asset_name}", app.name));
    let apps = root.join("apps");
    fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(&apps).map_err(|e| e.to_string())?.file_type().is_symlink() || is_reparse_point(&fs::symlink_metadata(&apps).map_err(|e| e.to_string())?) {
        return Err("App folder is a link or junction. Choose a regular data folder.".into());
    }
    let scratch = tempfile::Builder::new().prefix(".craft-stage-").tempdir_in(&apps).map_err(|e| format!("Couldn't prepare download. Check free disk space: {e}"))?;
    let zip_path = scratch.path().join("package.zip");
    transfer::download(&asset.browser_download_url, &zip_path, cancel, progress)?;
    on_status(format!("{}: unpacking...", app.name));
    let stage = scratch.path().join("unpacked");
    extract_zip_controlled(&zip_path, &stage, cancel)?;
    let found = find_file(&stage, &format!("{}.exe", app.id))
        .ok_or_else(|| format!("Missing {}.exe in the download", app.id))?;
    let src = found.parent().ok_or("Unexpected zip layout")?;
    let dest = exe.parent().ok_or("Bad install path")?.to_path_buf();
    let shipped = relative_files(src)?;
    let prepared = scratch.path().join("prepared");
    // Merge in a private directory so portable user data and the running install
    // are untouched until every download and copy has succeeded.
    if dest.exists() {
        check_uninstall_tree(&dest)?;
        copy_dir_controlled(&dest, &prepared, cancel)?;
    }
    copy_dir_controlled(src, &prepared, cancel)?;
    ensure_portable_marker(&prepared.join(format!("{}.exe", app.id)), app.name);
    transfer::check(cancel)?;
    if app_is_running(app.id)? { return Err(format!("{} is open. Close it, then update.", app.name)); }
    on_status(format!("{}: installing...", app.name));
    if let Err(error) = commit_install(&apps, app.id, &prepared, &release.tag_name, &shipped, &scratch.path().join("previous")) {
        let recovery = scratch.keep();
        return Err(format!("{error}. Recovery files kept at {}", recovery.display()));
    }

    let from = if installed.is_empty() { "none".to_string() } else { installed };
    let mut body = release.name.unwrap_or_default();
    let notes = release.body.unwrap_or_default();
    if !notes.trim().is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(notes.trim());
    }
    Ok(Some(log_entry(
        &format!("App {from} → {}", release.tag_name),
        &trim_chars(&body, 2500),
    )))
}

fn fetch_release(id: &str) -> Result<Release> {
    fetch_release_api(id).or_else(|api_error| {
        fetch_release_page(id).map_err(|page_error| format!("{api_error} · Release page: {page_error}"))
    })
}

fn fetch_release_api(id: &str) -> Result<Release> {
    let url = format!("https://api.github.com/repos/storytold/{id}/releases/latest");
    let response = api_agent()
        .get(&url)
        .set("User-Agent", "ArtcraftLauncher")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("Couldn't reach GitHub: {error}"))?;
    response.into_json::<Release>().map_err(|error| format!("Couldn't read the release: {error}"))
}

fn fetch_release_page(id: &str) -> Result<Release> {
    let response = api_agent()
        .get(&format!("https://github.com/storytold/{id}/releases/latest"))
        .set("User-Agent", "CraftLauncher")
        .call().map_err(|error| error.to_string())?;
    let release_url = response.get_url().to_string();
    let prefix = format!("https://github.com/storytold/{id}/releases/tag/");
    let tag = release_url.strip_prefix(&prefix)
        .filter(|tag| !tag.is_empty() && !tag.contains('/') && !tag.contains('?'))
        .ok_or("Couldn't identify the latest release tag")?.to_string();
    let assets_page = api_agent()
        .get(&format!("https://github.com/storytold/{id}/releases/expanded_assets/{tag}"))
        .set("User-Agent", "CraftLauncher")
        .call().map_err(|error| error.to_string())?
        .into_string().map_err(|error| error.to_string())?;
    let assets = release_assets_from_html(id, &tag, &assets_page);
    if assets.is_empty() {
        return Err("No download files found on the release page".into());
    }
    Ok(Release {
        tag_name: tag.clone(),
        name: Some(format!("{id} {tag}")),
        body: Some(format!("Release notes: {release_url}")),
        assets,
    })
}

fn release_assets_from_html(id: &str, tag: &str, html: &str) -> Vec<Asset> {
    let prefix = format!("/storytold/{id}/releases/download/{tag}/");
    html.split("href=\"").skip(1).filter_map(|part| {
        let path = part.split('"').next()?;
        let name = path.strip_prefix(&prefix)?;
        if name.is_empty() || name.contains('/') {
            return None;
        }
        Some(Asset { name: name.to_string(), browser_download_url: format!("https://github.com{path}") })
    }).collect()
}

#[cfg(test)]
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    extract_zip_controlled(zip_path, dest, &AtomicBool::new(false))
}

fn extract_zip_controlled(zip_path: &Path, dest: &Path, cancel: &AtomicBool) -> Result<()> {
    let file = File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        transfer::check(cancel)?;
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = entry.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') || name.contains(':') || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("Zip path is unsafe".into());
        }
        let out = dest.join(&name);
        if entry.is_dir() || name.ends_with('/') {
            fs::create_dir_all(&out).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut outfile = File::create(&out).map_err(|error| error.to_string())?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            transfer::check(cancel)?;
            let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 { break; }
            outfile.write_all(&buffer[..count]).map_err(|e| format!("Couldn't unpack file. Check free disk space: {e}"))?;
        }
    }
    Ok(())
}

fn copy_dir_controlled(src: &Path, dest: &Path, cancel: &AtomicBool) -> Result<()> {
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(src).map_err(|error| error.to_string())? {
        transfer::check(cancel)?;
        let entry = entry.map_err(|error| error.to_string())?;
        let to = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_controlled(&entry.path(), &to, cancel)?;
        } else {
            fs::copy(entry.path(), &to).map_err(|error| format!("Couldn't copy a file: {error}"))?;
        }
    }
    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|file| file.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}

fn ensure_portable_marker(exe: &Path, name: &str) {
    let Some(dir) = exe.parent() else { return };
    let marker = dir.join("portable.txt");
    if marker.exists() {
        return;
    }
    let text = format!(
        "{name} portable mode.\n\nWhile this file sits next to the program, settings stay beside it when the app supports portable mode.\n"
    );
    let _ = fs::write(marker, text);
}

fn app_is_running(id: &str) -> Result<bool> {
    let image = format!("{id}.exe");
    let output = command("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image}"), "/FO", "CSV", "/NH"])
        .output().map_err(|error| format!("Couldn't check running apps: {error}"))?;
    if !output.status.success() {
        return Err("Couldn't check running apps. Try again before updating or uninstalling.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_ascii_lowercase().contains(&image.to_ascii_lowercase()))
}

fn exe_path(root: &Path, id: &str) -> PathBuf {
    root.join("apps").join(id).join(format!("{id}.exe"))
}

fn logs_dir(root: &Path) -> PathBuf {
    root.join("logs")
}

fn log_path(root: &Path, id: &str) -> PathBuf {
    logs_dir(root).join(format!("{id}.json"))
}

fn push_log(root: &Path, id: &str, title: &str, body: &str) -> Result<LogEntry> {
    fs::create_dir_all(logs_dir(root)).map_err(|error| error.to_string())?;
    let entry = log_entry(title, body);
    let mut entries = load_log(root, id);
    entries.insert(0, entry.clone());
    entries.truncate(40);
    let json = serde_json::to_string_pretty(&entries).map_err(|error| error.to_string())?;
    fs::write(log_path(root, id), json).map_err(|error| error.to_string())?;
    Ok(entry)
}

fn log_entry(title: &str, body: &str) -> LogEntry {
    let at = chrono::Local::now().format("%d %b %Y").to_string();
    LogEntry {
        at,
        title: title.to_string(),
        body: body.trim().to_string(),
    }
}

fn asset_name(id: &str, tag: &str, arch: &str) -> String {
    format!("{id}-{}-windows-{arch}-portable.zip", tag.trim_start_matches('v'))
}

fn win_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => "x64",
    }
}

fn trim_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

fn api_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .redirects(8)
        .build()
}

#[cfg(test)]
fn download_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(600))
        .redirects(8)
        .build()
}

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_commit_restores_previous_app_and_version() {
        let dir = tempfile::tempdir().unwrap(); let apps = dir.path().join("apps"); let stage = apps.join(".craft-stage-test");
        fs::create_dir_all(apps.join("photocraft")).unwrap(); fs::create_dir_all(&stage).unwrap();
        fs::write(apps.join("photocraft/photocraft.exe"), b"old").unwrap(); fs::write(apps.join("photocraft.version"), "v1").unwrap();
        assert!(commit_install(&apps, "photocraft", &stage.join("missing"), "v2", &[], &stage.join("previous")).is_err());
        assert_eq!(fs::read(apps.join("photocraft/photocraft.exe")).unwrap(), b"old");
        assert_eq!(fs::read_to_string(apps.join("photocraft.version")).unwrap(), "v1");
    }

    #[cfg(windows)]
    #[test]
    fn locked_version_file_rolls_back_app_replacement() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap(); let apps = dir.path().join("apps"); let stage = apps.join(".craft-stage-test");
        fs::create_dir_all(apps.join("photocraft")).unwrap(); fs::create_dir_all(stage.join("prepared")).unwrap();
        fs::write(apps.join("photocraft/photocraft.exe"), b"old").unwrap(); fs::write(apps.join("photocraft.version"), "v1").unwrap();
        fs::write(stage.join("prepared/photocraft.exe"), b"new").unwrap();
        let locked = fs::OpenOptions::new().read(true).share_mode(0).open(apps.join("photocraft.version")).unwrap();
        assert!(commit_install(&apps, "photocraft", &stage.join("prepared"), "v2", &[], &stage.join("previous")).is_err());
        assert_eq!(fs::read(apps.join("photocraft/photocraft.exe")).unwrap(), b"old");
        drop(locked); assert_eq!(fs::read_to_string(apps.join("photocraft.version")).unwrap(), "v1");
    }

    #[test]
    fn interrupted_commit_recovers_previous_app_on_startup() {
        let dir = tempfile::tempdir().unwrap(); let apps = dir.path().join("apps"); let stage = apps.join(".craft-stage-test");
        fs::create_dir_all(stage.join("previous")).unwrap(); fs::create_dir_all(apps.join("photocraft")).unwrap();
        fs::write(stage.join("previous/photocraft.exe"), b"old").unwrap(); fs::write(apps.join("photocraft/photocraft.exe"), b"new").unwrap();
        fs::write(stage.join("old.version"), "v1").unwrap(); fs::write(apps.join("photocraft.version"), "v2").unwrap();
        fs::write(stage.join("transaction.json"), r#"{"id":"photocraft","had_app":true,"had_version":true,"had_manifest":false,"committed":false}"#).unwrap();
        recover_interrupted_installs(dir.path()).unwrap(); recover_interrupted_installs(dir.path()).unwrap();
        assert_eq!(fs::read(apps.join("photocraft/photocraft.exe")).unwrap(), b"old");
        assert_eq!(fs::read_to_string(apps.join("photocraft.version")).unwrap(), "v1");
    }

    #[test]
    fn uninstall_can_keep_user_created_portable_data() {
        let dir = tempfile::tempdir().unwrap(); let apps = dir.path().join("apps");
        fs::create_dir_all(apps.join("photocraft/settings")).unwrap();
        fs::write(apps.join("photocraft/photocraft.exe"), b"app").unwrap(); fs::write(apps.join("photocraft/settings/user.json"), b"keep").unwrap();
        fs::write(apps.join("photocraft.files.json"), r#"["photocraft.exe"]"#).unwrap();
        uninstall_app_with_data(dir.path(), "photocraft", false).unwrap();
        assert!(!is_installed(dir.path(), "photocraft")); assert_eq!(fs::read(apps.join("photocraft/settings/user.json")).unwrap(), b"keep");
    }

    #[test]
    fn extraction_rejects_parent_traversal() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap(); let archive = dir.path().join("bad.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        writer.start_file("../outside.txt", zip::write::SimpleFileOptions::default()).unwrap(); writer.write_all(b"bad").unwrap(); writer.finish().unwrap();
        assert!(extract_zip(&archive, &dir.path().join("unpacked")).is_err()); assert!(!dir.path().join("outside.txt").exists());
    }

    #[test]
    #[ignore = "Requires GitHub network access"]
    fn github_release_can_be_downloaded() {
        let release = fetch_release("photocraft").expect("latest release must be reachable");
        let expected = asset_name("photocraft", &release.tag_name, win_arch());
        let asset = release.assets.iter().find(|asset| asset.name == expected).expect("Windows package must exist");
        let response = download_agent().get(&asset.browser_download_url).call().expect("Windows package must download");
        let mut signature = [0; 4];
        response.into_reader().read_exact(&mut signature).unwrap();
        assert_eq!(&signature, b"PK\x03\x04");
        let root = std::env::temp_dir().join(format!("craft-install-live-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let outcome = update_binary(&root, &APPS[0], &mut |_| {}).expect("App ZIP must install successfully");
        assert!(outcome.is_some());
        assert!(is_installed(&root, "photocraft"));
        assert!(!read_version(&root, "photocraft").is_empty());
        assert!(root.join("apps/photocraft/portable.txt").is_file());
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires GitHub network access"]
    fn github_release_pages_have_all_windows_packages() {
        for app in APPS {
            let release = fetch_release_page(app.id).expect(app.name);
            let expected = asset_name(app.id, &release.tag_name, win_arch());
            assert!(release.assets.iter().any(|asset| asset.name == expected), "Missing {expected}");
        }
    }

    #[test]
    fn release_page_assets_include_the_windows_download_only_from_this_release() {
        let html = r#"<a href="/storytold/photocraft/releases/download/v0.5.0/photocraft-0.5.0-windows-x64-portable.zip">Download</a>
            <a href="/storytold/photocraft/releases/download/v0.4.0/old.zip">Old</a>
            <a href="/other/repo/releases/download/v0.5.0/unrelated.zip">Other</a>"#;
        let assets = release_assets_from_html("photocraft", "v0.5.0", html);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, asset_name("photocraft", "v0.5.0", "x64"));
        assert_eq!(assets[0].browser_download_url, "https://github.com/storytold/photocraft/releases/download/v0.5.0/photocraft-0.5.0-windows-x64-portable.zip");
    }

    #[test]
    fn asset_name_matches_windows_portable_zip() {
        assert_eq!(
            asset_name("photocraft", "v0.5.0", "x64"),
            "photocraft-0.5.0-windows-x64-portable.zip"
        );
    }

    #[test]
    fn log_roundtrip_keeps_newest_first() {
        let dir = std::env::temp_dir().join(format!("artcraft-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("apps")).unwrap();
        push_log(&dir, "photocraft", "Installed v0.5.0", "start").unwrap();
        push_log(&dir, "photocraft", "App v0.5.0 → v0.6.0", "notes").unwrap();
        let logs = load_log(&dir, "photocraft");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].title, "App v0.5.0 → v0.6.0");
        assert!(logs[0].body.contains("notes"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn storage_uses_windows_user_data() {
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
        assert_eq!(artcraft_root(), local.join("CraftLauncher"));
        assert_eq!(exe_path(&artcraft_root(), "photocraft"), local.join("CraftLauncher/apps/photocraft/photocraft.exe"));
    }

    #[test]
    fn uninstall_removes_only_selected_app_and_preserves_history() {
        let root = std::env::temp_dir().join(format!("craft-uninstall-test-{}", std::process::id()));
        fs::create_dir_all(root.join("apps/photocraft/settings")).unwrap();
        fs::create_dir_all(root.join("apps/vectorcraft")).unwrap();
        fs::write(exe_path(&root, "photocraft"), b"fake exe").unwrap();
        fs::write(root.join("apps/photocraft/settings/config.json"), b"{}").unwrap();
        fs::write(root.join("apps/photocraft.version"), "v1").unwrap();
        fs::write(exe_path(&root, "vectorcraft"), b"keep").unwrap();
        fs::write(root.join("apps/vectorcraft.version"), "v2").unwrap();
        push_log(&root, "photocraft", "Installed v1", "history").unwrap();
        remember_recent(&root, "vectorcraft");
        remember_recent(&root, "photocraft");
        assert!(is_installed(&root, "photocraft"));
        uninstall_app(&root, "photocraft").unwrap();
        assert!(!root.join("apps/photocraft").exists());
        assert!(!root.join("apps/photocraft.version").exists());
        assert!(!is_installed(&root, "photocraft"));
        assert_eq!(read_version(&root, "photocraft"), "");
        assert_eq!(read_version(&root, "vectorcraft"), "v2");
        assert_eq!(load_recent(&root), vec!["vectorcraft"]);
        let logs = load_log(&root, "photocraft");
        assert_eq!(logs[0].title, "Uninstalled");
        assert_eq!(logs[1].title, "Installed v1");
        assert!(uninstall_app(&root, "../vectorcraft").is_err());
        assert!(exe_path(&root, "vectorcraft").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_executable_is_not_reported_as_installed() {
        let root = std::env::temp_dir().join(format!("craft-missing-test-{}", std::process::id()));
        fs::create_dir_all(root.join("apps")).unwrap();
        fs::write(root.join("apps/photocraft.version"), "v1").unwrap();
        assert_eq!(read_version(&root, "photocraft"), "");
        assert!(!is_installed(&root, "photocraft"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn uninstall_refuses_junction_and_keeps_external_files() {
        let root = std::env::temp_dir().join(format!("craft-junction-test-{}", std::process::id()));
        let external = root.join("outside");
        let junction = root.join("apps/photocraft");
        fs::create_dir_all(&external).unwrap();
        fs::create_dir_all(root.join("apps")).unwrap();
        fs::write(external.join("photocraft.exe"), b"keep").unwrap();
        let status = command("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path $env:CRAFT_TEST_LINK -Target $env:CRAFT_TEST_TARGET -ErrorAction Stop | Out-Null"])
            .env("CRAFT_TEST_LINK", &junction)
            .env("CRAFT_TEST_TARGET", &external)
            .status().unwrap();
        assert!(status.success());
        let error = uninstall_app(&root, "photocraft").err().expect("junction must be rejected");
        assert!(error.contains("links or junctions"));
        assert_eq!(fs::read(external.join("photocraft.exe")).unwrap(), b"keep");
        fs::remove_dir(&junction).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
