//! Signage Studio — Windows-installer.
//!
//! Eén zelfstandige setup die de native Signage Studio-app installeert:
//! bestanden, startmenu- en bureaubladsnelkoppelingen, een uninstaller en een
//! vermelding bij *Apps en onderdelen*. Geen administratorrechten nodig.
//!
//! De app zelf zit in de setup; de gebruikersgegevens in
//! `%LOCALAPPDATA%\Signage\data` worden nooit aangeraakt.

use std::env;
use std::fs;
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use mslnk::ShellLink;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
};

/* ------------------------------------------------------------- ingebakken app */

const APP_NAME: &str = "Signage Studio";
const APP_EXE_NAME: &str = "Signage Studio.exe";
const UNINSTALLER_NAME: &str = "Uninstall.exe";
const ICON_NAME: &str = "Signage.ico";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SW_SHOWNORMAL: i32 = 1;

const APP_EXE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../native/target/release/signage-studio.exe"
));
const UNINSTALLER_EXE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/uninstaller/target/release/Signage-Studio-Uninstall.exe"
));
const README: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../native/README.md"
));
const LICENSE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../LICENSE"
));
const NOTICE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../NOTICE"
));
const THIRD_PARTY_NOTICES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../THIRD-PARTY-NOTICES.md"
));
const LICENSE_FILES: &[(&str, &[u8])] = &[
    (
        "DeckCraft-NOTICE.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/NOTICE"
        )),
    ),
    (
        "DeckCraft-ATTRIBUTION.md",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/ATTRIBUTION.md"
        )),
    ),
    (
        "DeckCraft-LICENSE-APACHE.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/LICENSE-APACHE"
        )),
    ),
    (
        "DeckCraft-LICENSE-MIT.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/LICENSE-MIT"
        )),
    ),
    (
        "pdf-writer-LICENSE-ICC.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/licenses/pdf-writer/LICENSE-ICC.txt"
        )),
    ),
    (
        "ArtCraft-brand-LICENSE.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../deckcraft/deckcraft-main/docs/brand/LICENSE-brand.txt"
        )),
    ),
    (
        "DeckCraft-font-licenses.txt",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../licenses/DeckCraft-font-licenses.txt"
        )),
    ),
];
const ICON_ICO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Signage.ico"
));

/* --------------------------------------------------------------------- opties */

#[derive(Default)]
struct Opts {
    help: bool,
    silent: bool,
    desktop: bool,
    no_shortcuts: bool,
    no_registry: bool,
    dir: Option<PathBuf>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut opts = Opts {
            desktop: true,
            ..Opts::default()
        };
        let mut i = 0;
        while i < args.len() {
            let raw = args[i].clone();
            let (flag, inline) = match raw.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (raw.clone(), None),
            };
            let value = |i: &mut usize| -> Option<String> {
                if let Some(v) = inline.clone() {
                    return Some(v);
                }
                if *i + 1 < args.len() && !args[*i + 1].starts_with('-') {
                    *i += 1;
                    return Some(args[*i].clone());
                }
                None
            };

            match flag.as_str() {
                "--help" | "-h" | "/?" => opts.help = true,
                "--silent" | "/S" | "-s" => opts.silent = true,
                "--desktop" => opts.desktop = true,
                "--no-desktop" => opts.desktop = false,
                "--no-shortcuts" => opts.no_shortcuts = true,
                "--no-registry" => opts.no_registry = true,
                "--dir" => {
                    opts.dir = value(&mut i).map(PathBuf::from);
                    if opts.dir.is_none() {
                        return Err("--dir heeft een pad nodig".into());
                    }
                }
                other => return Err(format!("onbekende optie: {other}")),
            }
            i += 1;
        }
        Ok(opts)
    }
}

/* ---------------------------------------------------------------------- paden */

fn local_appdata() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_root() -> PathBuf {
    local_appdata().join("Programs").join(APP_NAME)
}

fn start_menu_root() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|base| {
        PathBuf::from(base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
    })
}

fn start_menu_dir() -> Option<PathBuf> {
    start_menu_root().map(|base| base.join(APP_NAME))
}

fn expand_env_vars(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            match env::var(&after[..end]) {
                Ok(found) => out.push_str(&found),
                Err(_) => {
                    out.push('%');
                    out.push_str(&after[..end]);
                    out.push('%');
                }
            }
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Het echte bureaublad. OneDrive kan de map verplaatsen, dus vraag het pad op
/// in plaats van blind `%USERPROFILE%\Desktop` te gebruiken.
fn desktop_dir() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) =
        hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
    {
        if let Ok(value) = key.get_value::<String, _>("Desktop") {
            let path = PathBuf::from(expand_env_vars(&value));
            if !path.as_os_str().is_empty() {
                return Some(path);
            }
        }
    }
    Some(env::var_os("USERPROFILE")?.into()).map(|p: PathBuf| p.join("Desktop"))
}

/* --------------------------------------------------------------------- dialoog */

fn win_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message(title: &str, text: &str, style: u32) -> i32 {
    let body = wide(text);
    let caption = wide(title);
    unsafe { MessageBoxW(std::ptr::null_mut(), body.as_ptr(), caption.as_ptr(), style) }
}

fn info(text: &str) {
    message(APP_NAME, text, MB_OK | MB_ICONINFORMATION);
}

fn error(text: &str) {
    message(APP_NAME, text, MB_OK | MB_ICONERROR);
}

fn confirm(text: &str) -> bool {
    message(APP_NAME, text, MB_YESNO | MB_ICONQUESTION) == IDYES
}

fn open_app(path: &Path) {
    let op = wide("open");
    let file = wide(&win_path(path));
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

/* -------------------------------------------------------------- procesbeheer */

fn app_running() -> bool {
    let filter = format!("IMAGENAME eq {APP_EXE_NAME}");
    let output = Command::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains(&APP_EXE_NAME.to_ascii_lowercase()),
        Err(_) => false,
    }
}

fn stop_app() -> Result<(), String> {
    if !app_running() {
        return Ok(());
    }

    let _ = Command::new("taskkill")
        .args(["/IM", APP_EXE_NAME, "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(Duration::from_millis(700));

    if app_running() {
        Err(format!(
            "{APP_NAME} is nog geopend. Sluit de app en start de installer opnieuw."
        ))
    } else {
        Ok(())
    }
}

/* ----------------------------------------------------------------- installeren */

struct Report {
    root: PathBuf,
    bytes: u64,
}

fn dir_size(path: &Path) -> Option<u64> {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let meta = entry.metadata().ok()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Some(total)
}

fn make_lnk(
    target: &Path,
    args: &str,
    workdir: &Path,
    icon: Option<&Path>,
    path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("map {} aanmaken: {e}", parent.display()))?;
    }
    let mut link = ShellLink::new(target).map_err(|e| format!("snelkoppeling maken: {e}"))?;
    link.set_arguments(Some(args.to_string()));
    link.set_working_dir(Some(win_path(workdir)));
    link.set_name(Some(APP_NAME.to_string()));
    if let Some(icon) = icon {
        link.set_icon_location(Some(win_path(icon)));
    }
    link.create_lnk(path)
        .map_err(|e| format!("{} schrijven: {e}", path.display()))
}

fn make_shortcuts(
    app: &Path,
    uninstaller: &Path,
    root: &Path,
    icon: &Path,
    desktop: bool,
) -> Result<(), String> {
    if let Some(dir) = start_menu_dir() {
        make_lnk(
            app,
            "",
            root,
            Some(icon),
            &dir.join(format!("{APP_NAME}.lnk")),
        )?;
        make_lnk(
            uninstaller,
            "",
            root,
            Some(icon),
            &dir.join(format!("{APP_NAME} verwijderen.lnk")),
        )?;
    }

    if desktop {
        if let Some(dir) = desktop_dir() {
            make_lnk(
                app,
                "",
                root,
                Some(icon),
                &dir.join(format!("{APP_NAME}.lnk")),
            )?;
        }
    }

    // Ruim de snelkoppeling van de eerdere losse/handmatige installatie op.
    if let Some(base) = start_menu_root() {
        let _ = fs::remove_file(
            base.join("Signage")
                .join(format!("{APP_NAME}.lnk")),
        );
    }

    Ok(())
}

fn register_uninstall(root: &Path, uninstaller: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_NAME}"
        ))
        .map_err(|e| format!("registersleutel aanmaken: {e}"))?;

    let size_kb: u64 = dir_size(root).unwrap_or(0) / 1024 + 64;
    let exe = win_path(uninstaller);
    let icon = root.join(ICON_NAME);

    key.set_value("DisplayName", &APP_NAME.to_string()).ok();
    key.set_value("DisplayVersion", &VERSION.to_string()).ok();
    key.set_value("Publisher", &"Signage".to_string()).ok();
    key.set_value("URLInfoAbout", &"https://github.com/DrIamlegion/signage".to_string())
        .ok();
    key.set_value("InstallLocation", &win_path(root)).ok();
    key.set_value(
        "DisplayIcon",
        &format!("{},0", win_path(&icon)),
    )
    .ok();
    key.set_value("UninstallString", &format!("\"{exe}\"")).ok();
    key.set_value(
        "QuietUninstallString",
        &format!("\"{exe}\" --silent"),
    )
    .ok();
    key.set_value("EstimatedSize", &(size_kb as u32)).ok();
    key.set_value("NoModify", &1u32).ok();
    key.set_value("NoRepair", &1u32).ok();
    Ok(())
}

fn write_with_retry(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..8 {
        match fs::write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                if attempt < 7 {
                    std::thread::sleep(Duration::from_millis(400));
                }
            }
        }
    }
    Err(format!("{} schrijven: {last}", path.display()))
}

fn install_license_files(root: &Path) -> Result<(), String> {
    write_with_retry(&root.join("NOTICE"), NOTICE)?;
    write_with_retry(&root.join("THIRD-PARTY-NOTICES.md"), THIRD_PARTY_NOTICES)?;

    let dir = root.join("licenses");
    fs::create_dir_all(&dir)
        .map_err(|e| format!("licensemap {} aanmaken: {e}", dir.display()))?;
    for (name, bytes) in LICENSE_FILES {
        write_with_retry(&dir.join(name), bytes)?;
    }
    Ok(())
}

fn install(opts: &Opts) -> Result<Report, String> {
    let root = opts.dir.clone().unwrap_or_else(default_root);
    let app = root.join(APP_EXE_NAME);
    let uninstaller = root.join(UNINSTALLER_NAME);
    let icon = root.join(ICON_NAME);

    fs::create_dir_all(&root)
        .map_err(|e| format!("installatiemap {} aanmaken: {e}", root.display()))?;

    write_with_retry(&app, APP_EXE)?;
    write_with_retry(&uninstaller, UNINSTALLER_EXE)?;
    fs::write(root.join("LEESMIJ.md"), README)
        .map_err(|e| format!("LEESMIJ.md schrijven: {e}"))?;
    fs::write(root.join("LICENSE"), LICENSE)
        .map_err(|e| format!("LICENSE schrijven: {e}"))?;
    install_license_files(&root)?;
    fs::write(&icon, ICON_ICO)
        .map_err(|e| format!("{} schrijven: {e}", icon.display()))?;

    if !opts.no_shortcuts {
        make_shortcuts(&app, &uninstaller, &root, &icon, opts.desktop)?;
    }
    if !opts.no_registry {
        register_uninstall(&root, &uninstaller)?;
    }

    let bytes = dir_size(&root).unwrap_or(0);
    Ok(Report { root, bytes })
}

/* ----------------------------------------------------------------------- main */

fn print_help() {
    println!(
        "{APP_NAME} {VERSION} — Windows-installer

Gebruik:
  Signage-Studio-Setup.exe                 installeren (met venster)
  Signage-Studio-Setup.exe --silent        installeren zonder vragen
  Signage-Studio-Setup.exe --no-desktop    geen bureaubladsnelkoppeling
  Signage-Studio-Setup.exe --no-shortcuts  geen snelkoppelingen
  Signage-Studio-Setup.exe --no-registry   geen vermelding bij Apps en onderdelen
  Signage-Studio-Setup.exe --dir <pad>     ergens anders installeren
  Signage-Studio-Setup.exe --help          deze uitleg

Standaard installatiemap: %LOCALAPPDATA%\\Programs\\{APP_NAME}
Gegevens (blijven bij verwijderen staan): %LOCALAPPDATA%\\Signage\\data
Verwijderen: Start → {APP_NAME} verwijderen, of Uninstall.exe in de installatiemap.
"
    );
}

fn run(opts: &Opts) -> Result<i32, String> {
    if opts.help {
        print_help();
        return Ok(0);
    }

    let root = opts.dir.clone().unwrap_or_else(default_root);
    let app = root.join(APP_EXE_NAME);

    if !opts.silent {
        let upgrade = app.is_file();
        let question = if upgrade {
            format!(
                "{APP_NAME} {VERSION} bijwerken?\n\nNaar: {}\n\nJe playlists, media en decks in %LOCALAPPDATA%\\Signage\\data blijven staan.",
                root.display()
            )
        } else {
            format!(
                "{APP_NAME} {VERSION} installeren?\n\nNaar: {}\n\nJe gegevens in %LOCALAPPDATA%\\Signage\\data blijven staan. Er komen snelkoppelingen in het startmenu{}.",
                root.display(),
                if opts.desktop {
                    " en op het bureaublad"
                } else {
                    ""
                }
            )
        };
        if !confirm(&question) {
            return Ok(0);
        }
    }

    if app_running() {
        if !opts.silent {
            if !confirm(&format!(
                "{APP_NAME} is geopend. De app wordt gesloten om de installatie bij te werken.\n\nNiet-opgeslagen werk gaat verloren. Doorgaan?"
            )) {
                info("Installatie geannuleerd.");
                return Ok(0);
            }
        }
        stop_app()?;
    }

    let report = install(opts)?;

    if opts.silent {
        println!(
            "geïnstalleerd in {} ({} KB)",
            report.root.display(),
            report.bytes / 1024
        );
    } else {
        let start = confirm(&format!(
            "{APP_NAME} is geïnstalleerd in:\n{}\n\n{} KB.\n\nNu starten?",
            report.root.display(),
            report.bytes / 1024
        ));
        if start {
            open_app(&report.root.join(APP_EXE_NAME));
        }
    }

    Ok(0)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let opts = match Opts::parse(&args) {
        Ok(opts) => opts,
        Err(text) => {
            eprintln!("{text}");
            error(&format!("{text}\n\nGebruik --help voor de opties."));
            std::process::exit(2);
        }
    };

    match run(&opts) {
        Ok(code) => {
            let _ = std::io::stdout().flush();
            std::process::exit(code)
        }
        Err(text) => {
            eprintln!("{text}");
            if !opts.silent {
                error(&text);
            }
            let _ = std::io::stdout().flush();
            std::process::exit(1)
        }
    }
}
