# CraftLauncher

Current version: **v0.9.0 Public beta**.

**Your ArtCraft apps, together in one place.**

A native Windows launcher for opening, downloading, and updating the ArtCraft creative apps. Choose an app, get its latest portable build, and start creating.

![CraftLauncher logo](assets/artlauncher.png)

## Download

### [Download CraftLauncher.exe](https://github.com/yyyyynx/craft-launcher/raw/HEAD/CraftLauncher.exe)

For Windows 10 / 11 (64-bit, x64). Download the executable above; you do not need Rust, a compiler, or a copy of this repository to use it.

If GitHub opens a file preview, click **Download raw file**. When a release is available, you can also open [Releases](https://github.com/yyyyynx/craft-launcher/releases) and select `CraftLauncher.exe` under **Assets**. The launcher is distributed separately from the creative apps, which it downloads when you choose **Install**.

Project repository: [yyyyynx/craft-launcher](https://github.com/yyyyynx/craft-launcher).

## Get started

1. Download `CraftLauncher.exe`.
2. Double-click it from any location, including Downloads or Desktop. No installation wizard is needed.
3. Wait for the initial update check to finish. An internet connection is required to check for releases and download apps.
4. Select an app and click **Install** to download it. You can also use **Update all** to download all available apps.
5. When the download finishes, click **Open**, or double-click the app card.

Apps marked **Not installed** show an **Install** button and grayscale icons. Installed apps show **Update** when a newer release is available. Close an app before updating it.

Click **×** to hide CraftLauncher in the Windows system tray. Downloads and updates continue in the background. Click the tray icon to reopen the window, or right-click it and choose **Open CraftLauncher** or **Exit**. The icon may be inside the taskbar's hidden-icons arrow. Choose **Exit** before replacing the launcher executable.

Only one instance runs per Windows session. Opening the executable again brings back the existing window, including when it is hidden in the tray.

Click the **star** in the top-left corner of an app icon to add or remove it from **Favorites**. Selected stars turn gold. Choose **Favorites** in the sidebar to view your starred apps, and use search to narrow the list. Favorites are saved between launches and can include apps you have not installed yet.

## Supported apps

| App | Description | Category | Source |
| --- | --- | --- | --- |
| PhotoCraft | Photo editing | Image | [storytold/photocraft](https://github.com/storytold/photocraft) |
| VectorCraft | Vector illustration | Design | [storytold/vectorcraft](https://github.com/storytold/vectorcraft) |
| FilmCraft | Video editing | Video | [storytold/filmcraft](https://github.com/storytold/filmcraft) |
| LightCraft | RAW photo development | Image | [storytold/lightcraft](https://github.com/storytold/lightcraft) |
| PdfCraft | PDF tools | Documents | [storytold/pdfcraft](https://github.com/storytold/pdfcraft) |
| EffectCraft | Motion graphics | Video | [storytold/effectcraft](https://github.com/storytold/effectcraft) |
| DesignCraft | Layout and publishing | Design | [storytold/designcraft](https://github.com/storytold/designcraft) |

Each app popup shows a short description below its name, followed by **GitHub**, **Releases**, and **Website** links. The sidebar categories remain available for filtering apps.

## Features

- Open all seven apps from one window.
- Check for releases automatically at startup, or click **Check for updates**.
- Download or update one app at a time, or use **Update all**.
- See download percentages and file sizes, cancel a download, or retry a failed attempt.
- Download portable app releases directly, without cloning source repositories.
- Keep the previous app until a staged update is ready; restore it if installation fails.
- Repair an installation, see its disk usage, and open its app folder.
- Configure startup checks and tray behavior in **Settings**.
- Check, download, verify, and restart into newer CraftLauncher releases from **Settings**.
- Uninstall an individual app beside **Update**, or use **Uninstall all** in the sidebar.
- Search by app name and browse by category, available updates, or recently opened apps.
- Star favorite apps and browse them from the **Favorites** sidebar filter.
- Read release notes and local update history in each app's **Update log**.
- Visit each app's source, releases, and website directly from its popup.
- Hide the launcher in the system tray while downloads and updates continue.
- Reopen the existing window when launching again, with one tray icon per Windows session.
- Enjoy a compact dark interface with rounded corners.
- Read actionable error messages when opening apps, checking releases, or installing updates fails.

## Where files are stored

Apps are stored in **`%LOCALAPPDATA%\CraftLauncher\apps`**, regardless of where you save the launcher. You can paste `%LOCALAPPDATA%\CraftLauncher` into the Windows Explorer address bar to open its data folder.

```text
%LOCALAPPDATA%\CraftLauncher\
├── apps/
│   ├── photocraft/
│   ├── vectorcraft/
│   ├── ...
│   └── photocraft.version
├── logs/                   # Update history and recently opened apps
├── favorites.json          # Starred apps
├── settings.json           # Launcher preferences
└── launcher-updates/        # Verified launcher update staging
```

Downloaded apps run in portable mode, with their portable data inside their app folders when supported. Moving or replacing `CraftLauncher.exe` does not move or delete installed apps.

Older versions stored apps in `repo/apps` beside the launcher. This version uses the new location; existing files in the old location are not moved or deleted automatically. Use **Install** to install apps in the new location.

Git and Rust are not needed to install apps. Downloads use the prebuilt portable packages published by each app's creator. Older source repositories are left in place.

During updates, the downloaded package and a prepared copy of the app use temporary disk space on the same drive as app storage. Existing portable files are copied into the prepared installation before release files are overlaid. The old app stays in place during downloading and unpacking. A failed installation restores the old app; interrupted replacements are recovered on the next startup. Recovery files are retained if cleanup or recovery needs attention.

Cancel stops the current download or preparation step. Network requests may take a few seconds to stop. The final replacement runs to completion once it begins. Retry starts a fresh download; partial downloads are not resumed. When the server does not provide a file size, the launcher displays downloaded bytes instead of a percentage.

## Uninstall apps

Select an app and click **Uninstall** beside **Update**, or choose **Uninstall all** in the sidebar. Confirm the removal in the dialog. Close an app before uninstalling it.

By default, uninstall removes files recorded in the downloaded package and keeps user-created portable files. Select **Delete portable data** to remove the entire app folder, including settings and projects inside it. Files saved elsewhere and update history are kept. An older installation without a package file list needs **Repair / Reinstall** before uninstalling with data preservation. Back up projects before uninstalling: files supplied by the package are treated as app files even if edited later.

## Troubleshooting

- **An app says “Not installed”:** click **Install** and wait for the download to finish.
- **An update says the app is open:** close that app, then try again.
- **GitHub cannot be reached:** check your internet connection and retry. If GitHub limits API requests, the launcher automatically tries the release website instead.
- **A release file is missing:** the upstream release may not include the Windows package expected by the launcher. Check the app's repository linked above.
- **Files cannot be saved:** check free disk space and write access to `%LOCALAPPDATA%\CraftLauncher`.

App updates come from each app's GitHub releases. For CraftLauncher, open **Settings**, choose **Check launcher updates**, then **Download launcher update** and **Restart and update**. Startup checks are enabled by default. Beta releases are included by default and can be disabled in Settings. The update is checked against the GitHub release's SHA-256 checksum before a helper waits for the old process to exit, replaces the executable, and starts the new one. If replacement or startup fails, it attempts to restore the previous executable and reports the error on startup. This requires a writable launcher folder; Downloads or Desktop work well. Older launchers need one manual upgrade to this version to gain in-app updating.

Checksums verify download integrity; the executable remains unsigned and Windows SmartScreen may still show a warning. Public beta testing is ongoing. Report issues with the launcher version and error message, without including private project files.

See [CraftLauncher v0.9.0 release notes](RELEASE_NOTES.md) for the latest changes.

## Version history

| Version | Changes |
| --- | --- |
| v0.1.0 | Initial Windows launcher for seven ArtCraft apps, individual installation and updates, category filters, search, update history, uninstall, and storage in LocalAppData. |
| v0.1.1 | System tray support, app website and GitHub links, cleaner centered popups, aligned header controls, and a single-instance guard to prevent duplicate tray icons. |
| v0.1.2 | Clearer installation and launch errors, version metadata in the executable, corrected window controls, consistent rounded window edges, brighter update indicators, and clearer log dates. |
| v0.2.0 | Persistent Favorites with star buttons, a Favorites filter, app counts, and search. |
| v0.9.0 Public beta | Download percentage and file size, Cancel and Retry, direct portable downloads, staged installation and rollback, startup recovery, in-app launcher updates with checksum verification, Settings, installed size, app folders, Repair, optional portable-data removal, and Windows CI builds with release checksums. Smaller rounded favorite stars and padded, fixed Settings window. |

The intermediate roadmap milestones v0.3.0–v0.8.0 were consolidated into v0.9.0 rather than published as separate versions. Launcher updates and Settings (v0.3.0), app management (v0.4.0), and build/release tooling (v0.5.0) are included in this beta. This table records implemented versions; it does not claim separate releases for intermediate milestones.

Full details: [Release notes](RELEASE_NOTES.md).

## Special thanks

Special thanks to **[storytold](https://github.com/storytold)**, the creator of ArtCraft, for building these creative tools and sharing their source with the community. Your work is the foundation of CraftLauncher and makes this project possible.

Thank you as well to everyone who contributes to the ArtCraft projects through code, bug reports, documentation, and feedback.

Favorite star icons are from [Lucide](https://lucide.dev/). Their original SVG and license are included in `assets/icons`; PNG variants are embedded in the executable.

CraftLauncher is a separate launcher project. The ArtCraft apps remain the work of their respective creators and contributors; each app's own license and notices apply.

## Build from source

For developers, use a Rust toolchain supporting edition 2024 and a Windows build environment including the Windows SDK. The build automatically embeds the launcher icon into the executable.

```powershell
cargo build --release --locked
.\target\release\CraftLauncher.exe
```

To prepare the downloadable executable used by this README:

```powershell
Move-Item .\target\release\CraftLauncher.exe .\CraftLauncher.exe -Force
```

Include `CraftLauncher.exe` in the repository so the download link works. In-app updates require a published GitHub Release with a version tag such as `v0.9.1` and assets named exactly `CraftLauncher.exe` and `SHA256SUMS.txt`. The checksum file uses `SHA256  CraftLauncher.exe` on one line. GitHub's asset SHA-256 digest is also accepted when no checksum file is attached.

The Windows GitHub Actions workflow runs tests and builds the executable on pushes and pull requests. Pushing a `v*` tag builds and uploads the executable plus checksum to a draft release; review and publish it on GitHub to make it visible to users. Tags below v1.0.0 are marked as prereleases. CI artifacts are available without publishing a release.
