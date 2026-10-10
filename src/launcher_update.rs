use std::{fs, io::Read, path::{Path, PathBuf}, process::Command, sync::atomic::AtomicBool, time::Duration};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use crate::transfer::{self, Progress};

const REPOSITORY: &str = "https://github.com/yyyyynx/craft-launcher";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct ApiRelease { tag_name: String, body: Option<String>, draft: bool, prerelease: bool, assets: Vec<Asset> }
#[derive(Deserialize)]
struct Asset { name: String, browser_download_url: String, size: u64, digest: Option<String> }

pub fn check(include_beta: bool) -> Result<Option<Release>, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    let api = agent.get("https://api.github.com/repos/yyyyynx/craft-launcher/releases?per_page=30")
        .set("User-Agent", "CraftLauncher").call().map_err(|e| e.to_string())
        .and_then(|response| response.into_json::<Vec<ApiRelease>>().map_err(|e| e.to_string()));
    let releases = match api { Ok(releases) => releases, Err(_) => releases_from_pages(&agent)? };
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    let release = releases.into_iter().filter(|r| !r.draft && (include_beta || !r.prerelease))
        .filter_map(|r| semver::Version::parse(r.tag_name.trim_start_matches('v')).ok().map(|v| (v, r)))
        .filter(|(v, _)| v > &current).max_by(|a, b| a.0.cmp(&b.0));
    let Some((_, release)) = release else { return Ok(None); };
    let prefix = format!("{REPOSITORY}/releases/download/{}/", release.tag_name);
    let asset = release.assets.iter().find(|a| a.name == "CraftLauncher.exe").ok_or("New release has no CraftLauncher.exe yet")?;
    if asset.browser_download_url != format!("{prefix}CraftLauncher.exe") { return Err("Unexpected launcher download URL".into()); }
    let checksum = if let Some(checksum) = release.assets.iter().find(|a| a.name == "SHA256SUMS.txt") {
        if checksum.browser_download_url != format!("{prefix}SHA256SUMS.txt") { return Err("Unexpected checksum URL".into()); }
        let text = agent.get(&checksum.browser_download_url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
        checksum_from_text(&text)?
    } else {
        asset.digest.as_deref().and_then(|hash| hash.strip_prefix("sha256:")).ok_or("The release has no SHA-256 checksum. Please ask the maintainer to attach SHA256SUMS.txt.")?.to_string()
    };
    validate_hash(&checksum)?;
    Ok(Some(Release { version: release.tag_name, notes: release.body.unwrap_or_default(), url: asset.browser_download_url.clone(), sha256: checksum.to_ascii_lowercase(), size: asset.size }))
}

fn tags_from_html(html: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for tail in html.split("href=\"/yyyyynx/craft-launcher/releases/tag/").skip(1) {
        let tag = tail.split('"').next().unwrap_or_default();
        if semver::Version::parse(tag.trim_start_matches('v')).is_ok() && !tags.iter().any(|existing| existing == tag) { tags.push(tag.to_string()); }
    }
    tags
}

fn releases_from_pages(agent: &ureq::Agent) -> Result<Vec<ApiRelease>, String> {
    let html = agent.get(&format!("{REPOSITORY}/releases")).set("User-Agent", "CraftLauncher").call().map_err(|e| format!("Couldn't check GitHub releases. Check your connection and retry: {e}"))?.into_string().map_err(|e| e.to_string())?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    let mut releases = Vec::new();
    for tag in tags_from_html(&html) {
        if semver::Version::parse(tag.trim_start_matches('v')).map_err(|e| e.to_string())? <= current { continue; }
        let page = agent.get(&format!("{REPOSITORY}/releases/tag/{tag}")).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
        let expanded = agent.get(&format!("{REPOSITORY}/releases/expanded_assets/{tag}")).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
        let mut assets = Vec::new();
        for name in ["CraftLauncher.exe", "SHA256SUMS.txt"] {
            let path = format!("/yyyyynx/craft-launcher/releases/download/{tag}/{name}");
            if expanded.contains(&format!("href=\"{path}\"")) { assets.push(Asset { name: name.into(), browser_download_url: format!("https://github.com{path}"), size: 0, digest: None }); }
        }
        let prerelease = page.contains("Pre-release") || page.contains("Prerelease");
        releases.push(ApiRelease { tag_name: tag, draft: false, prerelease, body: Some("Open the release on GitHub for full notes.".into()), assets });
    }
    Ok(releases)
}

fn validate_hash(hash: &str) -> Result<(), String> {
    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) { Ok(()) } else { Err("Invalid release checksum".into()) }
}
fn checksum_from_text(text: &str) -> Result<String, String> {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(hash), Some(name)) = (fields.next(), fields.next()) {
            if name.trim_start_matches('*') == "CraftLauncher.exe" { validate_hash(hash)?; return Ok(hash.to_string()); }
        }
    }
    Err("Checksum file does not include CraftLauncher.exe".into())
}
pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut reader = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop { let count = reader.read(&mut buffer).map_err(|e| e.to_string())?; if count == 0 { break; } hash.update(&buffer[..count]); }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn prepare(root: &Path, release: &Release, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<PathBuf, String> {
    validate_hash(&release.sha256)?;
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    // Fail before downloading when Windows would not let us replace this copy.
    let _probe = tempfile::NamedTempFile::new_in(target.parent().ok_or("Launcher folder is missing")?).map_err(|_| "Launcher folder is not writable. Move CraftLauncher.exe to Downloads or Desktop, then try again.".to_string())?;
    let updates = root.join("launcher-updates");
    fs::create_dir_all(&updates).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new().prefix("update-").tempdir_in(updates).map_err(|e| e.to_string())?;
    let executable = stage.path().join("CraftLauncher.exe");
    transfer::download(&release.url, &executable, cancel, progress)?;
    transfer::check(cancel)?;
    if release.size > 0 && fs::metadata(&executable).map_err(|e| e.to_string())?.len() != release.size { return Err("Launcher download size does not match the release".into()); }
    if hash_file(&executable)? != release.sha256 { return Err("Launcher checksum did not match. Existing launcher was kept. Try downloading again.".into()); }
    let mut magic = [0_u8; 2];
    fs::File::open(&executable).map_err(|e| e.to_string())?.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if magic != *b"MZ" { return Err("Download is not a Windows executable".into()); }
    fs::write(stage.path().join("expected.sha256"), &release.sha256).map_err(|e| e.to_string())?;
    let _ = stage.keep();
    Ok(executable)
}

pub fn restart_and_replace(staged: &Path, root: &Path) -> Result<(), String> {
    let stage = staged.canonicalize().map_err(|e| e.to_string())?;
    let updates = root.join("launcher-updates").canonicalize().map_err(|e| e.to_string())?;
    if !stage.starts_with(&updates) || stage.file_name().and_then(|n| n.to_str()) != Some("CraftLauncher.exe") { return Err("Invalid launcher update location".into()); }
    let expected = fs::read_to_string(stage.parent().ok_or("Missing update folder")?.join("expected.sha256")).map_err(|e| e.to_string())?;
    validate_hash(&expected)?;
    if hash_file(&stage)? != expected { return Err("Prepared launcher changed. Download it again.".into()); }
    let helper = stage.parent().unwrap().join("replace.ps1");
    fs::write(&helper, REPLACE_SCRIPT).map_err(|e| e.to_string())?;
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(helper).arg("-Target").arg(target).arg("-Staged").arg(stage)
        .arg("-ParentId").arg(std::process::id().to_string()).arg("-Expected").arg(expected)
        .arg("-ErrorFile").arg(root.join("last-launcher-update-error.txt"));
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }
    command.spawn().map_err(|e| format!("Couldn't start launcher updater: {e}"))?;
    Ok(())
}

pub const REPLACE_SCRIPT: &str = r#"param([string]$Target,[string]$Staged,[int]$ParentId,[string]$Expected,[string]$ErrorFile,[switch]$NoRestart)
$ErrorActionPreference = 'Stop'
$backup = $Target + '.previous-' + [Guid]::NewGuid().ToString('N')
$moved = $false
$installed = $false
try {
    $stream = [IO.File]::OpenRead($Staged)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { $actual = [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','').ToLowerInvariant() } finally { $stream.Dispose(); $algorithm.Dispose() }
    if ($actual -ne $Expected.ToLowerInvariant()) { throw 'Update checksum changed. Existing launcher was kept.' }
    if ($ParentId -gt 0) {
        $parent = Get-Process -Id $ParentId -ErrorAction SilentlyContinue
        if ($parent -and -not $parent.WaitForExit(120000)) { throw 'Launcher did not exit. Try again.' }
    }
    for ($attempt=0; $attempt -lt 50; $attempt++) {
        try { [IO.File]::Move($Target,$backup); $moved=$true; break } catch { if ($attempt -eq 49) { throw }; Start-Sleep -Milliseconds 200 }
    }
    [IO.File]::Copy($Staged,$Target,$false)
    $installed = $true
    if (-not $NoRestart) {
        $replacement = Start-Process -FilePath $Target -WindowStyle Hidden -PassThru
        Start-Sleep -Seconds 3
        if ($replacement.HasExited) { throw 'Updated launcher exited during startup. Previous launcher restored.' }
    }
    if (Test-Path -LiteralPath $ErrorFile) { Remove-Item -LiteralPath $ErrorFile -Force }
    Remove-Item -LiteralPath $backup -Force
} catch {
    $reason = $_.Exception.Message
    try {
        if ($installed -and (Test-Path -LiteralPath $Target)) { Remove-Item -LiteralPath $Target -Force }
        if ($moved) { [IO.File]::Move($backup,$Target) }
        if (-not $NoRestart -and $moved) { Start-Process -FilePath $Target -WindowStyle Hidden }
    } catch { $reason += ' Recovery failed: ' + $_.Exception.Message + ' Backup: ' + $backup }
    [IO.File]::WriteAllText($ErrorFile,$reason)
    exit 1
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_page_tags_are_scoped_to_launcher_and_valid_versions() {
        let html = r#"<a href="/yyyyynx/craft-launcher/releases/tag/v0.10.0">release</a><a href="/yyyyynx/craft-launcher/releases/tag/v0.10.0">duplicate</a><a href="/yyyyynx/craft-launcher/releases/tag/not-a-version">bad</a><a href="/other/repo/releases/tag/v9.0.0">other</a>"#;
        assert_eq!(tags_from_html(html), vec!["v0.10.0"]);
    }
    #[test]
    #[ignore = "Requires GitHub network access"]
    fn launcher_update_check_works_with_api_or_page_fallback() {
        check(true).unwrap();
    }
    #[test]
    fn checksum_is_specific_to_launcher() {
        let hash = "a".repeat(64);
        assert_eq!(checksum_from_text(&format!("{}  Other.exe\n{hash}  CraftLauncher.exe", "b".repeat(64))).unwrap(), hash);
        assert!(checksum_from_text("abc  CraftLauncher.exe").is_err());
    }
    #[test]
    fn versions_compare_numerically() {
        assert!(semver::Version::parse("0.10.0").unwrap() > semver::Version::parse("0.9.0").unwrap());
    }
    #[test]
    fn replacement_helper_rejects_bad_hash_and_preserves_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("old.exe"); let staged = dir.path().join("new.exe");
        fs::write(&target, b"old").unwrap(); fs::write(&staged, b"new").unwrap();
        let script = dir.path().join("replace.ps1"); fs::write(&script, REPLACE_SCRIPT).unwrap();
        let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script).arg("-Target").arg(&target).arg("-Staged").arg(&staged).args(["-ParentId", "0", "-Expected"]).arg("a".repeat(64))
            .arg("-ErrorFile").arg(dir.path().join("error.txt")).arg("-NoRestart").output().unwrap();
        assert!(!output.status.success()); assert_eq!(fs::read(target).unwrap(), b"old");
    }
    #[test]
    fn replacement_helper_swaps_verified_file() {
        let dir = tempfile::tempdir().unwrap(); let target = dir.path().join("old.exe"); let staged = dir.path().join("new.exe");
        fs::write(&target, b"old").unwrap(); fs::write(&staged, b"new").unwrap();
        let script = dir.path().join("replace.ps1"); fs::write(&script, REPLACE_SCRIPT).unwrap();
        let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script).arg("-Target").arg(&target).arg("-Staged").arg(&staged).args(["-ParentId", "0", "-Expected"]).arg(hash_file(&staged).unwrap())
            .arg("-ErrorFile").arg(dir.path().join("error.txt")).arg("-NoRestart").output().unwrap();
        assert!(output.status.success(), "stdout={} stderr={} error={:?}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr), fs::read_to_string(dir.path().join("error.txt"))); assert_eq!(fs::read(target).unwrap(), b"new");
    }

    #[test]
    fn replacement_helper_restores_previous_file_if_new_executable_cannot_start() {
        let dir = tempfile::tempdir().unwrap(); let target = dir.path().join("old.exe"); let staged = dir.path().join("new.exe");
        fs::write(&target, b"old").unwrap(); fs::write(&staged, b"invalid executable").unwrap();
        let script = dir.path().join("replace.ps1"); fs::write(&script, REPLACE_SCRIPT).unwrap();
        let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script).arg("-Target").arg(&target).arg("-Staged").arg(&staged).args(["-ParentId", "0", "-Expected"]).arg(hash_file(&staged).unwrap())
            .arg("-ErrorFile").arg(dir.path().join("error.txt")).output().unwrap();
        assert!(!output.status.success()); assert_eq!(fs::read(target).unwrap(), b"old"); assert!(dir.path().join("error.txt").exists());
    }
}
