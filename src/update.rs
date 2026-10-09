use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

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

pub fn update_app(root: &Path, id: &str, on_status: &mut dyn FnMut(String)) -> Result<UpdateOutcome> {
    let app = APPS.iter().find(|app| app.id == id).ok_or_else(|| format!("unknown app {id}"))?;
    let mut notes = Vec::new();

    on_status(format!("{}: pulling source...", app.name));
    match update_source(root, app) {
        Ok(Some(entry)) => {
            notes.push(entry.title.clone());
            let _ = push_log(root, app.id, &entry.title, &entry.body);
        }
        Ok(None) => {}
        Err(error) => notes.push(format!("source: {error}")),
    }

    on_status(format!("{}: checking the release...", app.name));
    match update_binary(root, app, on_status) {
        Ok(Some(entry)) => {
            notes.push(entry.title.clone());
            let _ = push_log(root, app.id, &entry.title, &entry.body);
        }
        Ok(None) => {}
        Err(error) => {
            let _ = push_log(root, app.id, "App update failed", &error);
            return Err(error);
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
        return Err("Not installed. Update to download the app.".into());
    }
    let dir = exe.parent().unwrap_or(root).to_path_buf();
    Command::new(&exe)
        .current_dir(dir)
        .spawn()
        .map_err(|error| format!("Couldn't open the app: {error}"))?;
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

pub fn uninstall_app(root: &Path, id: &str) -> Result<UpdateOutcome> {
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
            fs::remove_dir_all(&dir).map_err(|error| format!("Couldn't uninstall {}: {error}", app.name))?;
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
    push_log(root, id, "Uninstalled", "Removed the app and its portable data. Update to install it again.")?;
    Ok(UpdateOutcome { status: format!("{} uninstalled", app.name) })
}

fn update_source(root: &Path, app: &CraftApp) -> Result<Option<LogEntry>> {
    let dir = root.join("repo").join(app.id);
    if !dir.join(".git").is_dir() {
        fs::create_dir_all(root.join("repo")).map_err(|error| error.to_string())?;
        let status = git_at(
            root,
            &[
                "clone",
                &format!("https://github.com/storytold/{}.git", app.id),
                &dir.to_string_lossy(),
            ],
        )?;
        let head = git(&dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
        return Ok(Some(log_entry(
            &format!("Cloned source {head}"),
            &status,
        )));
    }

    git(&dir, &["fetch", "--prune", "--quiet", "origin"])?;
    let local = git(&dir, &["rev-parse", "HEAD"])?;
    let remote = git(&dir, &["rev-parse", "origin/main"])?;
    if local == remote {
        return Ok(None);
    }
    let count = git(&dir, &["rev-list", "--count", "HEAD..origin/main"])?
        .parse::<usize>()
        .unwrap_or(0);
    let subjects = git(&dir, &["log", "-n", "12", "--pretty=format:%h  %s", "HEAD..origin/main"])?;
    let short_local = git(&dir, &["rev-parse", "--short", "HEAD"]).unwrap_or(local);
    git(&dir, &["pull", "--ff-only", "origin", "main"])?;
    let short_new = git(&dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let mut body = subjects;
    if count > 12 {
        body.push_str(&format!("\n…and {} more commits", count - 12));
    }
    Ok(Some(log_entry(
        &format!("Source +{count} · {short_local} → {short_new}"),
        &body,
    )))
}

fn update_binary(root: &Path, app: &CraftApp, on_status: &mut dyn FnMut(String)) -> Result<Option<LogEntry>> {
    let release = fetch_release(app.id)?;
    let installed = read_version(root, app.id);
    let exe = exe_path(root, app.id);
    if installed == release.tag_name && exe.is_file() {
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
    let zip_path = std::env::temp_dir().join(&asset_name);
    let stage = std::env::temp_dir().join(format!("artcraft-stage-{}-{}", app.id, release.tag_name.trim_start_matches('v')));
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir_all(&stage);
    download(&asset.browser_download_url, &zip_path, |fraction| {
        let pct = (fraction * 100.0).round() as u32;
        on_status(format!("{}: downloading {pct}%", app.name));
    })?;

    extract_zip(&zip_path, &stage)?;
    let found = find_file(&stage, &format!("{}.exe", app.id))
        .ok_or_else(|| format!("Missing {}.exe in the download", app.id))?;
    let src = found.parent().ok_or("Unexpected zip layout")?;
    let dest = exe.parent().ok_or("Bad install path")?.to_path_buf();
    copy_dir(src, &dest)?;
    ensure_portable_marker(&dest.join(format!("{}.exe", app.id)), app.name);
    fs::create_dir_all(root.join("apps")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("apps").join(format!("{}.version", app.id)),
        &release.tag_name,
    )
    .map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&zip_path);
    let _ = fs::remove_dir_all(&stage);

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

fn download(url: &str, dest: &Path, mut on_progress: impl FnMut(f32)) -> Result<()> {
    let response = download_agent()
        .get(url)
        .set("User-Agent", "ArtcraftLauncher")
        .call()
        .map_err(|error| format!("Download failed: {error}"))?;
    let total = response.header("Content-Length").and_then(|value| value.parse::<u64>().ok());
    let mut reader = response.into_reader();
    let mut file = File::create(dest).map_err(|error| error.to_string())?;
    let mut buf = [0_u8; 64 * 1024];
    let mut got = 0_u64;
    let mut last_pct = 0_u32;
    loop {
        let read = reader.read(&mut buf).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read]).map_err(|error| error.to_string())?;
        got += read as u64;
        if let Some(total) = total.filter(|total| *total > 0) {
            let pct = (got * 100 / total) as u32;
            if pct != last_pct {
                last_pct = pct;
                on_progress(got as f32 / total as f32);
            }
        }
    }
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = entry.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
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
        std::io::copy(&mut entry, &mut outfile).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(src).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let to = dest.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &to)?;
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
    let at = chrono::Local::now().format("%d %b %H:%M").to_string();
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

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = command("git").arg("-C").arg(dir).args(args).output().map_err(|error| format!("git: {error}"))?;
    command_text(output)
}

fn git_at(dir: &Path, args: &[&str]) -> Result<String> {
    let output = command("git").current_dir(dir).args(args).output().map_err(|error| format!("git: {error}"))?;
    command_text(output)
}

fn command_text(output: std::process::Output) -> Result<String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if err.is_empty() {
            Err(format!("Command exited with {}", output.status))
        } else {
            Err(err)
        }
    }
}

fn api_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .redirects(8)
        .build()
}

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
