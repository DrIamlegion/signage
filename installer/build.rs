//! Compileert app.rc (icoon + versie-informatie) tot een Windows-resource en
//! koppelt die aan de executable. Zonder rc.exe bouwt de installer gewoon door,
//! alleen dan zonder icoon in de exe.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    if let Ok(path) = env::var("RC") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(found) = which_rc_on_path() {
        return Some(found);
    }

    let kits = env::var("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files (x86)"))
        .join("Windows Kits")
        .join("10")
        .join("bin");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&kits) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("x64").join("rc.exe");
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort();
    candidates.pop()
}

fn which_rc_on_path() -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join("rc.exe");
        if candidate.is_file() && candidate.to_string_lossy().contains("Windows Kits") {
            return Some(candidate);
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/Signage.ico");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let resource = out_dir.join("signage.res");

    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe niet gevonden: de executable krijgt geen icoon");
        return;
    };

    let status = Command::new(&rc)
        .current_dir(&manifest)
        .args([
            "/nologo",
            "/fo",
            &resource.display().to_string(),
            "app.rc",
        ])
        .status();

    match status {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg={}", resource.display());
        }
        _ => println!("cargo:warning=rc.exe kon app.rc niet compileren; executable zonder icoon"),
    }
}
