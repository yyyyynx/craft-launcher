# CraftLauncher release history

This file records changes in the implemented launcher versions, including follow-up fixes made before the next version bump. v0.3.0–v0.8.0 were roadmap milestones consolidated into v0.9.0; they were not published as separate versions. v1.0.0 remains a future milestone.

# CraftLauncher v0.9.0 — Public beta

## Downloads and app updates

- Download progress with percentages and file sizes, plus Cancel and Retry.
- Direct portable downloads without waiting for source repositories.
- Stage app updates before replacing the installed copy; restore previous files on failure and recover interrupted replacements on startup.
- Show downloaded bytes when the server does not provide a total size. Retry starts a fresh download; partial downloads are not resumed.
- Preserve existing portable files while preparing an update. Check running apps before replacement and reject unsafe archive paths and filesystem junctions.

## Launcher updates and Settings

- Check for newer CraftLauncher versions from GitHub, download and verify the executable with SHA-256, then **Restart and update**.
- Wait for the existing launcher to exit before replacing it. Keep a backup and attempt recovery if replacement or startup fails.
- Fall back to GitHub release pages when the API is rate-limited.
- Save Settings for app/launcher startup checks, beta launcher releases, and close-to-tray behavior. Beta inclusion applies to CraftLauncher updates.
- Keep version and update status together on the left, with **Check launcher updates** centered beside them on the right.

## App management

- Display installed app sizes with compact **Open app folder** and **Repair / Reinstall** badges on the right of the same row. Installed size aligns with the bottom of the row.
- Repair an existing installation by downloading its portable package again.
- Keep user-created portable data during uninstall, with an explicit option to delete it.
- Move bulk uninstall into Settings as **Uninstall All Apps**, aligned on the left.
- Enlarge the portable-data checkbox and its spacing; align Cancel and Uninstall on the right of confirmation popups.

## Interface

- Use embedded PT Sans throughout the app, with a symbol fallback for characters such as version-transition arrows.
- Give Settings body text and buttons consistent font sizes; enlarge sidebar Settings and update-check text slightly.
- Show Settings as a centered modal with padding, rounded corners, a dimmed backdrop, and the same circular close button as the launcher. Remove collapse and resize controls.
- Use Lucide stars on both app cards and the Favorites sidebar. Keep their edges rounded and scale card favorite controls with app icons as the window grows.
- Reduce the launcher logo size in the header and Windows icons.
- Format log dates with a year when recorded and omit the time. Legacy entries without a year retain their original date information.

## Build and validation

- Windows CI tests, executable builds, and checksum assets for GitHub releases.
- Version tags prepare draft releases with executable and checksum assets; releases below v1.0.0 are marked as prereleases.
- Local tests cover retry/cancel, rollback, recovery, data preservation and launcher replacement. See [validation results and remaining checks](BETA_VALIDATION.md).

This beta includes the planned download, launcher update, app management, and release tooling changes from v0.2.0 onward. Favorites remain available. Download **CraftLauncher.exe** once to upgrade from older versions; future published releases can be installed from Settings. Windows 10/11 x64; executable currently unsigned.

---

# CraftLauncher v0.2.0

- Star apps directly from their cards. Selected stars turn gold; click again to remove them.
- Browse starred apps using **Favorites** in the sidebar, with an app count and search support.
- Favorites are saved between launches, including apps that are not installed yet.
- Clicking a star changes its favorite status without opening the app popup.

Choose **Exit** from the tray menu before replacing **CraftLauncher.exe**. Installed apps and update history are kept.

---

# CraftLauncher v0.1.2

## Stability update

- Improved error reporting for app launches, downloads, and update checks. Errors now appear in a readable dialog with suggested next steps.
- Keep the app popup open when launching an app fails.
- Close the app popup after a successful **Open** action; other actions keep it open.
- Show the launcher version in the header and in Windows executable properties.
- Retain native window controls: close hides to tray, minimize uses the taskbar, and maximize toggles the window size.
- Use a uniform dark background and rounded native corners.
- Show updates with a purple dot outlined in white and brighter version text. App versions appear beside product names in the popup.
- Remove the extra Update label beneath app cards; use the dot to indicate an available update. Show installed and available version transitions with clearer colors.
- Display clearer log dates without the time. New entries include the year; older entries keep the date information originally recorded.

Download **CraftLauncher.exe** from the release assets. Choose **Exit** from the tray menu before replacing the previous executable. Installed apps and update history are kept.

---

# CraftLauncher v0.1.1

System tray support, cleaner app popups, and a fix for duplicate launcher instances.

## New

- Click **×** to hide CraftLauncher in the Windows system tray. Downloads and updates continue in the background.
- Click the tray icon to reopen the window, or right-click for **Open CraftLauncher** and **Exit**.
- Open an app's **GitHub**, **Releases**, or **Website** directly from its popup.

## Improvements

- App popups now show short descriptions such as **Photo editing** and **Vector illustration** below the app name. Categories remain available as sidebar filters.
- Moved the app links below the description.
- Aligned the launcher logo, search bar, and window controls.

## Fixes

- Fixed repeated launches creating multiple windows and tray icons. Opening CraftLauncher again now brings back the existing window, even when hidden in the tray.

## Download and update

Download **CraftLauncher.exe** from the release assets and run it on Windows 10/11 (x64). No installation wizard or Rust installation needed.

Already using CraftLauncher? Right-click its tray icon and choose **Exit** before replacing your old executable. Your installed apps and update history will be kept.

Thanks for trying it out! Bug reports and feedback are always welcome.

---

# CraftLauncher v0.1.0

## Initial launcher

- Native Windows 10/11 x64 executable for PhotoCraft, VectorCraft, FilmCraft, LightCraft, PdfCraft, EffectCraft, and DesignCraft.
- Install and open individual apps; check for updates at startup or manually, and update one app or use **Update all**.
- Search by app name and filter by category, available updates, or recently opened apps.
- Show release notes and retain local app update history.
- Show uninstalled apps with grayscale icons and an **Install** action.
- Uninstall individual apps or all apps through confirmation dialogs.
- Store downloaded apps in `%LOCALAPPDATA%\CraftLauncher\apps`, independently of the launcher executable's location.
- Run downloaded portable executables without requiring a Rust installation. The original download flow also attempted optional source repository updates when Git was available; v0.9.0 removes that wait.

## Follow-up polish

- Add padding to uninstall confirmation dialogs.
- Embed the launcher icon in the Windows executable.

CraftLauncher is a separate community launcher project. ArtCraft apps and their source remain the work of their respective creators, with their own licenses.
