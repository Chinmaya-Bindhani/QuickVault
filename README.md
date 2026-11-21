# QuickVault

QuickVault keeps frequently reused text on your Windows computer. Press a global shortcut, type a few characters from a title or its content, and copy the selected snippet.

## Use QuickVault

Open **QuickVault.exe** in the project folder. The dark, rounded popup opens with the search field focused. The lightning icon in the Windows notification area opens it again. A never-used vault starts with four editable examples: Email, GitHub, LinkedIn and Portfolio.

- Type to search titles and content. Press the arrow keys or scroll the list for more results. Pinned snippets appear first.
- Click a row or its copy icon, or press **Enter**, to copy the complete snippet. Paste it into the destination app with **Ctrl+V**. **Ctrl+Enter** copies and keeps the popup open.
- Choose **+ Add** to save your own snippet. Right-click an existing row to edit, pin or delete it. **All Snippets** opens management mode, where clicking a row opens its editor. **F2** edits the selected snippet and **Ctrl+N** starts a new one.
- Use the gear for the global shortcut, auto-hide preference, startup option and backup. **Ctrl+,** also opens Settings. **Ctrl+Enter** saves a form. **Esc** returns from a form or closes the picker.
- Press **Alt+F** to open the popup and press it again to close it. Your configured global shortcut also works this way after a change. If Windows reports a shortcut conflict, open Settings through the tray and choose another combination. Closing the popup releases its window, database connection and drawing resources. **Exit QuickVault** in the tray menu stops the process.

## Storage and backup

The first launch creates `%LOCALAPPDATA%\QuickVault\quickvault.sqlite3`. The database stores title, content, category, pin status, creation and update times, and editing revisions. The shortcut and auto-hide preference are local database settings. Starting at sign-in is a per-user Windows Run entry, created only when enabled.

In **Settings**, click **Back up vault** and choose a new file path. The SQLite backup contains a consistent snapshot of snippets and settings. An existing destination is preserved. To restore, close open QuickVault sessions, exit the tray app, keep a copy of your current vault, and place the backup at the database path above.

Snippets and backups are ordinary SQLite files. The app has no accounts, network service, analytics or content logs. Copied text includes the Windows format that requests exclusion from clipboard history and cross-device clipboard sync. [Windows clipboard formats](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats)

## Architecture

| Module | Responsibility |
| --- | --- |
| `main.rs` | Selects the launch mode and enables per-monitor DPI awareness. |
| `tray.rs` | Owns the single-instance guard, tray icon, Windows message loop and global shortcut. A second launch activates the running app. |
| `ui/mod.rs` | Manages the popup, native text fields, keyboard and mouse input, snippet editor and settings. |
| `ui/paint.rs` and `ui/art.rs` | Draw the black glass finish with native GDI+ gradients. Text and vector icons render at the actual pixel size of the window. |
| `database.rs` | Owns the SQLite schema, first-use examples, parameterized queries, paged search, revision-checked edits and backups. |
| `hotkey.rs`, `clipboard.rs` and `startup.rs` | Handle shortcut validation, Unicode clipboard transfers and opt-in startup registration. |

The GitHub and LinkedIn marks use a two-glyph subset of Font Awesome Free Brands 6.7.2, loaded privately while the popup is open. See [the bundled license](assets/font-awesome-license.txt).

The popup and tray share one process. While hidden, it blocks in `GetMessageW` with no polling or rendering loop. Search reads at most six previews at once. Copy reads the selected snippet's full content on demand.

## Build and test

Target 64-bit Windows 10 or 11. Use stable Rust with Cargo and a compatible native C compiler for the bundled SQLite library. For the MSVC Rust target, use a Visual Studio Developer PowerShell with the C++ build tools and Windows SDK resource compiler. GNU targets use `windres` for the application icon. This release was built with Rust 1.100, the x64 Windows GNU/LLVM target and LLVM-MinGW.

Run these commands from the repository root in PowerShell.

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\quickvault.exe
```

On the machine used for this build, load the isolated build tools into the current PowerShell session first.

```powershell
. "$env:LOCALAPPDATA\QuickVault-build\env.ps1"
```

Cargo downloads dependencies on the first build. Once cached, add `--offline` to build without a network connection. The release executable contains SQLite and imports Windows system DLLs. Running it requires neither Rust nor a separate SQLite installation.

## Verification

Run `cargo test --release --locked` for persistence, backup, safe sample initialization, hotkey parsing and UTF-16 clipboard tests. The graphical release was also exercised on Windows 11 x64 with an isolated test vault. The checks covered first launch, add, pin, edit, multiline content search, shortcut changes, auto-hide settings, Escape and reopening the popup. Windows 10 has not been tested on a separate host.
