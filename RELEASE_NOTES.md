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
