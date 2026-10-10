# CraftLauncher v0.9.0 validation

Checked on 10 October 2026. Final UI changes are prepared for the user-authorized push.

## Completed

- Local automated tests: 31 passed, including Cancel inside the app modal, retry after a broken connection, locked-file rollback, interrupted-install recovery, and preserving portable data.
- Live GitHub checks: downloaded and installed PhotoCraft in temporary storage, confirmed Windows packages for all seven apps, and checked launcher releases with API/page fallback.
- Native launcher update: downloaded a real executable from a local HTTP fixture, verified SHA-256 and download size, kept the old file while its process ran, replaced it after exit, and restarted the new executable. Test data was isolated from normal launcher storage.
- Launcher replacement failures: rejected a bad checksum and restored the previous file when the replacement could not start.
- Settings layout: update status and button share one row without overlap at 100%, 125%, 150%, and 200% egui scale.
- Favorite icons: the app cards and sidebar use the same embedded Lucide PNG assets; no custom star geometry remains.
- Native close, minimize, maximize, restore, tray reopen, and tray Exit passed on the current Windows environment. Settings was also inspected visually.
- GitHub Windows build for commit `3c0ac78`: successful. [Workflow run](https://github.com/yyyyynx/craft-launcher/actions/runs/38037366701).

## Release preparation

The current executable and matching checksum are available at the repository root. A copy of the release assets and release text is prepared in `target/release-package/v0.9.0` for review. No tag or release publication is part of this push.

## Checks requiring another environment or publication

- GitHub Actions results for the final pushed commit must be checked separately from the local results above.
- Windows 10 and physical Windows display scaling on other devices have not been tested here. The scale checks above exercise egui layout, not every Windows/DPI combination.
- Updating from a newly published v0.9.0 GitHub release has not been tested: that release is not published. GitHub retrieval and the complete native replacement flow were tested separately.

When publishing the beta, check CI for the final commit, create a matching `v0.9.0` tag, review the draft release assets, and publish the prerelease. Attach both `CraftLauncher.exe` and `SHA256SUMS.txt`. Perform a final update check against those published assets before calling the beta distribution fully verified.
