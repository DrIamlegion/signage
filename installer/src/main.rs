//! Signage — Windows-installer.
//!
//! Eén executable die drie dingen doet:
//!   * installeren (bestanden, snelkoppelingen, registersleutel),
//!   * draaien (`--serve`: Node-server starten en de browser openen),
//!   * verwijderen (`--uninstall`).
//!
//! De app zelf (server.js + public/) zit in de executable, dus de setup is
//! zelfstandig: geen zip, geen Node-pakketten, geen administrator-rechten.

use std::env;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use include_dir::{include_dir, Dir};
use mslnk::ShellLink;
use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
use winreg::RegKey;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO,
};

/* ------------------------------------------------------------- ingebakken app */

static PUBLIC: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../public");
const SERVER_JS: &[u8] = include_bytes!("../../server.js");
const README_MD: &[u8] = include_bytes!("../../README.md");

const APP_NAME: &str = "Signage";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PORT: u16 = 8100;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SW_SHOWNORMAL: i32 = 1;
const NODE_DOWNLOAD: &str = "https://nodejs.org/nl/download";

static ICON_ICO: &[u8] = include_bytes!("../assets/Signage.ico");
const ICON_FILE: &str = "Signage.ico";
const FIREWALL_RULE: &str = "Signage-tcp-8100";

/* --------------------------------------------------------------------- opties */

#[derive(Default)]
struct Opts {
    help: bool,
    silent: bool,
    uninstall: bool,
    serve: bool,
    stop: bool,
    no_browser: bool,
    no_shortcuts: bool,
    no_registry: bool,
    remove_data: bool,
    desktop: bool,
    autostart: bool,
    firewall: bool,
    firewall_remove: bool,
    no_firewall: bool,
    dir: Option<PathBuf>,
    port: Option<u16>,
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
                "--uninstall" | "/U" => opts.uninstall = true,
                "--serve" | "/RUN" => opts.serve = true,
                "--stop" | "/STOP" => opts.stop = true,
                "--no-browser" => opts.no_browser = true,
                "--no-shortcuts" => opts.no_shortcuts = true,
                "--no-registry" => opts.no_registry = true,
                "--remove-data" => opts.remove_data = true,
                "--desktop" => opts.desktop = true,
                "--autostart" => opts.autostart = true,
                "--firewall" => opts.firewall = true,
                "--firewall-remove" | "--remove-firewall" => opts.firewall_remove = true,
                "--no-firewall" => opts.no_firewall = true,
                "--dir" => {
                    opts.dir = value(&mut i).map(PathBuf::from);
                    if opts.dir.is_none() {
                        return Err("--dir heeft een pad nodig".into());
                    }
                }
                "--port" => {
                    opts.port = value(&mut i).and_then(|v| v.parse().ok());
                    if opts.port.is_none() {
                        return Err("--port heeft een nummer nodig".into());
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

fn config_dir() -> PathBuf {
    local_appdata().join(APP_NAME)
}

fn data_dir() -> PathBuf {
    config_dir().join("data")
}

fn default_root() -> PathBuf {
    local_appdata().join("Programs").join(APP_NAME)
}

fn exe_root() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn start_menu_dir() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|base| {
        PathBuf::from(base)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(APP_NAME)
    })
}

fn desktop_dir() -> Option<PathBuf> {
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

fn open_url(url: &str) {
    let op = wide("open");
    let file = wide(url);
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

/* ----------------------------------------------------------------------- node */

fn find_node() -> Option<PathBuf> {
    if let Ok(saved) = fs::read_to_string(config_dir().join("node.txt")) {
        let candidate = PathBuf::from(saved.trim());
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let candidate = dir.join("node.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    for guess in [r"C:\Program Files\nodejs\node.exe", r"C:\Program Files (x86)\nodejs\node.exe"] {
        let candidate = PathBuf::from(guess);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn node_version(node: &Path) -> Option<String> {
    let output = Command::new(node)
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/* --------------------------------------------------------------------- poort */

fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(350)).is_ok()
}

fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if port_open(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/* -------------------------------------------------------------- procesbeheer */

fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        CloseHandle(handle);
    }
    true
}

fn terminate(pid: u32) {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 0);
            CloseHandle(handle);
        }
    }
}

fn read_pid() -> Option<u32> {
    fs::read_to_string(config_dir().join("server.pid"))
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

fn write_pid(pid: u32) {
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::write(config_dir().join("server.pid"), pid.to_string());
}

fn clear_pid() {
    let _ = fs::remove_file(config_dir().join("server.pid"));
}

/* ----------------------------------------------------------------- installeren */

struct Report {
    root: PathBuf,
    files: usize,
    bytes: u64,
}

fn write_tree(dir: &Dir, base: &Path, files: &mut usize, bytes: &mut u64) -> Result<(), String> {
    for file in dir.files() {
        let target = base.join(file.path());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("map {} aanmaken: {e}", parent.display()))?;
        }
        fs::write(&target, file.contents()).map_err(|e| format!("{} schrijven: {e}", target.display()))?;
        *files += 1;
        *bytes += file.contents().len() as u64;
    }
    for sub in dir.dirs() {
        write_tree(sub, base, files, bytes)?;
    }
    Ok(())
}

fn install(opts: &Opts) -> Result<Report, String> {
    let root = opts.dir.clone().unwrap_or_else(default_root);
    let app = root.join("app");

    fs::create_dir_all(app.join("public"))
        .map_err(|e| format!("installatiemap {} aanmaken: {e}", app.display()))?;
    fs::create_dir_all(data_dir()).map_err(|e| format!("gegevensmap aanmaken: {e}"))?;

    let mut files = 0;
    let mut bytes = 0;
    write_tree(&PUBLIC, &app.join("public"), &mut files, &mut bytes)?;

    fs::write(app.join("server.js"), SERVER_JS).map_err(|e| format!("server.js schrijven: {e}"))?;
    fs::write(app.join("LEESMIJ.md"), README_MD).map_err(|e| format!("LEESMIJ.md schrijven: {e}"))?;
    files += 2;
    bytes += (SERVER_JS.len() + README_MD.len()) as u64;

    // de launcher is een kopie van deze executable
    let launcher = root.join("Signage.exe");
    let current = env::current_exe().map_err(|e| format!("eigen pad bepalen: {e}"))?;
    if current != launcher {
        fs::copy(&current, &launcher).map_err(|e| format!("launcher plaatsen: {e}"))?;
    }

    if let Some(node) = find_node() {
        let _ = fs::create_dir_all(config_dir());
        let _ = fs::write(config_dir().join("node.txt"), node.display().to_string());
    }

    let icon = root.join(ICON_FILE);
    fs::write(&icon, ICON_ICO).map_err(|e| format!("icoon schrijven: {e}"))?;
    files += 1;
    bytes += ICON_ICO.len() as u64;

    if !opts.no_shortcuts {
        make_shortcuts(&launcher, &root, opts.desktop, &icon)?;
    }
    if !opts.no_registry {
        register_uninstall(&root, &launcher, &app)?;
    } else {
        let _ = unregister_uninstall();
    }
    if opts.autostart {
        register_autostart(&launcher)?;
    } else {
        let _ = unregister_autostart();
    }

    // onthoud waar we geïnstalleerd hebben, ook als de registersleutel ontbreekt
    fs::create_dir_all(config_dir()).map_err(|e| format!("configuratiemap aanmaken: {e}"))?;
    fs::write(config_dir().join("install.txt"), win_path(&root))
        .map_err(|e| format!("installatiepad opslaan: {e}"))?;

    Ok(Report { root, files, bytes })
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
    link.set_name(Some(format!("{APP_NAME} — digitale signage")));
    if let Some(icon) = icon {
        // mslnk zet zelf de index (standaard 0) achter het pad
        link.set_icon_location(Some(win_path(icon)));
    }
    link.create_lnk(path)
        .map_err(|e| format!("{} schrijven: {e}", path.display()))
}

fn make_shortcuts(launcher: &Path, root: &Path, desktop: bool, icon: &Path) -> Result<(), String> {
    if let Some(dir) = start_menu_dir() {
        make_lnk(
            launcher,
            "--serve",
            root,
            Some(icon),
            &dir.join(format!("{APP_NAME}.lnk")),
        )?;
        make_lnk(
            launcher,
            "--uninstall",
            root,
            Some(icon),
            &dir.join(format!("{APP_NAME} verwijderen.lnk")),
        )?;
    }
    if desktop {
        if let Some(dir) = desktop_dir() {
            make_lnk(
                launcher,
                "--serve",
                root,
                Some(icon),
                &dir.join(format!("{APP_NAME}.lnk")),
            )?;
        }
    }
    Ok(())
}

fn register_uninstall(root: &Path, launcher: &Path, app: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(format!(
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{APP_NAME}"
        ))
        .map_err(|e| format!("registersleutel aanmaken: {e}"))?;

    let size_kb: u64 = dir_size(app).unwrap_or(0) / 1024 + 64;
    let exe = win_path(launcher);

    key.set_value("DisplayName", &APP_NAME.to_string()).ok();
    key.set_value("DisplayVersion", &VERSION.to_string()).ok();
    key.set_value("Publisher", &"Signage".to_string()).ok();
    key.set_value("InstallLocation", &win_path(root)).ok();
    let icon = root.join(ICON_FILE);
    let display_icon = if icon.is_file() {
        format!("{},0", win_path(&icon))
    } else {
        exe.clone()
    };
    key.set_value("DisplayIcon", &display_icon).ok();
    key.set_value("UninstallString", &format!("\"{exe}\" --uninstall")).ok();
    key.set_value("QuietUninstallString", &format!("\"{exe}\" --uninstall --silent")).ok();
    key.set_value("EstimatedSize", &(size_kb as u32)).ok();
    key.set_value("NoModify", &1u32).ok();
    key.set_value("NoRepair", &1u32).ok();
    Ok(())
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

fn register_autostart(launcher: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_WRITE)
        .map_err(|e| format!("autostart-sleutel openen: {e}"))?;
    key.set_value(APP_NAME, &format!("\"{}\" --serve --no-browser", win_path(launcher)))
        .map_err(|e| format!("autostart instellen: {e}"))
}

fn unregister_autostart() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_WRITE)
        .map_err(|e| format!("autostart-sleutel openen: {e}"))?;
    match key.delete_value(APP_NAME) {
        Ok(()) => Ok(()),
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("autostart verwijderen: {e}")),
    }
}

/// Deze executable draait nog en kan zichzelf niet verwijderen: een klein
/// hulp-script ruimt de installatiemap een paar seconden later op.
fn schedule_removal(root: &Path) {
    let temp = env::temp_dir().join(format!("signage-verwijderen-{}.cmd", std::process::id()));
    let script = format!(
        "@echo off\r\ntimeout /t 4 /nobreak >nul\r\nrd /s /q \"{}\" 2>nul\r\ndel \"%~f0\" >nul 2>nul\r\n",
        win_path(root)
    );
    if fs::write(&temp, script).is_ok() {
        let _ = Command::new("cmd")
            .arg("/c")
            .arg(&temp)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
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

/* -------------------------------------------------------------------- firewall */

fn is_elevated() -> bool {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

fn elevated_result_path() -> PathBuf {
    env::temp_dir().join("signage-firewall-result.txt")
}

fn wait_for_result(path: &Path, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(text) = fs::read_to_string(path) {
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Start deze executable opnieuw met beheerdersrechten.
fn ask_elevation(arguments: &str) -> Result<(), String> {
    let exe = env::current_exe().map_err(|e| format!("eigen pad bepalen: {e}"))?;
    let verb = wide("runas");
    let file = wide(&win_path(&exe));
    let params = wide(arguments);
    let launched = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (launched as isize) <= 32 {
        return Err("de beheerdersbevestiging is geweigerd".into());
    }
    Ok(())
}

fn netsh(args: &[&str]) -> Result<(), String> {
    let output = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("netsh starten: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let mut detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if detail.is_empty() {
        detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    Err(detail)
}

fn firewall_add() -> Result<(), String> {
    let name = format!("name={FIREWALL_RULE}");
    let port = format!("localport={DEFAULT_PORT}");
    // eerst opruimen zodat opnieuw installeren geen dubbele regel geeft
    let _ = netsh(&["advfirewall", "firewall", "delete", "rule", name.as_str()]);
    netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        name.as_str(),
        "dir=in",
        "action=allow",
        "protocol=TCP",
        port.as_str(),
        "profile=private,domain",
        "enable=yes",
    ])
}

fn firewall_delete() -> Result<(), String> {
    let name = format!("name={FIREWALL_RULE}");
    netsh(&["advfirewall", "firewall", "delete", "rule", name.as_str()])
}

fn firewall(remove: bool, quiet: bool) -> i32 {
    if !is_elevated() {
        let result_path = elevated_result_path();
        let _ = fs::remove_file(&result_path);
        let arguments = if remove {
            "--firewall-remove --silent"
        } else {
            "--firewall --silent"
        };
        let outcome = match ask_elevation(arguments) {
            Ok(()) => match wait_for_result(&result_path, Duration::from_secs(120)) {
                Some(text) if text.trim() == "ok" => Ok(()),
                Some(text) => Err(text.trim().to_string()),
                None => Err("geen antwoord van de beheerderstaak".to_string()),
            },
            Err(message) => Err(message),
        };
        let _ = fs::remove_file(&result_path);
        return report_firewall(outcome, remove, quiet);
    }

    let outcome = if remove { firewall_delete() } else { firewall_add() };
    let _ = fs::write(
        elevated_result_path(),
        match &outcome {
            Ok(()) => "ok".to_string(),
            Err(message) => message.clone(),
        },
    );
    report_firewall(outcome, remove, quiet)
}

fn report_firewall(outcome: Result<(), String>, remove: bool, quiet: bool) -> i32 {
    let action = if remove { "verwijderd" } else { "toegevoegd" };
    match outcome {
        Ok(()) => {
            if !quiet {
                info(&format!("Firewall-regel voor poort {DEFAULT_PORT} is {action}."));
            }
            0
        }
        Err(message) => {
            if !quiet {
                error(&format!("Firewall-regel aanpassen mislukte:\n{message}"));
            } else {
                eprintln!("firewall: {message}");
            }
            1
        }
    }
}

/* -------------------------------------------------------------------- starten */

fn serve(opts: &Opts) -> Result<String, String> {
    let root = opts.dir.clone().unwrap_or_else(exe_root);
    let app = root.join("app");
    if !app.join("server.js").exists() {
        return Err(format!(
            "{APP_NAME} is hier niet geïnstalleerd (app\\server.js ontbreekt in {}).\n\nStart eerst Signage-Setup.exe.",
            root.display()
        ));
    }

    let port = opts.port.unwrap_or(DEFAULT_PORT);
    let data = data_dir();
    fs::create_dir_all(&data).map_err(|e| format!("gegevensmap aanmaken: {e}"))?;

    if let Some(pid) = read_pid() {
        if !process_alive(pid) {
            clear_pid();
        }
    }

    if !port_open(port) {
        let node = find_node().ok_or_else(|| {
            format!(
                "Node.js is niet gevonden.\n\nInstalleer Node.js 18 of nieuwer via {NODE_DOWNLOAD} en start Signage daarna opnieuw."
            )
        })?;

        let child = Command::new(&node)
            .arg("server.js")
            .current_dir(&app)
            .env("SIGNAGE_DATA", &data)
            .env("PORT", port.to_string())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("server starten met {}: {e}", node.display()))?;

        write_pid(child.id());
        if !wait_for_port(port, Duration::from_secs(15)) {
            return Err(format!(
                "De server begon niet te luisteren op poort {port}.\n\nControleer of Node.js correct is geïnstalleerd."
            ));
        }
    }

    let url = format!("http://localhost:{port}/");
    if !opts.no_browser {
        open_url(&url);
    }
    Ok(url)
}

fn stop() -> Result<(), String> {
    match read_pid() {
        Some(pid) if process_alive(pid) => {
            terminate(pid);
            std::thread::sleep(Duration::from_millis(400));
            clear_pid();
            Ok(())
        }
        _ => {
            clear_pid();
            Err("Signage lijkt niet te draaien.".into())
        }
    }
}

/* ---------------------------------------------------------------- verwijderen */

fn is_installation(root: &Path) -> bool {
    root.join("app").join("server.js").is_file()
}

fn recorded_root() -> Option<PathBuf> {
    let raw = fs::read_to_string(config_dir().join("install.txt")).ok()?;
    let root = PathBuf::from(raw.trim());
    if is_installation(&root) {
        Some(root)
    } else {
        None
    }
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

/// Zoekt de echte installatie. De map van de setup zelf is nooit een kandidaat
/// tenzij daar toevallig een volledige installatie in staat.
fn uninstall_root(opts: &Opts) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = &opts.dir {
        candidates.push(dir.clone());
    }
    if let Some(dir) = recorded_root() {
        candidates.push(dir);
    }
    if let Some(dir) = registry_root() {
        candidates.push(dir);
    }
    candidates.push(default_root());
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }

    for candidate in candidates {
        if is_installation(&candidate) {
            return Ok(candidate);
        }
    }

    Err(format!(
        "Geen bestaande {APP_NAME}-installatie gevonden.\n\nDe map van deze setup is niet aangeraakt ({}).\nGebruik --dir <pad> als je Signage ergens anders hebt geïnstalleerd.",
        exe_root().display()
    ))
}

fn uninstall(opts: &Opts) -> Result<String, String> {
    let root = uninstall_root(opts)?;
    let current = env::current_exe().map_err(|e| format!("eigen pad bepalen: {e}"))?;
    let current_canonical = fs::canonicalize(&current).ok();
    let root_canonical = fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let mut setup_inside = current_canonical
        .as_ref()
        .map(|path| path.starts_with(&root_canonical))
        .unwrap_or(false);

    let _ = stop();

    if opts.firewall_remove {
        let _ = firewall(true, true);
    }

    if let Some(dir) = start_menu_dir() {
        let _ = fs::remove_dir_all(&dir);
    }
    if let Some(dir) = desktop_dir() {
        let _ = fs::remove_file(dir.join(format!("{APP_NAME}.lnk")));
    }
    let _ = unregister_uninstall();
    let _ = unregister_autostart();
    let _ = fs::remove_file(config_dir().join("node.txt"));
    let _ = fs::remove_file(config_dir().join("server.pid"));
    let _ = fs::remove_file(config_dir().join("install.txt"));

    if opts.remove_data {
        let _ = fs::remove_dir_all(data_dir());
        let _ = fs::remove_dir_all(config_dir());
    }

    for entry in fs::read_dir(&root).map_err(|e| format!("installatiemap lezen: {e}"))? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();

        // nooit het bestand verwijderen waarmee we nu draaien
        if current_canonical.is_some() && fs::canonicalize(&path).ok() == current_canonical {
            setup_inside = true;
            continue;
        }

        if path.is_dir() {
            let dir_canonical = fs::canonicalize(&path).ok();
            let holds_setup = match (&dir_canonical, &current_canonical) {
                (Some(dir), Some(exe)) => exe.starts_with(dir),
                _ => false,
            };
            if holds_setup {
                setup_inside = true;
                continue;
            }
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }

    if setup_inside {
        return Ok(format!(
            "{} (de map met deze setup is blijven staan)",
            root.display()
        ));
    }

    schedule_removal(&root);
    Ok(root.display().to_string())
}

/* ----------------------------------------------------------------------- main */

fn print_help() {
    println!(
        "{APP_NAME} {VERSION} — Windows-installer

Gebruik:
  Signage-Setup.exe                     installeren (met vensters)
  Signage-Setup.exe --silent            installeren zonder vragen
  Signage-Setup.exe --desktop           ook een bureaubladsnelkoppeling
  Signage-Setup.exe --autostart         server automatisch starten bij inloggen
  Signage-Setup.exe --dir <pad>         ergens anders installeren
  Signage-Setup.exe --serve             server starten en beheer openen
  Signage-Setup.exe --stop              server stoppen
  Signage-Setup.exe --uninstall         verwijderen (gegevens blijven)
  Signage-Setup.exe --uninstall --remove-data --silent
                                        alles verwijderen, inclusief gegevens

  Verwijderen zoekt de bestaande installatie op en ruimt alleen die map op.
  De map waarin deze setup staat (bijvoorbeeld je downloadmap) blijft altijd staan.
  Signage-Setup.exe --help              deze uitleg

  Signage-Setup.exe --firewall          firewall-regel voor poort 8100 toevoegen
  Signage-Setup.exe --firewall-remove   firewall-regel weer verwijderen
  Signage-Setup.exe --no-firewall       installeren zonder firewall-regel

Installatiemap: %LOCALAPPDATA%\\Programs\\{APP_NAME}
Gegevens:       %LOCALAPPDATA%\\{APP_NAME}\\data
Node.js 18 of nieuwer is nodig (https://nodejs.org).
"
    );
}

fn run(opts: &Opts) -> Result<i32, String> {
    if opts.help {
        print_help();
        return Ok(0);
    }

    if opts.firewall || opts.firewall_remove {
        return Ok(firewall(opts.firewall_remove, opts.silent));
    }

    if opts.uninstall {
        if !opts.silent && !confirm(&format!("{APP_NAME} verwijderen?")) {
            return Ok(0);
        }
        let remove_data = opts.remove_data
            || (!opts.silent
                && confirm(&format!(
                    "Ook alle gegevens verwijderen?\n\nPlaylists, media en het beheerwachtwoord staan in:\n{}\n\nNee = de gegevens blijven bewaard.",
                    data_dir().display()
                )));
        let remove_firewall = opts.firewall_remove
            || (!opts.silent
                && confirm(&format!(
                    "Ook de firewall-regel voor poort {DEFAULT_PORT} verwijderen?\n\nWindows vraagt daarvoor beheerdersbevestiging."
                )));
        let effective = Opts {
            remove_data,
            firewall_remove: remove_firewall,
            dir: opts.dir.clone(),
            ..Opts::default()
        };
        let dir = uninstall(&effective)?;
        if !opts.silent {
            info(&format!("{APP_NAME} is verwijderd uit:\n{dir}"));
        }
        return Ok(0);
    }

    if opts.stop {
        let outcome = stop();
        if !opts.silent {
            match &outcome {
                Ok(()) => info("Signage is gestopt."),
                Err(message) => info(message),
            }
        }
        return Ok(if outcome.is_ok() { 0 } else { 1 });
    }

    if opts.serve {
        match serve(opts) {
            Ok(url) => {
                println!("{url}");
                Ok(0)
            }
            Err(message) => {
                if !opts.silent {
                    error(&message);
                }
                Err(message)
            }
        }
    } else {
        if !opts.silent {
            let root = opts.dir.clone().unwrap_or_else(default_root);
            let node_note = match find_node() {
                Some(node) => match node_version(&node) {
                    Some(version) => format!("Node.js gevonden: {version}"),
                    None => "Let op: Node.js reageert niet; controleer de installatie.".to_string(),
                },
                None => format!("Node.js is nog niet gevonden. Nodig: {NODE_DOWNLOAD}"),
            };
            let question = format!(
                "{APP_NAME} {VERSION} installeren?\n\nNaar: {}\nGegevens: {}\n\n{node_note}\n\nEr komen snelkoppelingen in het startmenu; beheerderstoegang is niet nodig.",
                root.display(),
                data_dir().display()
            );
            if !confirm(&question) {
                return Ok(0);
            }
        }

        if find_node().is_none() {
            let note = format!(
                "Node.js is niet gevonden. {APP_NAME} heeft Node.js 18 of nieuwer nodig om te draaien.\n\nWil je de downloadpagina openen? Daarna kun je deze installer opnieuw starten."
            );
            if !opts.silent && confirm(&note) {
                open_url(NODE_DOWNLOAD);
            }
            return Err("Node.js ontbreekt".into());
        }

        let report = install(opts)?;

        let firewall_wanted = if opts.no_firewall {
            false
        } else if opts.silent {
            true
        } else {
            confirm(&format!(
                "Firewall-regel toevoegen zodat andere apparaten op je netwerk Signage op poort {DEFAULT_PORT} kunnen openen?\n\nWindows vraagt daarvoor eenmalig om beheerdersbevestiging."
            ))
        };
        let firewall_status = if !firewall_wanted {
            "niet gevraagd".to_string()
        } else if firewall(false, true) == 0 {
            format!("poort {DEFAULT_PORT} vrijgegeven (privé/domein)")
        } else {
            "niet toegevoegd; later herhalen met --firewall".to_string()
        };
        let firewall_note = if firewall_wanted {
            format!("\n\nFirewall: {firewall_status}.")
        } else {
            String::new()
        };

        if opts.silent {
            println!(
                "geïnstalleerd in {} ({} bestanden, {} KB)",
                report.root.display(),
                report.files,
                report.bytes / 1024
            );
            if firewall_wanted {
                println!("firewall: {firewall_status}");
            }
        } else {
            let start = confirm(&format!(
                "{APP_NAME} is geïnstalleerd in:\n{}\n\n{} bestanden, {} KB.{firewall_note}\n\nNu starten en het beheer in de browser openen?",
                report.root.display(),
                report.files,
                report.bytes / 1024
            ));
            if start {
                match serve(opts) {
                    Ok(url) => info(&format!(
                        "Signage draait op {url}\n\nHet beheerwachtwoord staat in:\n{}",
                        data_dir().join("admin-password.txt").display()
                    )),
                    Err(message) => error(&message),
                }
            }
        }
        Ok(0)
    }
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
            let _ = std::io::stdout().flush();
            std::process::exit(1)
        }
    }
}
