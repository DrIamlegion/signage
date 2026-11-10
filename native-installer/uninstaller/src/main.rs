//! Signage Studio — verwijderaar.
//!
//! Wordt door de installer als `Uninstall.exe` naast de app geplaatst. Verwijdert
//! de app, snelkoppelingen en de vermelding bij *Apps en onderdelen*. De
//! gebruikersgegevens in `%LOCALAPPDATA%\Signage\data` blijven staan.

use std::env;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
};

const APP_NAME: &str = "Signage Studio";
const APP_EXE_NAME: &str = "Signage Studio.exe";
const UNINSTALLER_NAME: &str = "Uninstall.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
struct Opts {
    help: bool,
    silent: bool,
    dir: Option<PathBuf>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut opts = Opts::default();
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

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message(text: &str, style: u32) -> i32 {
    let body = wide(text);
    let caption = wide(APP_NAME);
    unsafe { MessageBoxW(std::ptr::null_mut(), body.as_ptr(), caption.as_ptr(), style) }
}

fn info(text: &str) {
    message(text, MB_OK | MB_ICONINFORMATION);
}

fn error(text: &str) {
    message(text, MB_OK | MB_ICONERROR);
}

fn confirm(text: &str) -> bool {
    message(text, MB_YESNO | MB_ICONQUESTION) == IDYES
}

/* ---------------------------------------------------------------------- zoeken */

fn is_installation(root: &Path) -> bool {
    root.join(APP_EXE_NAME).is_file() || root.join(UNINSTALLER_NAME).is_file()
}

fn registry_root() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_NAME}"
        ))
        .ok()?;
    let value: String = key.get_value("InstallLocation").ok()?;
    let root = PathBuf::from(value);
    if is_installation(&root) {
        Some(root)
    } else {
        None
    }
}

fn find_root(opts: &Opts) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = &opts.dir {
        candidates.push(dir.clone());
    }
    if let Some(dir) = registry_root() {
        candidates.push(dir);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }
    candidates.push(default_root());

    for candidate in candidates {
        if is_installation(&candidate) {
            return Ok(candidate);
        }
    }

    Err(format!(
        "Geen bestaande {APP_NAME}-installatie gevonden.\n\nGebruik --dir <pad> als de app ergens anders staat."
    ))
}

/* -------------------------------------------------------------------- stoppen */

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
            "{APP_NAME} is nog geopend. Sluit de app en probeer het opnieuw."
        ))
    } else {
        Ok(())
    }
}

/* ---------------------------------------------------------------- verwijderen */

fn win_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

fn unregister_uninstall() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.delete_subkey_all(format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_NAME}"
    )) {
        Ok(()) => Ok(()),
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("registersleutel verwijderen: {e}")),
    }
}

fn remove_shortcuts() {
    if let Some(dir) = start_menu_dir() {
        let _ = fs::remove_dir_all(dir);
    }
    if let Some(dir) = desktop_dir() {
        let _ = fs::remove_file(dir.join(format!("{APP_NAME}.lnk")));
    }
    // Snelkoppeling van de eerdere losse/handmatige installatie.
    if let Some(base) = start_menu_root() {
        let _ = fs::remove_file(base.join("Signage").join(format!("{APP_NAME}.lnk")));
    }
}

/// Deze executable draait nog en kan zichzelf niet verwijderen: een klein
/// hulp-script ruimt de installatiemap een paar seconden later op.
fn schedule_removal(root: &Path) {
    let temp = env::temp_dir().join(format!(
        "signage-studio-verwijderen-{}.cmd",
        std::process::id()
    ));
    let uninstaller = root.join("Uninstall.exe");
    let script = format!(
        "@echo off\r\nsetlocal\r\nset tries=0\r\n:wait\r\nping -n 2 127.0.0.1 >nul\r\nrd /s /q \"{}\" 2>nul\r\nif not exist \"{}\" goto done\r\nset /a tries+=1\r\nif %tries% lss 30 goto wait\r\n:done\r\ndel \"%~f0\" >nul 2>nul\r\n",
        win_path(root),
        win_path(&uninstaller)
    );
    if fs::write(&temp, script).is_ok() {
        let _ = Command::new("cmd")
            .arg("/c")
            .arg(&temp)
            .current_dir(env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

fn remove(opts: &Opts) -> Result<String, String> {
    let root = find_root(opts)?;

    if !opts.silent
        && !confirm(&format!(
            "{APP_NAME} verwijderen?\n\nDe app en snelkoppelingen verdwijnen. Je playlists, media en decks in %LOCALAPPDATA%\\Signage\\data blijven staan."
        ))
    {
        return Ok("geannuleerd".to_string());
    }

    stop_app()?;

    remove_shortcuts();
    unregister_uninstall()?;

    let current = env::current_exe().map_err(|e| format!("eigen pad bepalen: {e}"))?;
    let current_canonical = fs::canonicalize(&current).ok();
    let root_canonical = fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let mut self_inside = current_canonical
        .as_ref()
        .map(|path| path.starts_with(&root_canonical))
        .unwrap_or(false);

    for entry in fs::read_dir(&root).map_err(|e| format!("installatiemap lezen: {e}"))? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();

        if current_canonical.is_some() && fs::canonicalize(&path).ok() == current_canonical {
            self_inside = true;
            continue;
        }

        if path.is_dir() {
            let dir_canonical = fs::canonicalize(&path).ok();
            let holds_self = match (&dir_canonical, &current_canonical) {
                (Some(dir), Some(exe)) => exe.starts_with(dir),
                _ => false,
            };
            if holds_self {
                self_inside = true;
                continue;
            }
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }

    if self_inside {
        schedule_removal(&root);
    } else {
        fs::remove_dir(&root).map_err(|e| format!("installatiemap verwijderen: {e}"))?;
    }

    Ok(win_path(&root))
}

fn print_help() {
    println!(
        "{APP_NAME} — verwijderaar

Gebruik:
  Uninstall.exe                 verwijderen (met bevestiging)
  Uninstall.exe --silent        verwijderen zonder vragen
  Uninstall.exe --dir <pad>     een installatie op een afwijkend pad verwijderen
  Uninstall.exe --help          deze uitleg

De gegevens in %LOCALAPPDATA%\\Signage\\data blijven altijd staan.
"
    );
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

    if opts.help {
        print_help();
        return;
    }

    match remove(&opts) {
        Ok(where_) if where_ == "geannuleerd" => {
            if !opts.silent {
                info("Verwijderen geannuleerd.");
            }
        }
        Ok(path) => {
            if opts.silent {
                println!("verwijderd uit {path}");
            } else {
                info(&format!("{APP_NAME} is verwijderd uit:\n{path}"));
            }
        }
        Err(text) => {
            eprintln!("{text}");
            if !opts.silent {
                error(&text);
            }
            std::process::exit(1);
        }
    }
}
