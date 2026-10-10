# CraftLauncher v0.9.0 — Public beta

- Download progress with percentages and file sizes, plus Cancel and Retry.
- Direct portable downloads without waiting for source repositories.
- Stage app updates before replacing the installed copy; restore previous files on failure and recover interrupted replacements on startup.
- In-app launcher update checks, SHA-256 verification, and **Restart and update**.
- Settings for startup checks, beta launcher releases, and close-to-tray behavior.
- Installed app sizes, open-folder shortcuts, and Repair / Reinstall.
- Keep user-created portable data during uninstall, with an explicit option to delete it.
- Windows CI tests, executable builds, and checksum assets for GitHub releases.
- Smaller favorite stars with rounded edges and a padded Settings window that stays centered without collapsing.
- Smaller launcher logos in the header and Windows icons, Uninstall all moved into Settings, and update status aligned with its check button.
- PT Sans throughout the interface, consistent Settings text, a circular Settings close button and dimmed backdrop, and compact app management badges beside installed size.

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
- Show the launcher version in the header and in Windows executable properties.
- Retain native window controls: close hides to tray, minimize uses the taskbar, and maximize toggles the window size.
- Use a uniform dark background and rounded native corners.
- Show updates with a purple dot outlined in white and brighter version text. App versions appear beside product names in the popup.
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
