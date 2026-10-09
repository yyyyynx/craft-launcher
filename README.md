# CraftLauncher

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

## Supported apps

| App | Category | Source |
| --- | --- | --- |
| PhotoCraft | Image | [storytold/photocraft](https://github.com/storytold/photocraft) |
| VectorCraft | Design | [storytold/vectorcraft](https://github.com/storytold/vectorcraft) |
| FilmCraft | Video | [storytold/filmcraft](https://github.com/storytold/filmcraft) |
| LightCraft | Image | [storytold/lightcraft](https://github.com/storytold/lightcraft) |
| PdfCraft | Documents | [storytold/pdfcraft](https://github.com/storytold/pdfcraft) |
| EffectCraft | Video | [storytold/effectcraft](https://github.com/storytold/effectcraft) |
| DesignCraft | Design | [storytold/designcraft](https://github.com/storytold/designcraft) |

## Features

- Open all seven apps from one window.
- Check for releases automatically at startup, or click **Check for updates**.
- Download or update one app at a time, or use **Update all**.
- Uninstall an individual app beside **Update**, or use **Uninstall all** in the sidebar.
- Search by app name and browse by category, available updates, or recently opened apps.
- Read release notes and local update history in each app's **Update log**.
- Enjoy a compact interface with acrylic glass and rounded corners.

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
└── repo/                   # App source repositories, if Git is available
```

Downloaded apps run in portable mode, with their portable data inside their app folders when supported. Moving or replacing `CraftLauncher.exe` does not move or delete installed apps.

Older versions stored apps in `repo/apps` beside the launcher. This version uses the new location; existing files in the old location are not moved or deleted automatically. Use **Install** to install apps in the new location.

**Git is optional.** The launcher also tries to clone or update each app's source repository. Without Git, that step can report a source error, but the launcher still attempts to download the prebuilt app. Rust is only needed if you want to build the launcher yourself.

## Uninstall apps

Select an app and click **Uninstall** beside **Update**, or choose **Uninstall all** in the sidebar. Confirm the removal in the dialog. Close an app before uninstalling it.

Uninstall removes the selected app folder, including any portable settings or projects saved inside it, and its version marker. Files saved elsewhere, update history, and source repositories are kept. Apps in the old storage location are not affected. You can install an app again with **Install**.

## Troubleshooting

- **An app says “Not installed”:** click **Install** and wait for the download to finish.
- **An update says the app is open:** close that app, then try again.
- **GitHub cannot be reached:** check your internet connection and retry. If GitHub limits API requests, the launcher automatically tries the release website instead.
- **A release file is missing:** the upstream release may not include the Windows package expected by the launcher. Check the app's repository linked above.
- **Files cannot be saved:** check free disk space and write access to `%LOCALAPPDATA%\CraftLauncher`.

App updates come from each app's GitHub releases. To update CraftLauncher itself, download the new `CraftLauncher.exe`, close the launcher, and replace the old file in the same folder.

## Special thanks

Special thanks to **[storytold](https://github.com/storytold)**, the creator of ArtCraft, for building these creative tools and sharing their source with the community. Your work is the foundation of CraftLauncher and makes this project possible.

Thank you as well to everyone who contributes to the ArtCraft projects through code, bug reports, documentation, and feedback.

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

Include `CraftLauncher.exe` in the repository so the download link works. You can also attach it to a GitHub Release; keep the release asset name exactly `CraftLauncher.exe`.
