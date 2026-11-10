#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod deckcraft_audio;
mod deckcraft_pasteboard;

use eframe::egui::{
    self, Align, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Key, Layout, Pos2,
    Rect, Response, RichText, Sense, Shape, Stroke, Vec2, ViewportCommand,
};
use serde_json::{json, Value};

const ACCENT: Color32 = Color32::from_rgb(0xC4, 0x3E, 0x1C);
const INK: Color32 = Color32::from_rgb(0x37, 0x37, 0x37);
const MUTED: Color32 = Color32::from_rgb(0x6E, 0x6E, 0x6E);
const FAINT: Color32 = Color32::from_rgb(0x9A, 0x9A, 0x9A);
const RIBBON: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
const CHROME: Color32 = Color32::from_rgb(0xF3, 0xF2, 0xF1);
const BENCH: Color32 = Color32::from_rgb(0xF3, 0xF2, 0xF1);
const LINE: Color32 = Color32::from_rgb(0xE2, 0xE0, 0xDD);

const THEMES: [(&str, &str, &str, &str); 6] = [
    ("Wit", "#ffffff", "#262626", "#c43e1c"),
    ("Nacht", "#0b1c3a", "#f8fafc", "#5b8cff"),
    ("Papier", "#f5f1e8", "#2b2b2b", "#b45309"),
    ("Oceaan", "#0e7490", "#f0f9ff", "#facc15"),
    ("Bos", "#14532d", "#f0fdf4", "#a3e635"),
    ("Zonsondergang", "#7c2d12", "#fff7ed", "#fb923c"),
];

const LAYOUTS: [(&str, &str); 7] = [
    ("title", "Titel"),
    ("text", "Titel en tekst"),
    ("image-left", "Afbeelding links"),
    ("image-right", "Afbeelding rechts"),
    ("full", "Afbeelding volledig"),
    ("quote", "Citaat"),
    ("free", "Vrij canvas"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    File,
    DeckCraft,
    Home,
    Insert,
    Design,
    Transition,
    Anim,
    Show,
    View,
    Screens,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::File => "Bestand",
            Tab::DeckCraft => "DeckCraft",
            Tab::Home => "Start",
            Tab::Insert => "Invoegen",
            Tab::Design => "Ontwerp",
            Tab::Transition => "Overgangen",
            Tab::Anim => "Animaties",
            Tab::Show => "Diavoorstelling",
            Tab::View => "Beeld",
            Tab::Screens => "Schermen",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    NewSlide,
    Duplicate,
    Delete,
    Layout,
    Text,
    Image,
    Shape,
    Save,
    Open,
    Reload,
    Close,
    Bold,
    Italic,
    Underline,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Play,
    Palette,
    Transition,
    Anim,
    Zoom,
    Panel,
    Line,
    Back,
    Front,
    Link,
}

#[derive(Clone)]
struct Form {
    slide_id: String,
    deck_name: String,
    title: String,
    subtitle: String,
    body: String,
    footer: String,
    notes: String,
    bg: String,
    fg: String,
    accent: String,
    layout: String,
    font: String,
    transition: String,
    duration: f32,
    hidden: bool,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            slide_id: String::new(),
            deck_name: String::new(),
            title: String::new(),
            subtitle: String::new(),
            body: String::new(),
            footer: String::new(),
            notes: String::new(),
            bg: "#ffffff".into(),
            fg: "#262626".into(),
            accent: "#c43e1c".into(),
            layout: "title".into(),
            font: "sans".into(),
            transition: String::new(),
            duration: 8.0,
            hidden: false,
        }
    }
}

struct App {
    path: PathBuf,
    db: Value,
    deck: usize,
    slide: usize,
    tab: Tab,
    form: Form,
    form_dirty: bool,
    sel_el: Option<usize>,
    el_form: Value,
    el_dirty: bool,
    dirty: bool,
    status: String,
    zoom: f32,
    show_inspector: bool,
    show_db: bool,
    path_input: String,
    presenting: bool,
    present_idx: usize,
    drag_off: Option<(usize, Vec2)>,
    shape_kind: String,
    anim_kind: String,
    transition_kind: String,
    font_kind: String,
    screen_sel: Option<usize>,
    server_url: String,
    pick_deck: usize,
    pick_media: usize,
    new_url: String,
    new_url_duration: f32,
    confirm_screen_delete: bool,
    deckcraft: deckcraft_ui_egui::SlideApp,
}

fn hex(value: &str) -> Color32 {
    let t = value.trim().trim_start_matches('#');
    if t.len() >= 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&t[0..2], 16),
            u8::from_str_radix(&t[2..4], 16),
            u8::from_str_radix(&t[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    Color32::from_rgb(0x0F, 0x17, 0x2A)
}

fn hex_or(value: &str, fallback: Color32) -> Color32 {
    if value.trim().len() == 7 && value.trim().starts_with('#') {
        hex(value)
    } else {
        fallback
    }
}

fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

fn get_str(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn get_f32(v: &Value, key: &str, fallback: f32) -> f32 {
    v.get(key).and_then(Value::as_f64).map(|n| n as f32).unwrap_or(fallback)
}

fn get_bool(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn new_id() -> String {
    format!("{:x}", now_ms() as u32 ^ std::process::id().wrapping_mul(2_654_435_761))
}

fn aspect_ratio(name: &str) -> f32 {
    match name {
        "4:3" => 4.0 / 3.0,
        "9:16" => 9.0 / 16.0,
        "1:1" => 1.0,
        _ => 16.0 / 9.0,
    }
}

fn default_slide() -> Value {
    json!({
        "id": new_id(),
        "layout": "title",
        "title": "",
        "subtitle": "",
        "body": "",
        "footer": "",
        "bg": "#ffffff",
        "bg2": null,
        "bgAngle": 135,
        "fg": "#262626",
        "accent": "#c43e1c",
        "imageId": null,
        "shade": 0.45,
        "align": "center",
        "scale": 1,
        "duration": 8,
        "notes": "",
        "font": "sans",
        "transition": "",
        "hidden": false,
        "elements": []
    })
}

fn default_deck(name: &str) -> Value {
    json!({
        "id": new_id(),
        "name": name,
        "aspect": "16:9",
        "duration": 8,
        "transition": "fade",
        "slides": [default_slide()],
        "updatedAt": now_ms(),
    })
}

const DAYS: [(&str, u64); 7] = [
    ("ma", 1),
    ("di", 2),
    ("wo", 3),
    ("do", 4),
    ("vr", 5),
    ("za", 6),
    ("zo", 0),
];

fn random_key() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let state = std::collections::hash_map::RandomState::new();
    let mut out = String::with_capacity(32);
    for round in 0..4u64 {
        let mut hasher = state.build_hasher();
        hasher.write_u128(now_ms());
        hasher.write_u32(std::process::id());
        hasher.write_u64(round);
        for byte in hasher.finish().to_le_bytes() {
            out.push(ALPHABET[(byte & 63) as usize] as char);
        }
    }
    out
}

fn random_id(len: usize) -> String {
    random_key().chars().take(len).collect()
}

fn write_db_atomic(path: &Path, db: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(db).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, path)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Rendert de open DeckCraft-presentatie uit het geheugen naar PNG's en zet die als
/// dia's in de Signage-database. Geen extern programma: dezelfde renderer als de editor.
fn publish_presentation(
    db: &mut Value,
    db_path: &Path,
    state: &deckcraft_engine::DocState,
    deck_path: &Path,
) -> Result<String, String> {
    let count = state.doc.slides.len();
    if count == 0 {
        return Err("DeckCraft-presentatie heeft geen dia's".into());
    }

    let data_dir = db_path.parent().ok_or_else(|| "database heeft geen map".to_string())?;
    let media_dir = data_dir.join("media");
    fs::create_dir_all(&media_dir).map_err(|e| format!("mediamap aanmaken: {e}"))?;

    let mut rendered = Vec::with_capacity(count);
    for index in 0..count {
        let (png, width, height) =
            deckcraft_engine::cmd::file::render_png_with(&state.doc, index, 2.0, false, false);
        if png.is_empty() {
            return Err(format!("dia {} renderen gaf geen beeld", index + 1));
        }
        rendered.push((png, width, height));
    }

    let file_name = deck_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "presentatie.pptx".to_string());
    let stem = deck_path
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "DeckCraft".to_string());
    let mut deck_name = stem.replace('-', " ").replace('_', " ");
    deck_name = deck_name.trim().to_string();
    if deck_name.is_empty() {
        deck_name = "DeckCraft-presentatie".to_string();
    }

    if !db.get("media").is_some_and(Value::is_array) {
        db["media"] = json!([]);
    }
    if !db.get("presentations").is_some_and(Value::is_array) {
        db["presentations"] = json!([]);
    }

    let old_files: Vec<String> = db["media"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|item| get_str(item, "deckcraftDeck") == file_name)
                .filter_map(|item| item.get("file").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for file in &old_files {
        let _ = fs::remove_file(media_dir.join(file));
    }
    if let Some(list) = db["media"].as_array_mut() {
        list.retain(|item| get_str(item, "deckcraftDeck") != file_name);
    }

    let (width, height) = rendered
        .first()
        .map(|(_, width, height)| (*width, *height))
        .unwrap_or((1920, 1080));
    let ratio = width as f32 / height.max(1) as f32;
    let aspect = if (ratio - 4.0 / 3.0).abs() < 0.02 {
        "4:3"
    } else if (ratio - 9.0 / 16.0).abs() < 0.02 {
        "9:16"
    } else if (ratio - 1.0).abs() < 0.02 {
        "1:1"
    } else {
        "16:9"
    };

    let mut slides = Vec::with_capacity(rendered.len());
    let mut new_media = Vec::with_capacity(rendered.len());
    for (index, (png, _, _)) in rendered.iter().enumerate() {
        let id = random_id(8);
        let file = format!("{id}.png");
        let target = media_dir.join(&file);
        fs::write(&target, png).map_err(|e| format!("dia {} opslaan: {e}", index + 1))?;
        new_media.push(json!({
            "id": id.clone(),
            "name": format!("{} — dia {}", deck_name, index + 1),
            "file": file,
            "ext": ".png",
            "kind": "image",
            "mime": "image/png",
            "size": png.len() as u64,
            "uploadedAt": now_ms() as u64,
            "deckcraftDeck": file_name.clone()
        }));
        slides.push(json!({
            "id": random_id(8),
            "layout": "full",
            "title": "",
            "subtitle": "",
            "body": "",
            "footer": "",
            "bg": "#000000",
            "bg2": null,
            "bgAngle": 135,
            "fg": "#ffffff",
            "accent": "#c43e1c",
            "imageId": id,
            "shade": 0.35,
            "align": "center",
            "scale": 1,
            "duration": 8,
            "notes": "",
            "font": "sans",
            "transition": "",
            "hidden": false,
            "elements": []
        }));
    }
    if let Some(list) = db["media"].as_array_mut() {
        list.extend(new_media);
    }

    let existing = db["presentations"].as_array().and_then(|list| {
        list.iter()
            .position(|deck| get_str(deck, "deckcraftFile") == file_name)
    });
    let deck_id = existing
        .map(|index| get_str(&db["presentations"][index], "id"))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| random_id(8));

    let deck = json!({
        "id": deck_id,
        "name": deck_name,
        "aspect": aspect,
        "duration": 8,
        "transition": "fade",
        "slides": slides,
        "updatedAt": now_ms() as u64,
        "source": "deckcraft",
        "deckcraftFile": file_name
    });
    if let Some(index) = existing {
        db["presentations"][index] = deck;
    } else if let Some(list) = db["presentations"].as_array_mut() {
        list.push(deck);
    }

    let now = now_ms() as u64;
    if let Some(screens) = db["screens"].as_array_mut() {
        for screen in screens {
            screen["updatedAt"] = json!(now);
        }
    }
    Ok(format!("{} dia's gepubliceerd", count))
}

fn deckcraft_services() -> deckcraft_ui_egui::Services {
    let mut services = deckcraft_ui_egui::Services::default();
    services.pick_open = Some(Box::new(|purpose: &str| {
        let dialog = rfd::FileDialog::new();
        let dialog = match purpose {
            "picture" => dialog.add_filter(
                "Afbeeldingen",
                &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"],
            ),
            "audio" => dialog.add_filter(
                "Audio",
                &["wav", "mp3", "m4a", "m4b", "aac", "flac", "ogg", "oga", "opus"],
            ),
            "video" => dialog.add_filter("Video", &["mp4", "m4v", "mov", "webm", "mkv", "wmv"]),
            "theme" => dialog.add_filter("Thema's en presentaties", &["thmx", "pptx", "potx", "ppsx"]),
            _ => dialog
                .add_filter("Presentaties", &["deckcraft", "pptx", "potx", "ppsx"])
                .add_filter("DeckCraft", &["deckcraft"])
                .add_filter("PowerPoint", &["pptx", "potx", "ppsx"]),
        };
        dialog.pick_file().map(|path| path.to_string_lossy().to_string())
    }));
    services.pick_save = Some(Box::new(|name: &str| {
        rfd::FileDialog::new()
            .set_file_name(name)
            .save_file()
            .map(|path| path.to_string_lossy().to_string())
    }));
    services.read = Some(Box::new(|path: &str| fs::read(path).map_err(|e| e.to_string())));
    services.write = Some(Box::new(|path: &str, bytes: &[u8]| {
        fs::write(path, bytes).map_err(|e| e.to_string())
    }));
    services.clipboard_image = Some(Box::new(deckcraft_pasteboard::image_png));
    services.clipboard_html = Some(Box::new(deckcraft_pasteboard::html));
    services.clipboard_set_html = Some(Box::new(deckcraft_pasteboard::set_html));
    services.audio_out = Some(Box::new(deckcraft_audio::CpalOut::default()));
    services
}

fn default_screen(name: &str) -> Value {
    json!({
        "id": random_id(8),
        "name": name,
        "key": random_key(),
        "items": [],
        "fit": "contain",
        "paused": false,
        "createdAt": now_ms(),
        "updatedAt": now_ms(),
        "lastSeen": 0
    })
}

fn valid_time(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 5 || b[2] != b':' {
        return false;
    }
    if !b[0].is_ascii_digit()
        || !b[1].is_ascii_digit()
        || !b[3].is_ascii_digit()
        || !b[4].is_ascii_digit()
    {
        return false;
    }
    let hour = (b[0] - b'0') * 10 + (b[1] - b'0');
    let minute = (b[3] - b'0') * 10 + (b[4] - b'0');
    hour < 24 && minute < 60
}

fn seen_label(screen: &Value) -> String {
    let last = screen.get("lastSeen").and_then(Value::as_i64).unwrap_or(0);
    if last <= 0 {
        return "nooit verbonden".into();
    }
    let seconds = (now_ms() as i64 - last).max(0) / 1000;
    if seconds < 90 {
        "online".into()
    } else if seconds < 3600 {
        format!("offline · {} min", (seconds / 60).max(1))
    } else if seconds < 86_400 {
        format!("offline · {} u", seconds / 3600)
    } else {
        "offline".into()
    }
}

fn find_name(list: &[(String, String)], id: &str) -> String {
    list.iter()
        .find(|(item_id, _)| item_id == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| "(verwijderd)".into())
}

fn ellipsize(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn ago_label(modified: u64) -> String {
    let seconds = (now_ms() as u64).saturating_sub(modified) / 1000;
    if seconds < 60 {
        "net gewijzigd".into()
    } else if seconds < 3600 {
        format!("{} min geleden", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} u geleden", seconds / 3600)
    } else {
        format!("{} d geleden", seconds / 86_400)
    }
}

fn default_db() -> Value {
    json!({
        "screens": [],
        "media": [],
        "presentations": [default_deck("Nieuwe presentatie")],
        "settings": { "defaultDuration": 10, "title": "Signage" },
    })
}

fn db_has_screens(path: &std::path::Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("screens").and_then(Value::as_array).map(|a| !a.is_empty()))
        .unwrap_or(false)
}

fn default_db_path() -> PathBuf {
    if let Ok(p) = env::var("SIGNAGE_DB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    // Een geconfigureerde installatie (met schermen) is de live zender en
    // krijgt voorrang. Is die nog leeg, dan blijft de projectdatabase actief.
    let installed = env::var("LOCALAPPDATA")
        .ok()
        .map(|local| PathBuf::from(local).join("Signage").join("data").join("db.json"));
    if let Some(path) = &installed {
        if path.exists() && db_has_screens(path) {
            return path.clone();
        }
    }
    for candidate in ["data/db.json", "../data/db.json", "../../data/db.json"] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return path;
        }
    }
    if let Some(path) = installed {
        if path.exists() {
            return path;
        }
    }
    PathBuf::from("data/db.json")
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut add_font = |key: &str, file: &str, family: FontFamily| {
        if let Ok(bytes) = fs::read(format!("C:/Windows/Fonts/{file}")) {
            fonts.font_data.insert(
                key.to_string(),
                std::sync::Arc::new(FontData::from_owned(bytes)),
            );
            let mut list = vec![key.to_string()];
            list.extend(fallback.iter().cloned());
            fonts.families.insert(family, list);
        }
    };
    add_font("segoe", "segoeui.ttf", FontFamily::Proportional);
    add_font("georgia", "georgia.ttf", FontFamily::Name("serif".into()));
    add_font("consola", "consola.ttf", FontFamily::Name("mono".into()));
    add_font("black", "ariblk.ttf", FontFamily::Name("display".into()));
    ctx.set_fonts(fonts);
}

fn family_for(key: &str) -> FontFamily {
    match key {
        "serif" => FontFamily::Name("serif".into()),
        "mono" => FontFamily::Name("mono".into()),
        "display" => FontFamily::Name("display".into()),
        _ => FontFamily::Proportional,
    }
}

fn layout_label(key: &str) -> &'static str {
    LAYOUTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| *label)
        .unwrap_or("Titel")
}

fn transition_label(key: &str) -> &'static str {
    match key {
        "none" => "Geen",
        "push" => "Duwen",
        "wipe" => "Vegen",
        "zoom" => "Zoomen",
        _ => "Vervagen",
    }
}

fn font_label(key: &str) -> &'static str {
    match key {
        "serif" => "Serif",
        "mono" => "Mono",
        "display" => "Display",
        _ => "Sans",
    }
}

fn anim_label(key: &str) -> &'static str {
    match key {
        "fade" => "Vervagen",
        "up" => "Omhoog",
        "zoom" => "Zoomen",
        _ => "Geen",
    }
}

fn fit_rect(outer: Rect, ratio: f32) -> Rect {
    let mut w = outer.width();
    let mut h = w / ratio;
    if h > outer.height() {
        h = outer.height();
        w = h * ratio;
    }
    Rect::from_center_size(outer.center(), Vec2::new(w, h))
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        deckcraft_ui_egui::install_ime_pass_hooks(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());


        let path = default_db_path();
        let mut app = Self {
            path: path.clone(),
            db: default_db(),
            deck: 0,
            slide: 0,
            tab: Tab::DeckCraft,
            form: Form::default(),
            form_dirty: false,
            sel_el: None,
            el_form: Value::Null,
            el_dirty: false,
            dirty: false,
            status: String::new(),
            zoom: 1.0,
            show_inspector: true,
            show_db: false,
            path_input: path.display().to_string(),
            presenting: false,
            present_idx: 0,
            drag_off: None,
            shape_kind: "round".into(),
            anim_kind: "fade".into(),
            transition_kind: "fade".into(),
            font_kind: "sans".into(),
            screen_sel: None,
            server_url: env::var("SIGNAGE_URL")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "http://localhost:8100".into()),
            pick_deck: 0,
            pick_media: 0,
            new_url: String::new(),
            new_url_duration: 8.0,
            confirm_screen_delete: false,
            deckcraft: deckcraft_ui_egui::SlideApp::new(
                deckcraft_engine::Session::new(),
                deckcraft_services(),
            ),
        };
        app.load(path);
        if let Some(deck) = startup_deck() {
            if let Err(err) = app.deckcraft.session.execute(
                "file.open",
                &json!({"path": deck.to_string_lossy().to_string()}),
            ) {
                app.status = format!("Deck openen mislukt: {err}");
            }
        }
        app
    }

    fn decks(&self) -> &[Value] {
        self.db
            .get("presentations")
            .and_then(Value::as_array)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn deck(&self) -> Option<&Value> {
        self.decks().get(self.deck)
    }

    fn current_slide(&self) -> Option<&Value> {
        self.deck()?
            .get("slides")?
            .as_array()?
            .get(self.slide)
    }

    fn slides_len(&self) -> usize {
        self.deck()
            .and_then(|d| d.get("slides"))
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0)
    }

    fn deck_name(&self) -> String {
        self.deck().map(|d| get_str(d, "name")).unwrap_or_else(|| "Presentatie".into())
    }

    fn slide_id(&self) -> String {
        self.current_slide().map(|s| get_str(s, "id")).unwrap_or_default()
    }

    fn load(&mut self, path: PathBuf) {
        if !path.exists() {
            self.db = default_db();
            self.path = path.clone();
            self.normalize();
            self.dirty = false;
            self.status = "Nieuwe database aangemaakt".into();
            self.save_now();
            self.deck = 0;
            self.slide = 0;
            self.sync_form();
            return;
        }
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(mut db) => {
                    if !db.is_object() {
                        self.status = "Database is geen object".into();
                        return;
                    }
                    if db.get("presentations").is_none() {
                        db["presentations"] = json!([default_deck("Nieuwe presentatie")]);
                    }
                    if db.get("screens").is_none() {
                        db["screens"] = json!([]);
                    }
                    if db.get("media").is_none() {
                        db["media"] = json!([]);
                    }
                    self.db = db;
                    self.path = path.clone();
                    self.path_input = path.display().to_string();
                    self.normalize();
                    self.dirty = false;
                    self.status = format!("Geopend · {}", path.display());
                    self.deck = 0;
                    self.slide = 0;
                    self.sync_form();
                }
                Err(err) => {
                    self.status = format!("Kon database niet lezen: {err}");
                }
            },
            Err(err) => {
                self.status = format!("Kon bestand niet openen: {err}");
            }
        }
    }

    fn normalize(&mut self) {
        if !self.db.get("presentations").map(Value::is_array).unwrap_or(false) {
            self.db["presentations"] = json!([default_deck("Nieuwe presentatie")]);
        }
        if let Some(decks) = self.db.get_mut("presentations").and_then(Value::as_array_mut) {
            if decks.is_empty() {
                decks.push(default_deck("Nieuwe presentatie"));
            }
            for deck in decks.iter_mut() {
                if !deck.is_object() {
                    *deck = default_deck("Nieuwe presentatie");
                    continue;
                }
                let empty = deck
                    .get("slides")
                    .and_then(Value::as_array)
                    .map(|s| s.is_empty())
                    .unwrap_or(true);
                if empty {
                    deck["slides"] = json!([default_slide()]);
                }
                if deck.get("name").and_then(Value::as_str).unwrap_or("").trim().is_empty() {
                    deck["name"] = json!("Presentatie");
                }
            }
        }
        let max_deck = self.decks().len().saturating_sub(1);
        if self.deck > max_deck {
            self.deck = max_deck;
        }
        let max_slide = self.slides_len().saturating_sub(1);
        if self.slide > max_slide {
            self.slide = max_slide;
        }
        self.normalize_screens();
    }

    fn normalize_screens(&mut self) {
        if !self.db.get("screens").map(Value::is_array).unwrap_or(false) {
            self.db["screens"] = json!([]);
        }
        if let Some(list) = self.db.get_mut("screens").and_then(Value::as_array_mut) {
            for screen in list.iter_mut() {
                if !screen.is_object() {
                    *screen = default_screen("Scherm");
                    continue;
                }
                if get_str(screen, "id").is_empty() {
                    screen["id"] = json!(random_id(8));
                }
                if get_str(screen, "name").trim().is_empty() {
                    screen["name"] = json!("Scherm");
                }
                if get_str(screen, "key").is_empty() {
                    screen["key"] = json!(random_key());
                }
                if !screen.get("items").map(Value::is_array).unwrap_or(false) {
                    screen["items"] = json!([]);
                }
                if get_str(screen, "fit")
                    != "cover"
                    && get_str(screen, "fit") != "contain"
                {
                    screen["fit"] = json!("contain");
                }
                if screen.get("paused").is_none() {
                    screen["paused"] = json!(false);
                }
            }
        }
        if self.screens().is_empty() {
            self.screen_sel = None;
        } else {
            let max = self.screens().len() - 1;
            self.screen_sel = Some(self.screen_sel.unwrap_or(0).min(max));
        }
    }

    fn sync_form(&mut self) {
        let deck = self.deck().cloned().unwrap_or(Value::Null);
        let slide = self.current_slide().cloned().unwrap_or(Value::Null);
        let bg = get_str(&slide, "bg");
        let fg = get_str(&slide, "fg");
        let accent = get_str(&slide, "accent");
        self.form = Form {
            slide_id: get_str(&slide, "id"),
            deck_name: get_str(&deck, "name"),
            title: get_str(&slide, "title"),
            subtitle: get_str(&slide, "subtitle"),
            body: get_str(&slide, "body"),
            footer: get_str(&slide, "footer"),
            notes: get_str(&slide, "notes"),
            bg: if bg.is_empty() { "#ffffff".into() } else { bg },
            fg: if fg.is_empty() { "#262626".into() } else { fg },
            accent: if accent.is_empty() { "#c43e1c".into() } else { accent },
            layout: {
                let l = get_str(&slide, "layout");
                if l.is_empty() { "title".into() } else { l }
            },
            font: {
                let f = get_str(&slide, "font");
                if f.is_empty() { "sans".into() } else { f }
            },
            transition: get_str(&slide, "transition"),
            duration: get_f32(&slide, "duration", 8.0),
            hidden: get_bool(&slide, "hidden"),
        };
        self.form_dirty = false;
        self.sel_el = None;
        self.el_form = Value::Null;
        self.el_dirty = false;
        self.shape_kind = "round".into();
        self.anim_kind = "fade".into();
        self.transition_kind = if self.form.transition.is_empty() {
            "fade".into()
        } else {
            self.form.transition.clone()
        };
        self.font_kind = self.form.font.clone();
    }

    fn commit_form(&mut self) {
        if !self.form_dirty {
            return;
        }
        if self.form.slide_id.is_empty() || self.form.slide_id != self.slide_id() {
            self.form_dirty = false;
            return;
        }
        let f = self.form.clone();
        let deck_index = self.deck;
        let slide_index = self.slide;
        if let Some(deck) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
        {
            if !f.deck_name.trim().is_empty() {
                deck["name"] = json!(f.deck_name.trim());
            }
            deck["updatedAt"] = json!(now_ms() as u64);
            if let Some(slides) = deck.get_mut("slides").and_then(Value::as_array_mut) {
                if let Some(slide) = slides.get_mut(slide_index) {
                    if !slide.is_object() {
                        *slide = default_slide();
                    }
                    slide["title"] = json!(f.title);
                    slide["subtitle"] = json!(f.subtitle);
                    slide["body"] = json!(f.body);
                    slide["footer"] = json!(f.footer);
                    slide["notes"] = json!(f.notes);
                    slide["bg"] = json!(norm_hex(&f.bg, "#ffffff"));
                    slide["fg"] = json!(norm_hex(&f.fg, "#262626"));
                    slide["accent"] = json!(norm_hex(&f.accent, "#c43e1c"));
                    slide["layout"] = json!(f.layout);
                    slide["font"] = json!(f.font);
                    slide["transition"] = json!(f.transition);
                    slide["duration"] = json!(f.duration.clamp(1.0, 600.0).round() as i64);
                    slide["hidden"] = json!(f.hidden);
                }
            }
        }
        self.form_dirty = false;
        self.dirty = true;
    }

    fn commit_el(&mut self) {
        if !self.el_dirty {
            return;
        }
        self.el_dirty = false;
        let Some(index) = self.sel_el else { return };
        if !self.el_form.is_object() {
            return;
        }
        let deck_index = self.deck;
        let slide_index = self.slide;
        let updated = self.el_form.clone();
        if let Some(slide) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(|s| s.get_mut(slide_index))
        {
            if let Some(elements) = slide.get_mut("elements").and_then(Value::as_array_mut) {
                if index < elements.len() {
                    elements[index] = updated;
                    self.dirty = true;
                }
            }
        }
    }

    fn save_now(&mut self) {
        self.commit_form();
        self.commit_el();
        let text = match serde_json::to_string_pretty(&self.db) {
            Ok(text) => text,
            Err(err) => {
                self.status = format!("Kon niet opslaan: {err}");
                return;
            }
        };
        // atomaire opslag: de zender-watcher leest nooit een half bestand
        let tmp = self.path.with_extension("json.tmp");
        let result = fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, &self.path));
        match result {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Opgeslagen en live · {}", self.path.display());
            }
            Err(err) => {
                let _ = fs::remove_file(&tmp);
                self.status = format!("Opslaan mislukt: {err}");
            }
        }
    }

    fn select_deck(&mut self, index: usize) {
        if index == self.deck && !self.decks().is_empty() {
            return;
        }
        self.commit_form();
        self.commit_el();
        self.deck = index;
        self.slide = 0;
        self.sync_form();
    }

    fn select_slide(&mut self, index: usize) {
        if index == self.slide {
            return;
        }
        self.commit_form();
        self.commit_el();
        self.slide = index;
        self.sync_form();
    }

    fn new_deck(&mut self) {
        self.commit_form();
        self.commit_el();
        let count = self.decks().len() + 1;
        let mut deck = default_deck(&format!("Presentatie {count}"));
        deck["slides"] = json!([default_slide()]);
        if let Some(decks) = self.db.get_mut("presentations").and_then(Value::as_array_mut) {
            decks.push(deck);
            self.deck = decks.len() - 1;
        }
        self.slide = 0;
        self.sync_form();
        self.dirty = true;
        self.status = "Nieuwe presentatie".into();
    }

    fn add_slide(&mut self) {
        self.commit_form();
        let (bg, fg, accent, font) = (
            self.form.bg.clone(),
            self.form.fg.clone(),
            self.form.accent.clone(),
            self.form.font.clone(),
        );
        let mut slide = default_slide();
        slide["bg"] = json!(bg);
        slide["fg"] = json!(fg);
        slide["accent"] = json!(accent);
        slide["font"] = json!(font);
        let deck_index = self.deck;
        let mut new_index = 0;
        if let Some(slides) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(Value::as_array_mut)
        {
            slides.push(slide);
            new_index = slides.len() - 1;
        }
        self.slide = new_index;
        self.sync_form();
        self.dirty = true;
    }

    fn duplicate_slide(&mut self) {
        self.commit_form();
        let deck_index = self.deck;
        let slide_index = self.slide;
        let mut new_index = slide_index;
        if let Some(slides) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(Value::as_array_mut)
        {
            if let Some(current) = slides.get(slide_index).cloned() {
                let mut copy = current;
                copy["id"] = json!(new_id());
                slides.insert(slide_index + 1, copy);
                new_index = slide_index + 1;
            }
        }
        self.slide = new_index;
        self.sync_form();
        self.dirty = true;
    }

    fn delete_slide(&mut self) {
        self.commit_form();
        let deck_index = self.deck;
        let slide_index = self.slide;
        let mut next = slide_index;
        if let Some(slides) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(Value::as_array_mut)
        {
            if slides.len() > 1 {
                slides.remove(slide_index);
                next = slide_index.min(slides.len() - 1);
            } else if let Some(slide) = slides.get_mut(0) {
                *slide = default_slide();
            }
        }
        self.slide = next;
        self.sync_form();
        self.dirty = true;
    }

    fn apply_theme(&mut self, bg: &str, fg: &str, accent: &str) {
        self.form.bg = bg.into();
        self.form.fg = fg.into();
        self.form.accent = accent.into();
        self.form_dirty = true;
        self.dirty = true;
    }

    fn set_layout(&mut self, layout: &str) {
        if layout == "free" && self.form.layout != "free" {
            self.convert_to_free();
        }
        self.form.layout = layout.into();
        self.form_dirty = true;
        self.dirty = true;
    }

    fn convert_to_free(&mut self) {
        let mut elements: Vec<Value> = Vec::new();
        let fg = self.form.fg.clone();
        if !self.form.title.trim().is_empty() {
            elements.push(json!({
                "id": new_id(), "type": "text", "x": 8, "y": 8, "w": 84, "h": 20,
                "text": self.form.title, "size": 8, "weight": "bold", "align": "center",
                "valign": "center", "color": fg, "font": self.form.font, "bullet": false
            }));
        }
        if !self.form.subtitle.trim().is_empty() {
            elements.push(json!({
                "id": new_id(), "type": "text", "x": 12, "y": 30, "w": 76, "h": 12,
                "text": self.form.subtitle, "size": 4.2, "align": "center", "valign": "center",
                "color": fg, "font": self.form.font
            }));
        }
        if !self.form.body.trim().is_empty() {
            elements.push(json!({
                "id": new_id(), "type": "text", "x": 10, "y": 44, "w": 80, "h": 40,
                "text": self.form.body, "size": 3.6, "align": "left", "valign": "top",
                "bullet": true, "color": fg, "font": self.form.font
            }));
        }
        if !self.form.footer.trim().is_empty() {
            elements.push(json!({
                "id": new_id(), "type": "text", "x": 10, "y": 90, "w": 80, "h": 6,
                "text": self.form.footer, "size": 2.2, "align": "center", "valign": "center",
                "color": fg, "font": self.form.font
            }));
        }
        let deck_index = self.deck;
        let slide_index = self.slide;
        if let Some(slide) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(|s| s.get_mut(slide_index))
        {
            slide["elements"] = Value::Array(elements);
        }
        self.form.title.clear();
        self.form.subtitle.clear();
        self.form.body.clear();
        self.form.footer.clear();
        self.form_dirty = true;
    }

    fn add_element(&mut self, kind: &str, shape: &str) {
        self.commit_form();
        if self.form.layout != "free" {
            self.set_layout("free");
        }
        let fg = self.form.fg.clone();
        let accent = self.form.accent.clone();
        let element = if kind == "shape" {
            json!({
                "id": new_id(), "type": "shape", "shape": shape,
                "x": 34, "y": 30, "w": 32, "h": 32,
                "fill": if shape == "line" { "transparent" } else { &accent },
                "stroke": if shape == "line" { &fg } else { "transparent" },
                "strokeWidth": if shape == "line" { 0.6 } else { 0.0 },
                "radius": 10, "opacity": 1
            })
        } else if kind == "image" {
            json!({
                "id": new_id(), "type": "image", "x": 30, "y": 24, "w": 40, "h": 46,
                "imageId": null, "fit": "cover"
            })
        } else {
            json!({
                "id": new_id(), "type": "text", "x": 12, "y": 38, "w": 64, "h": 20,
                "text": "Nieuwe tekst", "size": 5, "weight": "normal", "align": "left",
                "valign": "center", "color": fg, "font": self.form.font, "bullet": false
            })
        };
        let deck_index = self.deck;
        let slide_index = self.slide;
        let mut new_index = 0;
        if let Some(slide) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(|s| s.get_mut(slide_index))
        {
            if !slide.get("elements").map(Value::is_array).unwrap_or(false) {
                slide["elements"] = json!([]);
            }
            if let Some(elements) = slide.get_mut("elements").and_then(Value::as_array_mut) {
                elements.push(element.clone());
                new_index = elements.len() - 1;
            }
        }
        self.form.layout = "free".into();
        self.sel_el = Some(new_index);
        self.el_form = element;
        self.el_dirty = false;
        self.dirty = true;
    }

    fn delete_element(&mut self) {
        let Some(index) = self.sel_el else { return };
        let deck_index = self.deck;
        let slide_index = self.slide;
        if let Some(slide) = self
            .db
            .get_mut("presentations")
            .and_then(|v| v.get_mut(deck_index))
            .and_then(|d| d.get_mut("slides"))
            .and_then(|s| s.get_mut(slide_index))
        {
            if let Some(elements) = slide.get_mut("elements").and_then(Value::as_array_mut) {
                if index < elements.len() {
                    elements.remove(index);
                }
            }
        }
        self.sel_el = None;
        self.el_form = Value::Null;
        self.el_dirty = false;
        self.dirty = true;
    }

    fn select_element(&mut self, index: usize) {
        if let Some(element) = self
            .current_slide()
            .and_then(|s| s.get("elements"))
            .and_then(Value::as_array)
            .and_then(|a| a.get(index))
            .cloned()
        {
            self.sel_el = Some(index);
            self.el_form = element;
            self.el_dirty = false;
            self.anim_kind = {
                let a = get_str(&self.el_form, "anim");
                if a.is_empty() { "fade".into() } else { a }
            };
        }
    }

    fn move_element(&mut self, index: usize, x: f32, y: f32) {
        let deck_index = self.deck;
        let slide_index = self.slide;
        let updated = {
            let Some(slide) = self
                .db
                .get_mut("presentations")
                .and_then(|v| v.get_mut(deck_index))
                .and_then(|d| d.get_mut("slides"))
                .and_then(|s| s.get_mut(slide_index))
            else {
                return;
            };
            let Some(elements) = slide.get_mut("elements").and_then(Value::as_array_mut) else {
                return;
            };
            let Some(element) = elements.get_mut(index) else {
                return;
            };
            let w = get_f32(element, "w", 30.0);
            let h = get_f32(element, "h", 10.0);
            let x = x.clamp(0.0, (100.0 - w).max(0.0));
            let y = y.clamp(0.0, (100.0 - h).max(0.0));
            element["x"] = json!(x.round());
            element["y"] = json!(y.round());
            element.clone()
        };
        self.el_form = updated;
        self.dirty = true;
    }

    fn start_present(&mut self, ctx: &egui::Context, from_beginning: bool) {
        self.commit_form();
        self.present_idx = if from_beginning { 0 } else { self.slide };
        self.presenting = true;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
    }

    fn stop_present(&mut self, ctx: &egui::Context) {
        self.presenting = false;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
    }

    fn visible_slides(&self) -> Vec<Value> {
        self.deck()
            .and_then(|d| d.get("slides"))
            .and_then(Value::as_array)
            .map(|slides| {
                slides
                    .iter()
                    .filter(|s| !get_bool(s, "hidden"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn present_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let slides = self.visible_slides();
        if slides.is_empty() {
            self.presenting = false;
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.stop_present(&ctx);
            return;
        }
        let forward = ctx.input(|i| {
            i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::Space) || i.key_pressed(Key::Enter)
        });
        let backward = ctx.input(|i| i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::Backspace));
        if forward {
            if self.present_idx + 1 < slides.len() {
                self.present_idx += 1;
            }
        }
        if backward {
            self.present_idx = self.present_idx.saturating_sub(1);
        }
        let clicked = ctx.input(|i| i.pointer.primary_clicked());
        if clicked {
            if self.present_idx + 1 < slides.len() {
                self.present_idx += 1;
            }
        }
        let index = self.present_idx.min(slides.len() - 1);
        let slide = slides[index].clone();
        let ratio = self
            .deck()
            .map(|d| aspect_ratio(&get_str(d, "aspect")))
            .unwrap_or(16.0 / 9.0);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ui, |ui| {
                let rect = fit_rect(ui.max_rect().shrink(24.0), ratio);
                draw_slide(ui.painter(), rect, &slide, false, None);
            });
        ctx.request_repaint();
    }

    /* ------------------------------------------------------------- chrome */

    fn tabs_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        let mut publish_clicked = false;
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let mut clicked = None;
            for tab in [Tab::File, Tab::DeckCraft, Tab::Screens] {
                if tab_button(ui, tab.label(), self.tab == tab).clicked() {
                    clicked = Some(tab);
                }
            }
            if let Some(tab) = clicked {
                self.tab = tab;
            }
            if self.tab == Tab::DeckCraft {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(14.0);
                    let has_doc = self.deckcraft.session.active().is_some();
                    let publish = egui::Button::new(
                        RichText::new("Publiceren")
                            .size(11.5)
                            .color(Color32::WHITE),
                    )
                    .fill(ACCENT);
                    if ui.add_enabled(has_doc, publish).clicked() {
                        publish_clicked = true;
                    }
                    let title = self
                        .deckcraft
                        .session
                        .active()
                        .map(|doc| doc.title())
                        .unwrap_or_else(|| "Geen presentatie".into());
                    ui.label(
                        RichText::new(format!("DeckCraft · ingebouwd · {title}"))
                            .size(11.5)
                            .color(MUTED),
                    );
                });
            } else {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(14.0);
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(118.0, 26.0), Sense::click());
                    let color = if resp.hovered() {
                        Color32::from_rgb(0xA8, 0x33, 0x16)
                    } else {
                        ACCENT
                    };
                    ui.painter().rect_filled(rect, 3.0, color);
                    let tri = vec![
                        Pos2::new(rect.left() + 18.0, rect.center().y - 5.0),
                        Pos2::new(rect.left() + 18.0, rect.center().y + 5.0),
                        Pos2::new(rect.left() + 26.0, rect.center().y),
                    ];
                    ui.painter()
                        .add(Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
                    ui.painter().text(
                        Pos2::new(rect.left() + 33.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        "Presenteren",
                        FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    if resp.clicked() && self.tab != Tab::Screens {
                        ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(true));
                        self.present_idx = self.slide;
                        self.presenting = true;
                    }
                    ui.label(
                        RichText::new(self.deck_name())
                            .size(11.5)
                            .color(MUTED),
                    );
                });
            }
        });
        let rect = ui.max_rect();
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, LINE),
        );
        if publish_clicked {
            self.publish_current_deck();
        }
    }

    fn ribbon_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            match self.tab {
                Tab::File => self.ribbon_file(ui),
                Tab::DeckCraft => {}
                Tab::Home => self.ribbon_home(ui),
                Tab::Insert => self.ribbon_insert(ui),
                Tab::Design => self.ribbon_design(ui),
                Tab::Transition => self.ribbon_transition(ui),
                Tab::Anim => self.ribbon_anim(ui),
                Tab::Show => self.ribbon_show(ui),
                Tab::View => self.ribbon_view(ui),
                Tab::Screens => self.ribbon_screens(ui),
            }
        });
    }

    fn ribbon_file(&mut self, ui: &mut egui::Ui) {
        if rbtn(ui, Icon::Save, "Opslaan", true).clicked() {
            self.save_now();
        }
        if rbtn(ui, Icon::Reload, "Herladen", true).clicked() {
            let path = self.path.clone();
            self.load(path);
        }
        if rbtn(ui, Icon::Open, "Database", true).clicked() {
            self.path_input = self.path.display().to_string();
            self.show_db = true;
        }
        if rbtn(ui, Icon::Close, "Afsluiten", true).clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Signage Studio").size(15.0).strong().color(INK));
            ui.label(
                RichText::new(self.path.display().to_string())
                    .size(10.5)
                    .color(MUTED),
            );
            ui.label(
                RichText::new("DeckCraft is de editor; deze app beheert schermen en publicatie.")
                    .size(10.5)
                    .color(FAINT),
            );
        });
    }

    fn ribbon_home(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let names: Vec<String> = self.decks().iter().map(|d| get_str(d, "name")).collect();
        ui.vertical(|ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("Presentatie").size(10.5).color(MUTED));
            egui::ComboBox::from_id_salt("deck-pick")
                .width(150.0)
                .selected_text(self.deck_name())
                .show_ui(ui, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        if ui.selectable_label(i == self.deck, name).clicked() {
                            picked = Some(i);
                        }
                    }
                });
        });
        if let Some(index) = picked {
            self.select_deck(index);
        }
        if rbtn(ui, Icon::NewSlide, "Nieuw", true).clicked() {
            self.new_deck();
        }
        ui.separator();
        if rbtn(ui, Icon::NewSlide, "Nieuwe dia", true).clicked() {
            self.add_slide();
        }
        if rbtn(ui, Icon::Duplicate, "Dupliceren", true).clicked() {
            self.duplicate_slide();
        }
        if rbtn(ui, Icon::Delete, "Verwijderen", self.slides_len() > 0).clicked() {
            self.delete_slide();
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("Lay-out").size(10.5).color(MUTED));
            let mut choice = self.form.layout.clone();
            let before = choice.clone();
            egui::ComboBox::from_id_salt("layout-pick")
                .width(132.0)
                .selected_text(layout_label(&choice))
                .show_ui(ui, |ui| {
                    for (key, label) in LAYOUTS {
                        ui.selectable_value(&mut choice, key.to_string(), label);
                    }
                });
            if choice != before {
                self.set_layout(&choice);
            }
        });
        ui.separator();
        let has_el = self.sel_el.is_some();                        if has_el && rbtn(ui, Icon::Bold, "Vet", true).clicked() {
                            toggle_el(&mut self.el_form, "weight", "bold", "normal");
                            self.el_dirty = true;
                            self.dirty = true;
                        }
                        if !has_el {
                            rbtn(ui, Icon::Bold, "Vet", false);
                        }
                        if has_el && rbtn(ui, Icon::Italic, "Cursief", true).clicked() {
                            toggle_el_bool(&mut self.el_form, "italic");
                            self.el_dirty = true;
                            self.dirty = true;
                        }
                        if !has_el {
                            rbtn(ui, Icon::Italic, "Cursief", false);
                        }
                        if has_el && rbtn(ui, Icon::Underline, "Onderstreept", true).clicked() {
                            toggle_el_bool(&mut self.el_form, "underline");
                            self.el_dirty = true;
                            self.dirty = true;
                        }
                        if !has_el {
                            rbtn(ui, Icon::Underline, "Onderstreept", false);
                        }
        ui.separator();
        if rbtn(ui, Icon::Text, "Tekstvak", true).clicked() {
            self.add_element("text", "rect");
        }
        if rbtn(ui, Icon::Shape, "Vorm", true).clicked() {
            self.add_element("shape", "round");
        }                        if has_el && rbtn(ui, Icon::Back, "Naar achter", true).clicked() {
            if let Some(index) = self.sel_el {
                if index > 0 {
                    move_layer(&mut self.db, self.deck, self.slide, index, index - 1);
                    self.sel_el = Some(index - 1);
                    self.dirty = true;
                }
            }
        }                        if has_el && rbtn(ui, Icon::Front, "Naar voren", true).clicked() {
            if let Some(index) = self.sel_el {
                move_layer(&mut self.db, self.deck, self.slide, index, index + 1);
                self.sel_el = Some(index + 1);
                self.dirty = true;
            }
        }
    }

    fn ribbon_insert(&mut self, ui: &mut egui::Ui) {
        if rbtn(ui, Icon::Text, "Tekstvak", true).clicked() {
            self.add_element("text", "rect");
        }
        if rbtn(ui, Icon::Image, "Afbeelding", true).clicked() {
            self.add_element("image", "rect");
        }
        ui.vertical(|ui| {
            ui.add_space(5.0);
            ui.label(RichText::new("Vorm").size(10.5).color(MUTED));
            egui::ComboBox::from_id_salt("shape-pick")
                .width(112.0)
                .selected_text(match self.shape_kind.as_str() {
                    "rect" => "Rechthoek",
                    "ellipse" => "Ellips",
                    "triangle" => "Driehoek",
                    "line" => "Lijn",
                    "arrow" => "Pijl",
                    "star" => "Ster",
                    _ => "Afgerond",
                })
                .show_ui(ui, |ui| {
                    for (key, label) in [
                        ("rect", "Rechthoek"),
                        ("round", "Afgerond"),
                        ("ellipse", "Ellips"),
                        ("triangle", "Driehoek"),
                        ("line", "Lijn"),
                        ("arrow", "Pijl"),
                        ("star", "Ster"),
                    ] {
                        ui.selectable_value(&mut self.shape_kind, key.to_string(), label);
                    }
                });
            if ui.button("Plaatsen").clicked() {
                let kind = self.shape_kind.clone();
                self.add_element("shape", &kind);
            }
        });
        ui.separator();
        if rbtn(ui, Icon::Line, "Lijn", true).clicked() {
            self.add_element("shape", "line");
        }
        ui.separator();
        if rbtn(ui, Icon::NewSlide, "Nieuwe dia", true).clicked() {
            self.add_slide();
        }
    }

    fn ribbon_design(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Thema").size(10.5).color(MUTED));
            ui.horizontal(|ui| {
                for (name, bg, fg, accent) in THEMES {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::click());
                    ui.painter().rect_filled(rect, 3.0, hex(bg));
                    ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            Pos2::new(rect.left() + 4.0, rect.bottom() - 9.0),
                            Vec2::new(26.0, 5.0),
                        ),
                        1.0,
                        hex(accent),
                    );
                    if resp.hovered() {
                        ui.painter().rect_stroke(rect, 3.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
                    }
                    if resp.clicked() {
                        self.apply_theme(bg, fg, accent);
                    }
                    resp.on_hover_text(name);
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Achtergrond").size(10.5).color(MUTED));
            if color_field(ui, "Achtergrond", &mut self.form.bg) {
                self.form_dirty = true;
            }
            if color_field(ui, "Tekst", &mut self.form.fg) {
                self.form_dirty = true;
            }
            if color_field(ui, "Accent", &mut self.form.accent) {
                self.form_dirty = true;
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Lettertype").size(10.5).color(MUTED));
            let mut choice = self.form.font.clone();
            let before = choice.clone();
            egui::ComboBox::from_id_salt("font-pick")
                .width(110.0)
                .selected_text(font_label(&choice))
                .show_ui(ui, |ui| {
                    for key in ["sans", "serif", "mono", "display"] {
                        ui.selectable_value(&mut choice, key.to_string(), font_label(key));
                    }
                });
            if choice != before {
                self.form.font = choice;
                self.form_dirty = true;
            }
        });
    }

    fn ribbon_transition(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Overgang").size(10.5).color(MUTED));
            let mut choice = if self.form.transition.is_empty() {
                "none".to_string()
            } else {
                self.form.transition.clone()
            };
            let before = choice.clone();
            egui::ComboBox::from_id_salt("transition-pick")
                .width(126.0)
                .selected_text(transition_label(&choice))
                .show_ui(ui, |ui| {
                    for key in ["none", "fade", "push", "wipe", "zoom"] {
                        ui.selectable_value(&mut choice, key.to_string(), transition_label(key));
                    }
                });
            if choice != before {
                self.form.transition = if choice == "none" { String::new() } else { choice };
                self.form_dirty = true;
            }
        });
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Duur (seconden)").size(10.5).color(MUTED));
            let mut duration = self.form.duration;
            if ui
                .add(egui::DragValue::new(&mut duration).range(1.0..=600.0).speed(0.5))
                .changed()
            {
                self.form.duration = duration;
                self.form_dirty = true;
            }
        });
        ui.separator();
        if rbtn(ui, Icon::Transition, "Op alle dia's", true).clicked() {
            let transition = self.form.transition.clone();
            let duration = self.form.duration;
            let deck_index = self.deck;
            if let Some(slides) = self
                .db
                .get_mut("presentations")
                .and_then(|v| v.get_mut(deck_index))
                .and_then(|d| d.get_mut("slides"))
                .and_then(Value::as_array_mut)
            {
                for slide in slides.iter_mut() {
                    slide["transition"] = json!(transition);
                    slide["duration"] = json!(duration.round() as i64);
                }
            }
            self.dirty = true;
            self.status = "Overgang op alle dia's toegepast".into();
        }
    }

    fn ribbon_anim(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.sel_el else {
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Selecteer een element op de dia").size(12.0).color(MUTED));
                ui.label(
                    RichText::new("Kies een tekstvak of vorm en stel hier de animatie in.")
                        .size(10.5)
                        .color(FAINT),
                );
            });
            return;
        };
        if self.el_form.is_null() {
            self.select_element(index);
        }
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Animatie").size(10.5).color(MUTED));
            let mut choice = self.anim_kind.clone();
            let before = choice.clone();
            egui::ComboBox::from_id_salt("anim-pick")
                .width(118.0)
                .selected_text(anim_label(&choice))
                .show_ui(ui, |ui| {
                    for key in ["none", "fade", "up", "zoom"] {
                        ui.selectable_value(&mut choice, key.to_string(), anim_label(key));
                    }
                });
            if choice != before {
                self.anim_kind = choice.clone();
                self.el_form["anim"] = json!(choice);
                self.el_dirty = true;
                self.dirty = true;
            }
        });
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Vertraging (s)").size(10.5).color(MUTED));
            let mut delay = get_f32(&self.el_form, "delay", 0.0);
            if ui
                .add(egui::DragValue::new(&mut delay).range(0.0..=20.0).speed(0.1))
                .changed()
            {
                self.el_form["delay"] = json!(delay);
                self.el_dirty = true;
                self.dirty = true;
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Element").size(10.5).color(MUTED));
            let kind = get_str(&self.el_form, "type");
            ui.label(
                RichText::new(match kind.as_str() {
                    "shape" => "Vorm",
                    "image" => "Afbeelding",
                    _ => "Tekstvak",
                })
                .size(12.0)
                .color(INK),
            );
            if ui.button("Verwijderen").clicked() {
                self.delete_element();
            }
        });
    }

    fn ribbon_show(&mut self, ui: &mut egui::Ui) {
        if rbtn(ui, Icon::Play, "Vanaf begin", true).clicked() {
            self.present_idx = 0;
            self.presenting = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
        if rbtn(ui, Icon::Play, "Vanaf huidige", true).clicked() {
            self.present_idx = self.slide;
            self.presenting = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Toetsen").size(10.5).color(MUTED));
            ui.label(RichText::new("→ / spatie: volgende   ← : vorige   Esc: stoppen").size(10.5).color(FAINT));
        });
    }

    fn ribbon_view(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Zoom").size(10.5).color(MUTED));
            ui.add(egui::Slider::new(&mut self.zoom, 0.5..=1.3).show_value(true));
        });
        if rbtn(ui, Icon::Zoom, "Passend", true).clicked() {
            self.zoom = 1.0;
        }
        ui.separator();
        if rbtn(ui, Icon::Panel, "Opmaak", true).clicked() {
            self.show_inspector = !self.show_inspector;
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Dia").size(10.5).color(MUTED));
            ui.label(
                RichText::new(format!("{} van {}", self.slide + 1, self.slides_len()))
                    .size(12.0)
                    .color(INK),
            );
            let deck_aspect = self
                .deck()
                .map(|d| get_str(d, "aspect"))
                .unwrap_or_else(|| "16:9".into());
            ui.label(RichText::new(deck_aspect).size(10.5).color(FAINT));
        });
    }

    /* --------------------------------------------------------------- rail */

    fn rail_ui(&mut self, ui: &mut egui::Ui) {
        let ratio = self
            .deck()
            .map(|d| aspect_ratio(&get_str(d, "aspect")))
            .unwrap_or(16.0 / 9.0);
        let slides: Vec<Value> = self
            .deck()
            .and_then(|d| d.get("slides"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut picked = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(10.0);
                for (index, slide) in slides.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (num_rect, _) =
                            ui.allocate_exact_size(Vec2::new(16.0, 82.0), Sense::hover());
                        ui.painter().text(
                            num_rect.center(),
                            Align2::CENTER_CENTER,
                            format!("{}", index + 1),
                            FontId::proportional(10.0),
                            MUTED,
                        );
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::new(134.0, 82.0), Sense::click());
                        let inner = fit_rect(rect.shrink(4.0), ratio);
                        if resp.hovered() {
                            ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(0xE9, 0xE7, 0xE5));
                        }
                        ui.painter().rect_filled(
                            inner.expand(1.0),
                            0.0,
                            Color32::from_rgb(0xD8, 0xD6, 0xD3),
                        );
                        draw_slide(ui.painter(), inner, slide, false, None);
                        if index == self.slide {
                            ui.painter()
                                .rect_stroke(rect, 2.0, Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
                        } else {
                            ui.painter().rect_stroke(
                                rect,
                                2.0,
                                Stroke::new(1.0, Color32::from_rgb(0xDE, 0xDC, 0xD9)), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            picked = Some(index);
                        }
                    });
                    ui.add_space(3.0);
                }
                ui.add_space(10.0);
            });
        if let Some(index) = picked {
            self.select_slide(index);
        }
        let rect = ui.max_rect();
        ui.painter().line_segment(
            [Pos2::new(rect.right(), rect.top()), Pos2::new(rect.right(), rect.bottom())],
            Stroke::new(1.0, LINE),
        );
    }

    /* -------------------------------------------------------------- notes */

    fn notes_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("Notities").size(10.5).color(MUTED));
        });
        let width = (ui.available_width() - 16.0).max(100.0);
        let resp = ui.add_sized(
            Vec2::new(width, 42.0),
            egui::TextEdit::multiline(&mut self.form.notes)
                .desired_rows(2)
                .frame(egui::Frame::NONE)
                .hint_text("Notities voor de presentator"),
        );
        if resp.changed() {
            self.form_dirty = true;
        }
    }

    /* ---------------------------------------------------------- inspector */

    fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        let mut close = false;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("Opmaak").size(15.0).strong().color(INK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.small_button("×").clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(4.0);
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.set_width(272.0);
                        section(ui, "Presentatie");
                        if text_field(ui, "Naam", &mut self.form.deck_name, false) {
                            self.form_dirty = true;
                        }
                        section(ui, "Dia");
                        let mut layout = self.form.layout.clone();
                        let before = layout.clone();
                        egui::ComboBox::from_id_salt("insp-layout")
                            .width(240.0)
                            .selected_text(layout_label(&layout))
                            .show_ui(ui, |ui| {
                                for (key, label) in LAYOUTS {
                                    ui.selectable_value(&mut layout, key.to_string(), label);
                                }
                            });
                        if layout != before {
                            self.set_layout(&layout);
                        }
                        ui.add_space(4.0);
                        if text_field(ui, "Titel", &mut self.form.title, true) {
                            self.form_dirty = true;
                        }
                        if text_field(ui, "Ondertitel", &mut self.form.subtitle, true) {
                            self.form_dirty = true;
                        }
                        if text_field(ui, "Tekst (regels = opsomming)", &mut self.form.body, true) {
                            self.form_dirty = true;
                        }
                        if text_field(ui, "Voettekst", &mut self.form.footer, false) {
                            self.form_dirty = true;
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Duur").size(11.0).color(MUTED));
                            let mut duration = self.form.duration;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut duration)
                                        .range(1.0..=600.0)
                                        .suffix(" s"),
                                )
                                .changed()
                            {
                                self.form.duration = duration;
                                self.form_dirty = true;
                            }
                            if ui.checkbox(&mut self.form.hidden, "Verborgen").changed() {
                                self.form_dirty = true;
                            }
                        });
                        ui.add_space(4.0);
                        section(ui, "Kleuren");
                        if color_field(ui, "Achtergrond", &mut self.form.bg) {
                            self.form_dirty = true;
                        }
                        if color_field(ui, "Tekst", &mut self.form.fg) {
                            self.form_dirty = true;
                        }
                        if color_field(ui, "Accent", &mut self.form.accent) {
                            self.form_dirty = true;
                        }
                        if let Some(index) = self.sel_el {
                            ui.add_space(8.0);
                            ui.separator();
                            section(ui, "Element");
                            if self.el_form.is_null() {
                                self.select_element(index);
                            }
                            self.element_inspector(ui);
                        }
                        ui.add_space(12.0);
                    });
                });
            });
        let rect = ui.max_rect();
        ui.painter().line_segment(
            [Pos2::new(rect.left(), rect.top()), Pos2::new(rect.left(), rect.bottom())],
            Stroke::new(1.0, LINE),
        );
        if close {
            self.show_inspector = false;
        }
    }

    fn element_inspector(&mut self, ui: &mut egui::Ui) {
        if !self.el_form.is_object() {
            return;
        }
        let kind = get_str(&self.el_form, "type");
        ui.label(
            RichText::new(match kind.as_str() {
                "shape" => "Vorm",
                "image" => "Afbeelding",
                _ => "Tekstvak",
            })
            .size(12.0)
            .strong()
            .color(INK),
        );
        ui.add_space(3.0);
        if kind == "shape" {
            let mut shape = get_str(&self.el_form, "shape");
            let before = shape.clone();
            egui::ComboBox::from_id_salt("el-shape")
                .width(240.0)
                .selected_text(match shape.as_str() {
                    "rect" => "Rechthoek",
                    "ellipse" => "Ellips",
                    "triangle" => "Driehoek",
                    "line" => "Lijn",
                    "arrow" => "Pijl",
                    "star" => "Ster",
                    _ => "Afgerond",
                })
                .show_ui(ui, |ui| {
                    for (key, label) in [
                        ("rect", "Rechthoek"),
                        ("round", "Afgerond"),
                        ("ellipse", "Ellips"),
                        ("triangle", "Driehoek"),
                        ("line", "Lijn"),
                        ("arrow", "Pijl"),
                        ("star", "Ster"),
                    ] {
                        ui.selectable_value(&mut shape, key.to_string(), label);
                    }
                });
            if shape != before {
                self.el_form["shape"] = json!(shape);
                self.el_dirty = true;
            }
            if el_color_field(ui, "Vulling", &mut self.el_form, "fill") {
                self.el_dirty = true;
            }
            if el_color_field(ui, "Rand", &mut self.el_form, "stroke") {
                self.el_dirty = true;
            }
        } else if kind != "image" {
            if el_text_field(ui, "Tekst", &mut self.el_form, "text") {
                self.el_dirty = true;
            }
            ui.horizontal(|ui| {
                let mut size = get_f32(&self.el_form, "size", 5.0);
                ui.label(RichText::new("Grootte").size(11.0).color(MUTED));
                if ui
                    .add(egui::DragValue::new(&mut size).range(1.0..=40.0).speed(0.2))
                    .changed()
                {
                    self.el_form["size"] = json!(size);
                    self.el_dirty = true;
                }
            });
            if el_color_field(ui, "Kleur", &mut self.el_form, "color") {
                self.el_dirty = true;
            }
            ui.horizontal(|ui| {
                let mut bold = get_str(&self.el_form, "weight") == "bold";
                if ui.checkbox(&mut bold, "Vet").changed() {
                    self.el_form["weight"] = json!(if bold { "bold" } else { "normal" });
                    self.el_dirty = true;
                }
                let mut italic = get_bool(&self.el_form, "italic");
                if ui.checkbox(&mut italic, "Cursief").changed() {
                    self.el_form["italic"] = json!(italic);
                    self.el_dirty = true;
                }
                let mut bullet = get_bool(&self.el_form, "bullet");
                if ui.checkbox(&mut bullet, "Opsomming").changed() {
                    self.el_form["bullet"] = json!(bullet);
                    self.el_dirty = true;
                }
            });
            let mut align = get_str(&self.el_form, "align");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Uitlijnen").size(11.0).color(MUTED));
                for key in ["left", "center", "right"] {
                    if ui.selectable_label(align == key, key).clicked() {
                        align = key.to_string();
                    }
                }
            });
            if get_str(&self.el_form, "align") != align {
                self.el_form["align"] = json!(align);
                self.el_dirty = true;
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            for key in ["x", "y", "w", "h"] {
                let mut value = get_f32(&self.el_form, key, 10.0);
                ui.label(RichText::new(key.to_uppercase()).size(10.0).color(MUTED));
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .range(0.0..=100.0)
                            .speed(0.5),
                    )
                    .changed()
                {
                    self.el_form[key] = json!(value);
                    self.el_dirty = true;
                }
            }
        });
        ui.add_space(4.0);
        if ui.button("Element verwijderen").clicked() {
            self.delete_element();
        }
    }

    /* ------------------------------------------------------------- canvas */

    fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_rect_before_wrap();
        let slide = self.current_slide().cloned().unwrap_or(Value::Null);
        let ratio = self
            .deck()
            .map(|d| aspect_ratio(&get_str(d, "aspect")))
            .unwrap_or(16.0 / 9.0);
        let base = fit_rect(avail.shrink2(Vec2::new(36.0, 30.0)), ratio);
        let size = base.size() * self.zoom;
        let rect = Rect::from_center_size(avail.center(), size);

        let painter = ui.painter();
        painter.rect_filled(
            rect.translate(Vec2::new(1.0, 3.0)).expand(1.0),
            1.0,
            Color32::from_black_alpha(22),
        );
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        draw_slide(painter, rect, &slide, true, self.sel_el);
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(0xCF, 0xCD, 0xCA)), egui::StrokeKind::Inside);

        let resp = ui.interact(
            rect,
            ui.id().with("slide-canvas"),
            Sense::click_and_drag(),
        );
        if (resp.clicked() || resp.drag_started()) && !self.presenting {
            if let Some(pos) = resp.interact_pointer_pos() {
                let hit = hit_test(&slide, rect, pos);
                match hit {
                    Some(index) => {
                        self.select_element(index);
                        let el_rect = element_rect(&slide, rect, index);
                        self.drag_off = Some((index, pos - el_rect.min));
                    }
                    None => {
                        self.sel_el = None;
                        self.el_form = Value::Null;
                        self.drag_off = None;
                    }
                }
            }
        }
        if resp.dragged() {
            if let (Some((index, offset)), Some(pos)) = (self.drag_off, resp.interact_pointer_pos()) {
                let local = pos - offset - rect.min;
                let x = local.x / rect.width() * 100.0;
                let y = local.y / rect.height() * 100.0;
                self.move_element(index, x, y);
            }
        }
        if resp.drag_stopped() {
            self.drag_off = None;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        ui.allocate_space(avail.size());
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let save = ctx.input(|i| {
            (i.modifiers.command || i.modifiers.ctrl || i.modifiers.mac_cmd)
                && (i.key_pressed(Key::S)
                    || i.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Key {
                                key: Key::S,
                                pressed: true,
                                ..
                            }
                        )
                    }))
        });
        if save {
            self.save_now();
        }
        if typing {
            return;
        }
        if self.tab == Tab::Screens || self.tab == Tab::DeckCraft {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.sel_el = None;
            self.el_form = Value::Null;
        }
        let next = ctx.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowRight));
        let prev = ctx.input(|i| i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::ArrowLeft));
        if next && self.slide + 1 < self.slides_len() {
            self.select_slide(self.slide + 1);
        }
        if prev && self.slide > 0 {
            self.select_slide(self.slide - 1);
        }
        if ctx.input(|i| i.key_pressed(Key::F5)) {
            self.start_present(ctx, false);
        }
    }

    fn db_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_db;
        egui::Window::new("Database")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Pad naar db.json van Signage")
                        .size(12.0)
                        .color(INK),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .desired_width(f32::INFINITY),
                );
                ui.label(
                    RichText::new("Standaard: de database naast de app of in %LOCALAPPDATA%\\Signage\\data.")
                        .size(10.5)
                        .color(FAINT),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Laden").clicked() {
                        let path = PathBuf::from(self.path_input.trim());
                        self.load(path);
                        self.show_db = false;
                    }
                    if ui.button("Annuleren").clicked() {
                        self.show_db = false;
                    }
                });
            });
        if !open {
            self.show_db = false;
        }
    }
}

impl App {
    fn data_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn decks_dir(&self) -> PathBuf {
        self.data_dir().join("decks")
    }

    fn publish_current_deck(&mut self) {
        if self.deckcraft.session.active().is_none() {
            self.status = "Open eerst een presentatie in DeckCraft".into();
            return;
        }
        let existing = self.deckcraft.session.active().and_then(|doc| doc.path.clone());
        let deck_path = match existing {
            Some(path) => PathBuf::from(path),
            None => {
                let dir = self.decks_dir();
                if let Err(err) = fs::create_dir_all(&dir) {
                    self.status = format!("Map {} aanmaken mislukt: {err}", dir.display());
                    return;
                }
                let path = dir.join(format!("presentatie-{}.pptx", now_ms()));
                if let Err(err) = self.deckcraft.session.execute(
                    "file.saveAs",
                    &json!({"path": path.to_string_lossy().to_string()}),
                ) {
                    self.status = format!("Presentatie opslaan mislukt: {err}");
                    return;
                }
                path
            }
        };
        // De renderer leest het document uit het geheugen; opslaan is nodig zodat het
        // .pptx-bestand en de zichtbare dia's dezelfde inhoud hebben.
        if deck_path.exists() {
            let _ = self.deckcraft.session.execute("file.save", &json!({}));
        }
        let outcome = match self.deckcraft.session.doc() {
            Ok(doc) => publish_presentation(&mut self.db, &self.path, doc, &deck_path),
            Err(err) => Err(err.to_string()),
        };
        match outcome {
            Ok(message) => {
                self.save_now();
                self.status = format!("{message} — live op de schermen");
            }
            Err(err) => self.status = format!("Publiceren mislukt: {err}"),
        }
    }

    /* ------------------------------------------------------------ schermen */

    fn screens(&self) -> &[Value] {
        self.db
            .get("screens")
            .and_then(Value::as_array)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn screen_sel_valid(&self) -> Option<usize> {
        self.screen_sel.filter(|i| *i < self.screens().len())
    }

    fn ensure_screen_sel(&mut self) {
        if self.screens().is_empty() {
            self.screen_sel = None;
        } else {
            let max = self.screens().len() - 1;
            self.screen_sel = Some(self.screen_sel.unwrap_or(0).min(max));
        }
    }

    fn screen_link(&self, screen: &Value) -> String {
        format!(
            "{}/player?k={}",
            self.server_url.trim().trim_end_matches('/'),
            get_str(screen, "key")
        )
    }

    fn add_screen(&mut self) {
        let name = format!("Scherm {}", self.screens().len() + 1);
        if let Some(list) = self.db.get_mut("screens").and_then(Value::as_array_mut) {
            list.push(default_screen(&name));
        }
        let len = self.screens().len();
        self.screen_sel = if len > 0 { Some(len - 1) } else { None };
        self.confirm_screen_delete = false;
        self.dirty = true;
        self.status = format!("{name} aangemaakt — opslaan met Ctrl+S");
    }

    fn delete_screen(&mut self) {
        let Some(index) = self.screen_sel_valid() else { return };
        let name = get_str(&self.screens()[index], "name");
        if let Some(list) = self.db.get_mut("screens").and_then(Value::as_array_mut) {
            if index < list.len() {
                list.remove(index);
            }
        }
        let len = self.screens().len();
        self.screen_sel = if len == 0 { None } else { Some(index.min(len - 1)) };
        self.confirm_screen_delete = false;
        self.dirty = true;
        self.status = format!("{name} verwijderd");
    }

    fn rotate_screen_key(&mut self) {
        let Some(index) = self.screen_sel_valid() else { return };
        let name = get_str(&self.screens()[index], "name");
        if let Some(list) = self.db.get_mut("screens").and_then(Value::as_array_mut) {
            if let Some(screen) = list.get_mut(index) {
                screen["key"] = json!(random_key());
                screen["updatedAt"] = json!(now_ms());
            }
        }
        self.dirty = true;
        self.status = format!("Nieuwe link voor {name} — opslaan met Ctrl+S");
    }

    fn ribbon_screens(&mut self, ui: &mut egui::Ui) {
        if rbtn(ui, Icon::Save, "Opslaan", true).clicked() {
            self.save_now();
        }
        if rbtn(ui, Icon::NewSlide, "Nieuw scherm", true).clicked() {
            self.add_screen();
        }
        let has = self.screen_sel_valid().is_some();
        if has && rbtn(ui, Icon::Delete, "Verwijderen", true).clicked() {
            self.delete_screen();
        }
        if !has {
            rbtn(ui, Icon::Delete, "Verwijderen", false);
        }
        ui.separator();
        if has && rbtn(ui, Icon::Reload, "Nieuwe link", true).clicked() {
            self.rotate_screen_key();
        }
        if !has {
            rbtn(ui, Icon::Reload, "Nieuwe link", false);
        }
        if has && rbtn(ui, Icon::Link, "Kopieer link", true).clicked() {
            if let Some(index) = self.screen_sel_valid() {
                let link = self.screen_link(&self.screens()[index]);
                ui.ctx().copy_text(link);
                self.status = "Player-link gekopieerd".into();
            }
        }
        if !has {
            rbtn(ui, Icon::Link, "Kopieer link", false);
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("Schermbeheer").size(13.0).strong().color(INK));
            ui.label(
                RichText::new("Kies links een scherm en stel rechts de playlist samen.")
                    .size(10.5)
                    .color(FAINT),
            );
            ui.label(
                RichText::new("Opslaan wordt direct door de draaiende zender opgepikt.")
                    .size(10.5)
                    .color(FAINT),
            );
        });
    }

    fn screens_list_ui(&mut self, ui: &mut egui::Ui) {
        self.ensure_screen_sel();
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("Schermen").size(13.0).strong().color(INK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.small_button("+").on_hover_text("Nieuw scherm").clicked() {
                    self.add_screen();
                }
            });
        });
        ui.separator();
        let screens: Vec<Value> = self.screens().to_vec();
        let mut picked = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                for (index, screen) in screens.iter().enumerate() {
                    let name = get_str(screen, "name");
                    let count = screen
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let paused = get_bool(screen, "paused");
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 54.0),
                        Sense::click(),
                    );
                    let selected = self.screen_sel == Some(index);
                    if selected {
                        ui.painter()
                            .rect_filled(rect, 0.0, Color32::from_rgb(0xF6, 0xE3, 0xDE));
                        ui.painter().rect_filled(
                            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                            0.0,
                            ACCENT,
                        );
                    } else if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, 0.0, Color32::from_rgb(0xEA, 0xE8, 0xE6));
                    }
                    ui.painter().text(
                        Pos2::new(rect.left() + 12.0, rect.top() + 10.0),
                        Align2::LEFT_TOP,
                        ellipsize(&name, 24),
                        FontId::proportional(12.5),
                        INK,
                    );
                    let sub = format!(
                        "{count} item{} · {}",
                        if count == 1 { "" } else { "s" },
                        if paused {
                            "gepauzeerd".to_string()
                        } else {
                            seen_label(screen)
                        }
                    );
                    ui.painter().text(
                        Pos2::new(rect.left() + 12.0, rect.top() + 31.0),
                        Align2::LEFT_TOP,
                        sub,
                        FontId::proportional(10.0),
                        MUTED,
                    );
                    if resp.clicked() {
                        picked = Some(index);
                    }
                    ui.add_space(1.0);
                }
                ui.add_space(8.0);
            });
        if let Some(index) = picked {
            self.screen_sel = Some(index);
            self.confirm_screen_delete = false;
        }
    }

    fn screens_ui(&mut self, ui: &mut egui::Ui) {
        self.ensure_screen_sel();
        let Some(index) = self.screen_sel else {
            ui.vertical_centered(|ui| {
                ui.add_space(90.0);
                ui.label(RichText::new("Nog geen schermen").size(19.0).strong().color(INK));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Maak een scherm aan en open de player-link op het scherm.")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.add_space(12.0);
                if ui.button("Nieuw scherm").clicked() {
                    self.add_screen();
                }
            });
            return;
        };
        let Some(mut screen) = self.screens().get(index).cloned() else {
            return;
        };
        let before = screen.clone();
        let mut delete_now = false;
        let deck_choices: Vec<(String, String)> = self
            .decks()
            .iter()
            .map(|d| (get_str(d, "id"), get_str(d, "name")))
            .collect();
        let media_choices: Vec<(String, String, String)> = self
            .db
            .get("media")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|m| (get_str(m, "id"), get_str(m, "name"), get_str(m, "kind")))
                    .collect()
            })
            .unwrap_or_default();
        let mut name = get_str(&screen, "name");
        let mut fit = get_str(&screen, "fit");
        if fit != "cover" {
            fit = "contain".into();
        }
        let mut paused = get_bool(&screen, "paused");
        let mut items = screen
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let before_items = items.clone();
        let link = self.screen_link(&screen);
        let seen = seen_label(&screen);
        let online = seen == "online";

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 22.0).max(360.0));
                        ui.label(RichText::new("SCHERM").size(10.0).color(MUTED));
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut name)
                                    .font(FontId::proportional(22.0))
                                    .frame(egui::Frame::NONE)
                                    .desired_width(420.0),
                            )
                            .changed()
                        {
                            screen["name"] = json!(name.clone());
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&seen)
                                    .size(11.0)
                                    .color(if online {
                                        Color32::from_rgb(0x1A, 0x7F, 0x37)
                                    } else {
                                        MUTED
                                    }),
                            );
                            ui.label(RichText::new("·").size(11.0).color(FAINT));
                            ui.label(
                                RichText::new(format!("{} items", items.len()))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                        ui.add_space(10.0);
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(0xFA, 0xF9, 0xF8))
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(4.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new("PLAYER-LINK")
                                        .size(9.5)
                                        .color(MUTED),
                                );
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&link).monospace().size(10.5).color(INK),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if ui.small_button("Kopieer").clicked() {
                                                ui.ctx().copy_text(link.clone());
                                            }
                                        },
                                    );
                                });
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new("SERVERADRES VOOR DE LINK")
                                        .size(9.5)
                                        .color(MUTED),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.server_url)
                                        .desired_width(300.0),
                                );
                                ui.label(
                                    RichText::new(
                                        "Bijvoorbeeld http://192.168.1.20:8100 — het adres waarop de ontvanger de zender bereikt.",
                                    )
                                    .size(9.5)
                                    .color(FAINT),
                                );
                            });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Weergave").size(11.0).color(MUTED));
                            let fit_before = fit.clone();
                            egui::ComboBox::from_id_salt("screen-fit")
                                .width(120.0)
                                .selected_text(if fit == "cover" { "Vullen" } else { "Passend" })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut fit, "contain".into(), "Passend");
                                    ui.selectable_value(&mut fit, "cover".into(), "Vullen");
                                });
                            if fit != fit_before {
                                screen["fit"] = json!(fit.clone());
                            }
                            if ui.checkbox(&mut paused, "Pauzeren").changed() {
                                screen["paused"] = json!(paused);
                            }
                        });
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            if self.confirm_screen_delete {
                                if ui
                                    .button(RichText::new("Ja, verwijderen").color(ACCENT))
                                    .clicked()
                                {
                                    delete_now = true;
                                }
                                if ui.button("Annuleren").clicked() {
                                    self.confirm_screen_delete = false;
                                }
                            } else if ui
                                .button(RichText::new("Scherm verwijderen").color(ACCENT))
                                .clicked()
                            {
                                self.confirm_screen_delete = true;
                            }
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("PLAYLIST").size(10.0).color(MUTED));
                            ui.label(
                                RichText::new(format!("{} items", items.len()))
                                    .size(10.0)
                                    .color(FAINT),
                            );
                        });
                        ui.label(
                            RichText::new(
                                "Volgorde met de pijltjes; dagen en tijden bepalen wanneer een item speelt.",
                            )
                            .size(10.0)
                            .color(FAINT),
                        );
                        ui.add_space(4.0);

                        let total = items.len();
                        let mut remove: Option<usize> = None;
                        let mut swap: Option<(usize, usize)> = None;
                        for (i, item) in items.iter_mut().enumerate() {
                            let kind = get_str(item, "type");
                            let (kind_label, kind_color) = match kind.as_str() {
                                "deck" => ("PRESENTATIE", Color32::from_rgb(0x1D, 0x4E, 0xD8)),
                                "media" => ("MEDIA", Color32::from_rgb(0x7C, 0x3A, 0xED)),
                                _ => ("URL", Color32::from_rgb(0x0F, 0x76, 0x6E)),
                            };
                            let title = match kind.as_str() {
                                "deck" => find_name(
                                    &deck_choices,
                                    &get_str(item, "presentationId"),
                                ),
                                "media" => {
                                    let id = get_str(item, "mediaId");
                                    media_choices
                                        .iter()
                                        .find(|(media_id, _, _)| media_id == &id)
                                        .map(|(_, name, _)| name.clone())
                                        .unwrap_or_else(|| "(media verwijderd)".into())
                                }
                                _ => get_str(item, "url"),
                            };
                            egui::Frame::NONE
                                .fill(Color32::WHITE)
                                .stroke(Stroke::new(1.0, LINE))
                                .corner_radius(4.0)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(format!("{:02}", i + 1))
                                                .size(9.5)
                                                .color(FAINT),
                                        );
                                        egui::Frame::NONE
                                            .fill(kind_color)
                                            .corner_radius(3.0)
                                            .inner_margin(4.0)
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(kind_label)
                                                        .size(8.0)
                                                        .color(Color32::WHITE),
                                                );
                                            });
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(title).size(12.0).color(INK),
                                            )
                                            .truncate(),
                                        );
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui
                                                    .button("×")
                                                    .on_hover_text("Verwijderen")
                                                    .clicked()
                                                {
                                                    remove = Some(i);
                                                }
                                                if ui
                                                    .add_enabled(
                                                        i + 1 < total,
                                                        egui::Button::new("↓"),
                                                    )
                                                    .clicked()
                                                {
                                                    swap = Some((i, i + 1));
                                                }
                                                if ui
                                                    .add_enabled(i > 0, egui::Button::new("↑"))
                                                    .clicked()
                                                {
                                                    swap = Some((i, i - 1));
                                                }
                                                let mut duration =
                                                    get_f32(item, "duration", 8.0)
                                                        .clamp(1.0, 86400.0);
                                                if ui
                                                    .add(
                                                        egui::DragValue::new(&mut duration)
                                                            .range(1.0..=86400.0)
                                                            .speed(1.0)
                                                            .suffix(" s"),
                                                    )
                                                    .changed()
                                                {
                                                    item["duration"] =
                                                        json!(duration.round() as i64);
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(3.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new("Dagen").size(9.5).color(MUTED),
                                        );
                                        let mut days: Vec<u64> = item
                                            .get("days")
                                            .and_then(Value::as_array)
                                            .map(|a| {
                                                a.iter()
                                                    .filter_map(Value::as_u64)
                                                    .collect()
                                            })
                                            .unwrap_or_default();
                                        let days_before = days.clone();
                                        for (day_label, day_num) in DAYS {
                                            let on = days.contains(&day_num);
                                            if ui
                                                .selectable_label(
                                                    on,
                                                    RichText::new(day_label).size(9.5),
                                                )
                                                .clicked()
                                            {
                                                if on {
                                                    days.retain(|d| *d != day_num);
                                                } else {
                                                    days.push(day_num);
                                                    days.sort_unstable();
                                                }
                                            }
                                        }
                                        if days != days_before {
                                            item["days"] = json!(days);
                                        }
                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new("Tijd").size(9.5).color(MUTED),
                                        );
                                        let mut time_start = get_str(item, "timeStart");
                                        let mut time_end = get_str(item, "timeEnd");
                                        let start_resp = ui.add(
                                            egui::TextEdit::singleline(&mut time_start)
                                                .hint_text("--:--")
                                                .desired_width(52.0)
                                                .font(FontId::monospace(10.0)),
                                        );
                                        ui.label(
                                            RichText::new("–").size(9.5).color(MUTED),
                                        );
                                        let end_resp = ui.add(
                                            egui::TextEdit::singleline(&mut time_end)
                                                .hint_text("--:--")
                                                .desired_width(52.0)
                                                .font(FontId::monospace(10.0)),
                                        );
                                        if start_resp.changed() {
                                            item["timeStart"] = json!(time_start);
                                        }
                                        if end_resp.changed() {
                                            item["timeEnd"] = json!(time_end);
                                        }
                                        if start_resp.lost_focus() {
                                            let value = get_str(item, "timeStart");
                                            item["timeStart"] = if valid_time(&value) {
                                                json!(value)
                                            } else {
                                                Value::Null
                                            };
                                        }
                                        if end_resp.lost_focus() {
                                            let value = get_str(item, "timeEnd");
                                            item["timeEnd"] = if valid_time(&value) {
                                                json!(value)
                                            } else {
                                                Value::Null
                                            };
                                        }
                                    });
                                });
                            ui.add_space(4.0);
                        }
                        if let Some((a, b)) = swap {
                            if a < items.len() && b < items.len() {
                                items.swap(a, b);
                            }
                        }
                        if let Some(i) = remove {
                            if i < items.len() {
                                items.remove(i);
                            }
                        }

                        ui.add_space(8.0);
                        section(ui, "Toevoegen");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Presentatie").size(10.5).color(MUTED),
                            );
                            if deck_choices.is_empty() {
                                ui.label(
                                    RichText::new("Geen presentaties")
                                        .size(10.5)
                                        .color(FAINT),
                                );
                            } else {
                                if self.pick_deck >= deck_choices.len() {
                                    self.pick_deck = 0;
                                }
                                egui::ComboBox::from_id_salt("screen-add-deck")
                                    .width(220.0)
                                    .selected_text(deck_choices[self.pick_deck].1.clone())
                                    .show_ui(ui, |ui| {
                                        for (i, (_, label)) in
                                            deck_choices.iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut self.pick_deck,
                                                i,
                                                label,
                                            );
                                        }
                                    });
                                if ui.button("+ Toevoegen").clicked() {
                                    let id = deck_choices[self.pick_deck].0.clone();
                                    items.push(json!({
                                        "id": random_id(8),
                                        "type": "deck",
                                        "presentationId": id,
                                        "duration": 10,
                                        "start": null,
                                        "end": null,
                                        "days": [],
                                        "timeStart": null,
                                        "timeEnd": null
                                    }));
                                    self.status =
                                        "Presentatie aan de playlist toegevoegd".into();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Media").size(10.5).color(MUTED));
                            if media_choices.is_empty() {
                                ui.label(
                                    RichText::new("Geen media in de bibliotheek")
                                        .size(10.5)
                                        .color(FAINT),
                                );
                            } else {
                                if self.pick_media >= media_choices.len() {
                                    self.pick_media = 0;
                                }
                                egui::ComboBox::from_id_salt("screen-add-media")
                                    .width(220.0)
                                    .selected_text(media_choices[self.pick_media].1.clone())
                                    .show_ui(ui, |ui| {
                                        for (i, (_, label, kind)) in
                                            media_choices.iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut self.pick_media,
                                                i,
                                                format!("{label} ({kind})"),
                                            );
                                        }
                                    });
                                if ui.button("+ Toevoegen").clicked() {
                                    let id = media_choices[self.pick_media].0.clone();
                                    items.push(json!({
                                        "id": random_id(8),
                                        "type": "media",
                                        "mediaId": id,
                                        "duration": 10,
                                        "start": null,
                                        "end": null,
                                        "days": [],
                                        "timeStart": null,
                                        "timeEnd": null
                                    }));
                                    self.status =
                                        "Media aan de playlist toegevoegd".into();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("URL").size(10.5).color(MUTED));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_url)
                                    .hint_text("https://voorbeeld.nl")
                                    .desired_width(320.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.new_url_duration)
                                    .range(1.0..=86400.0)
                                    .speed(1.0)
                                    .suffix(" s"),
                            );
                            if ui.button("+ Toevoegen").clicked() {
                                let url = self.new_url.trim().to_string();
                                if url.starts_with("http://") || url.starts_with("https://") {
                                    items.push(json!({
                                        "id": random_id(8),
                                        "type": "url",
                                        "url": url,
                                        "duration": self.new_url_duration.round().max(1.0) as i64,
                                        "start": null,
                                        "end": null,
                                        "days": [],
                                        "timeStart": null,
                                        "timeEnd": null
                                    }));
                                    self.new_url.clear();
                                    self.status = "URL aan de playlist toegevoegd".into();
                                } else {
                                    self.status =
                                        "URL moet met http:// of https:// beginnen".into();
                                }
                            }
                        });
                        ui.label(
                            RichText::new(
                                "Nieuwe mediabestanden uploaden kan nog niet native; bestaande media voeg je hier toe.",
                            )
                            .size(9.5)
                            .color(FAINT),
                        );
                    });
                });
            });

        if delete_now {
            self.delete_screen();
            return;
        }
        if items != before_items {
            screen["items"] = json!(items);
        }
        if screen != before {
            screen["updatedAt"] = json!(now_ms());
            if let Some(list) = self.db.get_mut("screens").and_then(Value::as_array_mut) {
                if let Some(slot) = list.get_mut(index) {
                    *slot = screen;
                }
            }
            self.dirty = true;
            self.status = "Scherm gewijzigd — opslaan met Ctrl+S".into();
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.deckcraft.logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.deckcraft.raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.presenting {
            self.present_ui(ui);
            return;
        }
        if self.tab != Tab::DeckCraft {
            self.handle_keys(&ctx);
        }

        let deckcraft_show = self.tab == Tab::DeckCraft && self.deckcraft.show.is_some();
        if !deckcraft_show {
            egui::Panel::top("tabs")
                .exact_size(36.0)
                .frame(egui::Frame::NONE.fill(CHROME))
                .show(ui, |ui| self.tabs_ui(ui));
        }

        if self.tab == Tab::DeckCraft {
            self.deckcraft.ui(ui);
            if self.show_db {
                self.db_window(&ctx);
            }
            return;
        }

        egui::Panel::top("ribbon")
            .exact_size(96.0)
            .frame(egui::Frame::NONE.fill(RIBBON))
            .show(ui, |ui| self.ribbon_ui(ui));
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::NONE.fill(CHROME))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Signage Studio 0.3 — ingebouwde DeckCraft-editor")
                            .size(10.5)
                            .color(MUTED),
                    );
                    ui.separator();
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.status.clone()).size(10.5).color(MUTED),
                        )
                        .truncate(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(if self.dirty {
                                "Wijzigingen niet opgeslagen"
                            } else {
                                "Alles opgeslagen"
                            })
                            .size(10.5)
                            .color(if self.dirty { ACCENT } else { MUTED }),
                        );
                    });
                });
            });
        let manage = self.tab == Tab::Screens;
        if manage {
            egui::Panel::left("screen-list")
                .exact_size(238.0)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(CHROME))
                .show(ui, |ui| self.screens_list_ui(ui));
        } else {
            egui::Panel::left("rail")
                .exact_size(172.0)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(CHROME))
                .show(ui, |ui| self.rail_ui(ui));
            if self.show_inspector {
                egui::Panel::right("inspector")
                    .exact_size(300.0)
                    .resizable(false)
                    .frame(egui::Frame::NONE.fill(Color32::WHITE))
                    .show(ui, |ui| self.inspector_ui(ui));
            }
            egui::Panel::bottom("notes")
                .exact_size(76.0)
                .frame(egui::Frame::NONE.fill(Color32::WHITE))
                .show(ui, |ui| self.notes_ui(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BENCH))
            .show(ui, |ui| {
                if manage {
                    self.screens_ui(ui);
                } else {
                    self.canvas_ui(ui);
                }
            });

        self.commit_form();
        self.commit_el();
        if self.show_db {
            self.db_window(&ctx);
        }
    }
}

/* ---------------------------------------------------------------- helpers */

fn norm_hex(value: &str, fallback: &str) -> String {
    let t = value.trim();
    if t.len() == 7 && t.starts_with('#') {
        t.to_ascii_lowercase()
    } else {
        fallback.to_string()
    }
}

fn toggle_el(value: &mut Value, key: &str, on: &str, off: &str) {
    if !value.is_object() {
        return;
    }
    let current = get_str(value, key);
    value[key] = json!(if current == on { off } else { on });
}

fn toggle_el_bool(value: &mut Value, key: &str) {
    if !value.is_object() {
        return;
    }
    let current = get_bool(value, key);
    value[key] = json!(!current);
}

fn move_layer(db: &mut Value, deck: usize, slide: usize, from: usize, to: usize) {
    if let Some(elements) = db
        .get_mut("presentations")
        .and_then(|v| v.get_mut(deck))
        .and_then(|d| d.get_mut("slides"))
        .and_then(|s| s.get_mut(slide))
        .and_then(|s| s.get_mut("elements"))
        .and_then(Value::as_array_mut)
    {
        if from < elements.len() && to < elements.len() {
            let element = elements.remove(from);
            elements.insert(to, element);
        }
    }
}

fn tab_button(ui: &mut egui::Ui, label: &str, active: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(13.0),
        INK,
    );
    let width = galley.size().x + 22.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    if active {
        ui.painter().rect_filled(rect, 0.0, RIBBON);
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.left() + 4.0, rect.bottom() - 3.0),
                Pos2::new(rect.right() - 4.0, rect.bottom()),
            ),
            0.0,
            ACCENT,
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_rgb(0xE9, 0xE7, 0xE5));
    }
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0 - 1.0,
        ),
        galley,
        INK,
    );
    resp
}

fn rbtn(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(64.0, 64.0), Sense::click());
    if resp.hovered() && enabled {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 4.0, Color32::from_rgb(0xEC, 0xEA, 0xE8));
    }
    let color = if enabled { INK } else { Color32::from_rgb(0xB9, 0xB7, 0xB5) };
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + 22.0),
        Vec2::new(22.0, 22.0),
    );
    paint_icon(ui.painter(), icon_rect, icon, color);
    ui.painter().text(
        Pos2::new(rect.center().x, rect.bottom() - 7.0),
        Align2::CENTER_BOTTOM,
        label,
        FontId::proportional(10.0),
        color,
    );
    if enabled {
        resp
    } else {
        resp.on_hover_text("Selecteer eerst een element")
    }
}

fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, color: Color32) {
    let c = r.center();
    let stroke = Stroke::new(1.5, color);
    let thin = Stroke::new(1.2, color);
    match icon {
        Icon::NewSlide => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(15.0, 12.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            let x = c.x + 6.0;
            let y = c.y - 7.0;
            p.line_segment([Pos2::new(x - 2.5, y), Pos2::new(x + 2.5, y)], stroke);
            p.line_segment([Pos2::new(x, y - 2.5), Pos2::new(x, y + 2.5)], stroke);
        }
        Icon::Duplicate => {
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x - 8.0, c.y - 8.0), Vec2::new(12.0, 12.0)),
                1.0,
                thin, egui::StrokeKind::Inside);
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x - 3.0, c.y - 3.0), Vec2::new(12.0, 12.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
        }
        Icon::Delete => {
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x - 6.0, c.y - 4.0), Vec2::new(12.0, 13.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [Pos2::new(c.x - 9.0, c.y - 7.0), Pos2::new(c.x + 9.0, c.y - 7.0)],
                stroke,
            );
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y - 10.0), Pos2::new(c.x + 3.0, c.y - 10.0)],
                stroke,
            );
        }
        Icon::Layout => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(16.0, 14.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [Pos2::new(c.x - 8.0, c.y - 1.0), Pos2::new(c.x + 8.0, c.y - 1.0)],
                thin,
            );
            p.line_segment(
                [Pos2::new(c.x - 1.0, c.y - 1.0), Pos2::new(c.x - 1.0, c.y + 7.0)],
                thin,
            );
        }
        Icon::Text => {
            p.text(c, Align2::CENTER_CENTER, "T", FontId::proportional(19.0), color);
        }
        Icon::Link => {
            p.rect_stroke(
                Rect::from_center_size(Pos2::new(c.x - 3.0, c.y), Vec2::new(11.0, 8.0)),
                4.0,
                thin, egui::StrokeKind::Inside);
            p.rect_stroke(
                Rect::from_center_size(Pos2::new(c.x + 3.0, c.y), Vec2::new(11.0, 8.0)),
                4.0,
                stroke, egui::StrokeKind::Inside);
            p.line_segment([Pos2::new(c.x - 2.0, c.y), Pos2::new(c.x + 2.0, c.y)], thin);
        }
        Icon::Image => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(17.0, 14.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.circle_filled(Pos2::new(c.x + 4.0, c.y - 3.0), 1.6, color);
            p.add(Shape::line(
                vec![
                    Pos2::new(c.x - 7.0, c.y + 5.0),
                    Pos2::new(c.x - 2.0, c.y - 1.0),
                    Pos2::new(c.x + 2.0, c.y + 3.0),
                    Pos2::new(c.x + 5.0, c.y + 0.5),
                    Pos2::new(c.x + 8.0, c.y + 5.0),
                ],
                thin,
            ));
        }
        Icon::Shape => {
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x - 9.0, c.y - 8.0), Vec2::new(11.0, 11.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.circle_stroke(Pos2::new(c.x + 3.0, c.y + 3.0), 5.5, stroke);
        }
        Icon::Save => {
            p.add(Shape::line(
                vec![
                    Pos2::new(c.x - 8.0, c.y + 1.0),
                    Pos2::new(c.x - 8.0, c.y + 8.0),
                    Pos2::new(c.x + 8.0, c.y + 8.0),
                    Pos2::new(c.x + 8.0, c.y + 1.0),
                ],
                stroke,
            ));
            p.line_segment(
                [Pos2::new(c.x, c.y - 9.0), Pos2::new(c.x, c.y + 3.0)],
                stroke,
            );
            p.add(Shape::line(
                vec![
                    Pos2::new(c.x - 3.5, c.y - 0.5),
                    Pos2::new(c.x, c.y + 3.5),
                    Pos2::new(c.x + 3.5, c.y - 0.5),
                ],
                stroke,
            ));
        }
        Icon::Open => {
            p.add(Shape::line(
                vec![
                    Pos2::new(c.x - 9.0, c.y + 7.0),
                    Pos2::new(c.x - 9.0, c.y - 6.0),
                    Pos2::new(c.x - 2.0, c.y - 6.0),
                    Pos2::new(c.x, c.y - 3.5),
                    Pos2::new(c.x + 8.0, c.y - 3.5),
                    Pos2::new(c.x + 8.0, c.y + 7.0),
                    Pos2::new(c.x - 9.0, c.y + 7.0),
                ],
                stroke,
            ));
        }
        Icon::Reload => {
            let mut points = Vec::new();
            for i in 0..30 {
                let angle = -0.6 + (i as f32 / 29.0) * 5.4;
                points.push(Pos2::new(
                    c.x + 8.0 * angle.cos(),
                    c.y + 8.0 * angle.sin(),
                ));
            }
            p.add(Shape::line(points, stroke));
            let tip = Pos2::new(c.x + 8.0 * (-0.6f32).cos(), c.y + 8.0 * (-0.6f32).sin());
            p.line_segment([tip, tip + Vec2::new(4.0, -1.0)], stroke);
            p.line_segment([tip, tip + Vec2::new(-1.0, 4.0)], stroke);
        }
        Icon::Close => {
            p.line_segment(
                [Pos2::new(c.x - 6.0, c.y - 6.0), Pos2::new(c.x + 6.0, c.y + 6.0)],
                stroke,
            );
            p.line_segment(
                [Pos2::new(c.x + 6.0, c.y - 6.0), Pos2::new(c.x - 6.0, c.y + 6.0)],
                stroke,
            );
        }
        Icon::Bold => {
            p.text(c, Align2::CENTER_CENTER, "B", FontId::proportional(18.0), color);
        }
        Icon::Italic => {
            p.text(c, Align2::CENTER_CENTER, "I", FontId::proportional(18.0), color);
        }
        Icon::Underline => {
            p.text(
                Pos2::new(c.x, c.y - 2.0),
                Align2::CENTER_CENTER,
                "U",
                FontId::proportional(18.0),
                color,
            );
            p.line_segment(
                [Pos2::new(c.x - 6.0, c.y + 8.0), Pos2::new(c.x + 6.0, c.y + 8.0)],
                stroke,
            );
        }
        Icon::AlignLeft | Icon::AlignCenter | Icon::AlignRight => {
            let widths = match icon {
                Icon::AlignLeft => [14.0, 9.0, 14.0, 7.0],
                Icon::AlignRight => [14.0, 9.0, 14.0, 7.0],
                _ => [14.0, 9.0, 14.0, 7.0],
            };
            for (i, width) in widths.iter().enumerate() {
                let y = c.y - 7.0 + i as f32 * 5.0;
                let x = match icon {
                    Icon::AlignLeft => c.x - 8.0,
                    Icon::AlignRight => c.x + 8.0 - width,
                    _ => c.x - width / 2.0,
                };
                p.line_segment([Pos2::new(x, y), Pos2::new(x + width, y)], stroke);
            }
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - 5.0, c.y - 8.0),
                    Pos2::new(c.x - 5.0, c.y + 8.0),
                    Pos2::new(c.x + 8.0, c.y),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Palette => {
            p.circle_stroke(c, 8.5, stroke);
            p.circle_filled(Pos2::new(c.x - 3.0, c.y - 3.0), 1.8, color);
            p.circle_filled(Pos2::new(c.x + 3.0, c.y - 3.0), 1.8, color);
            p.circle_filled(Pos2::new(c.x - 3.0, c.y + 3.0), 1.8, color);
            p.circle_filled(Pos2::new(c.x + 3.0, c.y + 3.0), 1.8, color);
        }
        Icon::Transition => {
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x - 9.0, c.y - 6.0), Vec2::new(10.0, 12.0)),
                1.0,
                thin, egui::StrokeKind::Inside);
            p.rect_stroke(
                Rect::from_min_size(Pos2::new(c.x + 1.0, c.y - 6.0), Vec2::new(10.0, 12.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y), Pos2::new(c.x + 4.0, c.y)],
                stroke,
            );
        }
        Icon::Anim => {
            let mut points = Vec::new();
            for i in 0..10 {
                let radius = if i % 2 == 0 { 8.5 } else { 3.5 };
                let angle = std::f32::consts::PI / 5.0 * i as f32 - std::f32::consts::FRAC_PI_2;
                points.push(Pos2::new(
                    c.x + radius * angle.cos(),
                    c.y + radius * angle.sin(),
                ));
            }
            p.add(Shape::convex_polygon(points, Color32::TRANSPARENT, stroke));
        }
        Icon::Zoom => {
            p.circle_stroke(Pos2::new(c.x - 2.0, c.y - 2.0), 6.5, stroke);
            p.line_segment(
                [Pos2::new(c.x + 3.0, c.y + 3.0), Pos2::new(c.x + 8.0, c.y + 8.0)],
                stroke,
            );
        }
        Icon::Panel => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(16.0, 13.0)),
                1.0,
                stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [Pos2::new(c.x + 3.0, c.y - 6.5), Pos2::new(c.x + 3.0, c.y + 6.5)],
                thin,
            );
        }
        Icon::Line => {
            p.line_segment(
                [Pos2::new(c.x - 7.0, c.y + 7.0), Pos2::new(c.x + 7.0, c.y - 7.0)],
                stroke,
            );
        }
        Icon::Back | Icon::Front => {
            let dir = if matches!(icon, Icon::Front) { 1.0 } else { -1.0 };
            p.line_segment(
                [Pos2::new(c.x - 9.0, c.y + dir * 1.0), Pos2::new(c.x + 5.0, c.y + dir * 1.0)],
                stroke,
            );
            p.add(Shape::line(
                vec![
                    Pos2::new(c.x + 1.0, c.y - 3.0 + dir * 1.0),
                    Pos2::new(c.x + 5.0, c.y + dir * 1.0),
                    Pos2::new(c.x + 1.0, c.y + 3.0 + dir * 1.0),
                ],
                stroke,
            ));
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(11.0).strong().color(ACCENT));
    ui.add_space(2.0);
}

fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String, multiline: bool) -> bool {
    ui.label(RichText::new(label).size(10.5).color(MUTED));
    let edit = if multiline {
        egui::TextEdit::multiline(value).desired_rows(2)
    } else {
        egui::TextEdit::singleline(value)
    };
    ui.add(edit.desired_width(f32::INFINITY)).changed()
}

fn el_text_field(ui: &mut egui::Ui, label: &str, value: &mut Value, key: &str) -> bool {
    let mut text = get_str(value, key);
    ui.label(RichText::new(label).size(10.5).color(MUTED));
    if ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        value[key] = json!(text);
        true
    } else {
        false
    }
}

fn color_field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let color = hex_or(value, Color32::WHITE);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
        if !label.is_empty() {
            ui.label(RichText::new(label).size(11.0).color(MUTED));
        }
        let resp = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(74.0)
                .font(egui::TextStyle::Monospace),
        );
        if resp.changed() {
            changed = true;
        }
    });
    changed
}

fn el_color_field(ui: &mut egui::Ui, label: &str, value: &mut Value, key: &str) -> bool {
    let mut text = get_str(value, key);
    if text.is_empty() {
        text = "transparent".into();
    }
    let mut changed = false;
    ui.horizontal(|ui| {
        let color = if text == "transparent" {
            Color32::from_rgb(0xF0, 0xF0, 0xF0)
        } else {
            hex_or(&text, Color32::WHITE)
        };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
        ui.label(RichText::new(label).size(11.0).color(MUTED));
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(74.0)
                    .font(egui::TextStyle::Monospace),
            )
            .changed()
        {
            value[key] = json!(text.trim().to_ascii_lowercase());
            changed = true;
        }
    });
    changed
}

/* ------------------------------------------------------------- rendering */

fn draw_slide(painter: &egui::Painter, rect: Rect, slide: &Value, editor: bool, sel_el: Option<usize>) {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    let p = painter.with_clip_rect(rect);
    let bg = hex_or(&get_str(slide, "bg"), Color32::from_rgb(0x0F, 0x17, 0x2A));
    let bg2 = get_str(slide, "bg2");
    if bg2.len() == 7 && bg2.starts_with('#') {
        let mut mesh = egui::epaint::Mesh::default();
        mesh.colored_vertex(rect.left_top(), bg);
        mesh.colored_vertex(rect.right_top(), bg);
        mesh.colored_vertex(rect.right_bottom(), hex(&bg2));
        mesh.colored_vertex(rect.left_bottom(), hex(&bg2));
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(Shape::mesh(mesh));
    } else {
        p.rect_filled(rect, 0.0, bg);
    }
    let fg = hex_or(&get_str(slide, "fg"), Color32::from_rgb(0xF8, 0xFA, 0xFC));
    let accent = hex_or(&get_str(slide, "accent"), Color32::from_rgb(0x5B, 0x8C, 0xFF));
    let font = get_str(slide, "font");
    let layout = get_str(slide, "layout");
    let h = rect.height();
    let title = get_str(slide, "title");
    let subtitle = get_str(slide, "subtitle");
    let body = get_str(slide, "body");
    let footer = get_str(slide, "footer");

    match layout.as_str() {
        "free" => {
            if slide.get("imageId").map(|v| !v.is_null()).unwrap_or(false) {
                p.rect_filled(rect, 0.0, Color32::from_rgb(0x1E, 0x24, 0x2C));
            }
            draw_elements(&p, rect, slide, fg, editor, sel_el);
            if editor && get_bool(slide, "hidden") {
                p.rect_filled(rect, 0.0, Color32::from_black_alpha(120));
                p.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Verborgen dia",
                    FontId::proportional(h * 0.06),
                    Color32::WHITE,
                );
            }
        }
        "full" => {
            draw_image_placeholder(&p, rect, "Afbeelding op volledige dia");
            let shade = get_f32(slide, "shade", 0.45).clamp(0.0, 1.0);
            p.rect_filled(rect, 0.0, Color32::from_black_alpha((shade * 180.0) as u8));
            title_block(&p, rect, &title, &subtitle, &body, &footer, fg, accent, &font, false);
        }
        "image-left" | "image-right" => {
            let half = rect.width() * 0.5;
            let image_rect = if layout == "image-left" {
                Rect::from_min_size(rect.min, Vec2::new(half, rect.height()))
            } else {
                Rect::from_min_max(
                    Pos2::new(rect.left() + half, rect.top()),
                    rect.max,
                )
            };
            draw_image_placeholder(&p, image_rect, "Afbeelding");
            let text_rect = if layout == "image-left" {
                Rect::from_min_max(Pos2::new(rect.left() + half, rect.top()), rect.max)
            } else {
                Rect::from_min_size(rect.min, Vec2::new(half, rect.height()))
            };
            title_block(
                &p,
                text_rect.shrink2(Vec2::new(h * 0.06, h * 0.06)),
                &title,
                &subtitle,
                &body,
                &footer,
                fg,
                accent,
                &font,
                false,
            );
        }
        "quote" => {
            let cx = rect.center().x;
            let mut y = rect.top() + h * 0.2;
            if title.is_empty() && editor {
                y += h * 0.06;
            } else {
                y = draw_wrapped(
                    &p,
                    cx,
                    y,
                    rect.width() * 0.76,
                    &title,
                    h * 0.065,
                    fg,
                    &font,
                    true,
                    false,
                ) + h * 0.04;
            }
            if !body.is_empty() {
                y = draw_wrapped(
                    &p,
                    cx,
                    y,
                    rect.width() * 0.76,
                    &format!("\u{201C}{body}\u{201D}"),
                    h * 0.038,
                    fg,
                    &font,
                    true,
                    false,
                ) + h * 0.04;
            }
            if !subtitle.is_empty() {
                draw_wrapped(
                    &p,
                    cx,
                    y,
                    rect.width() * 0.7,
                    &subtitle,
                    h * 0.028,
                    accent,
                    &font,
                    true,
                    false,
                );
            }
            footer_text(&p, rect, &footer, h, accent, &font);
        }
        "text" => {
            let left = rect.left() + rect.width() * 0.08;
            let width = rect.width() * 0.84;
            let mut y = rect.top() + h * 0.13;
            if title.is_empty() {
                if editor {
                    placeholder_box(
                        &p,
                        Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, h * 0.16)),
                        "Klik om een titel toe te voegen",
                        h * 0.062,
                        fg,
                    );
                }
                y += h * 0.2;
            } else {
                y = draw_wrapped(
                    &p,
                    left + width / 2.0,
                    y,
                    width,
                    &title,
                    h * 0.075,
                    fg,
                    &font,
                    true,
                    false,
                ) + h * 0.05;
            }
            if subtitle.is_empty() {
                if editor {
                    placeholder_box(
                        &p,
                        Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, h * 0.09)),
                        "Klik om een ondertitel toe te voegen",
                        h * 0.032,
                        fg,
                    );
                }
            } else {
                y = draw_wrapped(
                    &p,
                    left + width / 2.0,
                    y,
                    width,
                    &subtitle,
                    h * 0.034,
                    fg,
                    &font,
                    true,
                    false,
                ) + h * 0.04;
            }
            draw_bullets(&p, left, y, width, &body, h * 0.034, fg, accent, &font);
            footer_text(&p, rect, &footer, h, accent, &font);
        }
        _ => {
            title_block(&p, rect, &title, &subtitle, &body, &footer, fg, accent, &font, editor);
        }
    }
}

fn title_block(
    p: &egui::Painter,
    rect: Rect,
    title: &str,
    subtitle: &str,
    body: &str,
    footer: &str,
    fg: Color32,
    accent: Color32,
    font: &str,
    editor: bool,
) {
    let h = rect.height();
    let cx = rect.center().x;
    let width = rect.width() * 0.76;
    let mut y = rect.top() + h * 0.34;
    if title.is_empty() {
        if editor {
            let box_rect = Rect::from_center_size(
                Pos2::new(cx, rect.top() + h * 0.42),
                Vec2::new(width, h * 0.15),
            );
            placeholder_box(p, box_rect, "Klik om een titel toe te voegen", h * 0.07, fg);
        }
    } else {
        let used = draw_wrapped(
            p,
            cx,
            y,
            width,
            title,
            h * 0.082,
            fg,
            font,
            true,
            false,
        );
        y += used + h * 0.035;
    }
    if subtitle.is_empty() {
        if editor {
            let box_rect = Rect::from_center_size(
                Pos2::new(cx, rect.top() + h * 0.62),
                Vec2::new(width, h * 0.1),
            );
            placeholder_box(p, box_rect, "Klik om een ondertitel toe te voegen", h * 0.034, fg);
        }
    } else {
        y = draw_wrapped(p, cx, y, width, subtitle, h * 0.036, fg, font, true, false) + h * 0.03;
    }
    if !body.is_empty() {
        draw_bullets(
            p,
            rect.left() + rect.width() * 0.12,
            y,
            rect.width() * 0.76,
            body,
            h * 0.032,
            fg,
            accent,
            font,
        );
    }
    footer_text(p, rect, footer, h, accent, font);
}

fn footer_text(p: &egui::Painter, rect: Rect, footer: &str, h: f32, accent: Color32, font: &str) {
    if footer.is_empty() {
        return;
    }
    p.text(
        Pos2::new(rect.center().x, rect.bottom() - h * 0.04),
        Align2::CENTER_BOTTOM,
        footer,
        FontId::new(h * 0.024, family_for(font)),
        with_alpha(accent, 0.95),
    );
}

fn placeholder_box(p: &egui::Painter, rect: Rect, text: &str, size: f32, fg: Color32) {
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, with_alpha(fg, 0.28)), egui::StrokeKind::Inside);
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(size.max(8.0)),
        with_alpha(fg, 0.62),
    );
}

fn draw_image_placeholder(p: &egui::Painter, rect: Rect, label: &str) {
    p.rect_filled(rect, 0.0, Color32::from_rgb(0x27, 0x2C, 0x34));
    p.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(0x3A, 0x40, 0x49)), egui::StrokeKind::Inside);
    let c = rect.center();
    let size = (rect.height() * 0.18).min(rect.width() * 0.18).max(8.0);
    p.rect_stroke(
        Rect::from_center_size(c - Vec2::new(0.0, size * 0.2), Vec2::splat(size)),
        2.0,
        Stroke::new(1.4, Color32::from_gray(0x9A)), egui::StrokeKind::Inside);
    p.circle_filled(
        Pos2::new(c.x + size * 0.18, c.y - size * 0.42),
        size * 0.08,
        Color32::from_gray(0x9A),
    );
    p.add(Shape::line(
        vec![
            Pos2::new(c.x - size * 0.42, c.y + size * 0.18),
            Pos2::new(c.x - size * 0.1, c.y - size * 0.12),
            Pos2::new(c.x + size * 0.12, c.y + size * 0.1),
            Pos2::new(c.x + size * 0.28, c.y - size * 0.04),
            Pos2::new(c.x + size * 0.42, c.y + size * 0.18),
        ],
        Stroke::new(1.2, Color32::from_gray(0x9A)),
    ));
    p.text(
        Pos2::new(c.x, c.y + size * 0.9),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional((rect.height() * 0.026).max(8.0)),
        Color32::from_gray(0xA8),
    );
}

fn draw_elements(
    p: &egui::Painter,
    rect: Rect,
    slide: &Value,
    fg: Color32,
    editor: bool,
    sel_el: Option<usize>,
) {
    let Some(elements) = slide.get("elements").and_then(Value::as_array) else {
        if editor {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Vrij canvas — voeg tekst of een vorm toe via Invoegen",
                FontId::proportional((rect.height() * 0.035).max(8.0)),
                with_alpha(fg, 0.45),
            );
        }
        return;
    };
    for (index, element) in elements.iter().enumerate() {
        let r = element_rect(slide, rect, index);
        let kind = get_str(element, "type");
        let opacity = get_f32(element, "opacity", 1.0).clamp(0.05, 1.0);
        if kind == "shape" {
            let fill = paint_color(&get_str(element, "fill"), opacity);
            let stroke = paint_color(&get_str(element, "stroke"), opacity);
            draw_shape(
                p,
                r,
                &get_str(element, "shape"),
                fill,
                stroke,
                get_f32(element, "strokeWidth", 0.0),
                get_f32(element, "radius", 0.0),
            );
        } else if kind == "image" {
            let has_media = element.get("imageId").map(|v| !v.is_null()).unwrap_or(false);
            draw_image_placeholder(p, r, if has_media { "Afbeelding" } else { "Afbeeldingsvak" });
        } else {
            let background = paint_color(&get_str(element, "background"), opacity);
            if let Some(color) = background {
                p.rect_filled(r, 2.0, color);
            }
            let color = paint_color(&get_str(element, "color"), opacity).unwrap_or(fg);
            let size = rect.height() * get_f32(element, "size", 6.0) / 100.0;
            let font = get_str(element, "font");
            let weight = get_str(element, "weight");
            let align = get_str(element, "align");
            let valign = get_str(element, "valign");
            let text = get_str(element, "text");
            if get_bool(element, "bullet") {
                let gap = size * 0.55;
                let mut y = match valign.as_str() {
                    "top" => r.top(),
                    "bottom" => r.bottom() - size,
                    _ => r.center().y - (text.lines().count() as f32 * (size + gap)) / 2.0,
                };
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    p.circle_filled(Pos2::new(r.left() + size * 0.3, y + size * 0.55), size * 0.11, color);
                    let galley = p.layout(
                        line.trim().to_string(),
                        FontId::new(size, family_for(&font)),
                        color,
                        (r.width() - size * 0.8).max(10.0),
                    );
                    let x = match align.as_str() {
                        "center" => r.center().x - galley.size().x / 2.0,
                        "right" => r.right() - galley.size().x,
                        _ => r.left() + size * 0.8,
                    };
                    draw_galley(p, Pos2::new(x, y), &galley, color, weight == "bold");
                    y += galley.size().y + gap;
                }
            } else {
                let galley = p.layout(
                    text,
                    FontId::new(size, family_for(&font)),
                    color,
                    r.width(),
                );
                let x = match align.as_str() {
                    "center" => r.center().x - galley.size().x / 2.0,
                    "right" => r.right() - galley.size().x,
                    _ => r.left(),
                };
                let y = match valign.as_str() {
                    "top" => r.top(),
                    "bottom" => r.bottom() - galley.size().y,
                    _ => r.center().y - galley.size().y / 2.0,
                };
                draw_galley(p, Pos2::new(x, y), &galley, color, weight == "bold");
            }
        }
        if editor && sel_el == Some(index) {
            p.rect_stroke(r, 0.0, Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
            for corner in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                p.rect_filled(
                    Rect::from_center_size(corner, Vec2::splat(6.0)),
                    0.0,
                    Color32::WHITE,
                );
                p.rect_stroke(
                    Rect::from_center_size(corner, Vec2::splat(6.0)),
                    0.0,
                    Stroke::new(1.2, ACCENT), egui::StrokeKind::Inside);
            }
        }
    }
}

fn draw_galley(
    p: &egui::Painter,
    pos: Pos2,
    galley: &std::sync::Arc<egui::Galley>,
    color: Color32,
    bold: bool,
) {
    if bold {
        p.galley(pos + Vec2::new(0.7, 0.0), galley.clone(), color);
    }
    p.galley(pos, galley.clone(), color);
}

fn draw_shape(
    p: &egui::Painter,
    r: Rect,
    shape: &str,
    fill: Option<Color32>,
    stroke: Option<Color32>,
    stroke_width: f32,
    radius: f32,
) {
    let stroke = stroke
        .map(|c| Stroke::new(stroke_width.max(0.6), c))
        .unwrap_or(Stroke::NONE);
    let fill = fill.unwrap_or(Color32::TRANSPARENT);
    match shape {
        "ellipse" => {
            let points = ellipse_points(r.center(), r.size() / 2.0, 48);
            p.add(Shape::convex_polygon(points, fill, stroke));
        }
        "triangle" => {
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(r.center().x, r.top()),
                    Pos2::new(r.right(), r.bottom()),
                    Pos2::new(r.left(), r.bottom()),
                ],
                fill,
                stroke,
            ));
        }
        "line" => {
            let c = stroke.color;
            p.line_segment(
                [
                    Pos2::new(r.left(), r.center().y),
                    Pos2::new(r.right(), r.center().y),
                ],
                Stroke::new(stroke_width.max(1.0), c),
            );
        }
        "arrow" => {
            let mut points = vec![
                Pos2::new(r.left(), r.top() + r.height() * 0.3),
                Pos2::new(r.left() + r.width() * 0.6, r.top() + r.height() * 0.3),
                Pos2::new(r.left() + r.width() * 0.6, r.top() + r.height() * 0.12),
                Pos2::new(r.right(), r.center().y),
                Pos2::new(r.left() + r.width() * 0.6, r.bottom() - r.height() * 0.12),
                Pos2::new(r.left() + r.width() * 0.6, r.bottom() - r.height() * 0.3),
                Pos2::new(r.left(), r.bottom() - r.height() * 0.3),
            ];
            points.dedup();
            p.add(Shape::convex_polygon(points, fill, stroke));
        }
        "star" => {
            let mut points = Vec::new();
            for i in 0..10 {
                let radius = if i % 2 == 0 {
                    r.width().min(r.height()) / 2.0
                } else {
                    r.width().min(r.height()) * 0.19
                };
                let angle = std::f32::consts::PI / 5.0 * i as f32 - std::f32::consts::FRAC_PI_2;
                points.push(Pos2::new(
                    r.center().x + radius * angle.cos(),
                    r.center().y + radius * angle.sin(),
                ));
            }
            p.add(Shape::convex_polygon(points, fill, stroke));
        }
        "round" => {
            p.rect_filled(r, radius.max(2.0), fill);
            if stroke.width > 0.0 {
                p.rect_stroke(r, radius.max(2.0), stroke, egui::StrokeKind::Inside);
            }
        }
        _ => {
            p.rect_filled(r, 0.0, fill);
            if stroke.width > 0.0 {
                p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }
}

fn ellipse_points(center: Pos2, radius: Vec2, steps: usize) -> Vec<Pos2> {
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let angle = i as f32 / steps as f32 * std::f32::consts::TAU;
        points.push(Pos2::new(
            center.x + radius.x * angle.cos(),
            center.y + radius.y * angle.sin(),
        ));
    }
    points
}

fn paint_color(value: &str, alpha: f32) -> Option<Color32> {
    if value.trim().is_empty() || value.trim() == "transparent" {
        return None;
    }
    Some(with_alpha(hex_or(value, Color32::WHITE), alpha))
}

fn draw_wrapped(
    p: &egui::Painter,
    center_x: f32,
    top: f32,
    width: f32,
    text: &str,
    size: f32,
    color: Color32,
    font: &str,
    center: bool,
    bold: bool,
) -> f32 {
    if text.trim().is_empty() {
        return 0.0;
    }
    let galley = p.layout(
        text.to_string(),
        FontId::new(size.max(7.0), family_for(font)),
        color,
        width.max(20.0),
    );
    let x = if center {
        center_x - galley.size().x / 2.0
    } else {
        center_x
    };
    draw_galley(p, Pos2::new(x, top), &galley, color, bold);
    galley.size().y
}

fn draw_bullets(
    p: &egui::Painter,
    left: f32,
    top: f32,
    width: f32,
    body: &str,
    size: f32,
    color: Color32,
    bullet_color: Color32,
    font: &str,
) -> f32 {
    let gap = size * 0.55;
    let mut y = top;
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let clean = line.trim_start_matches(['-', '•', '*']).trim();
        if clean.is_empty() {
            continue;
        }
        p.circle_filled(
            Pos2::new(left + size * 0.32, y + size * 0.55),
            size * 0.12,
            bullet_color,
        );
        let galley = p.layout(
            clean.to_string(),
            FontId::new(size.max(7.0), family_for(font)),
            color,
            (width - size * 0.9).max(20.0),
        );
        p.galley(Pos2::new(left + size * 0.9, y), galley.clone(), color);
        y += galley.size().y + gap;
    }
    y - top
}

fn element_rect(slide: &Value, rect: Rect, index: usize) -> Rect {
    let element = slide
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|a| a.get(index));
    let Some(element) = element else {
        return Rect::from_min_size(rect.min, Vec2::ZERO);
    };
    let x = get_f32(element, "x", 0.0) / 100.0;
    let y = get_f32(element, "y", 0.0) / 100.0;
    let w = get_f32(element, "w", 10.0) / 100.0;
    let h = get_f32(element, "h", 10.0) / 100.0;
    Rect::from_min_max(
        Pos2::new(rect.left() + x * rect.width(), rect.top() + y * rect.height()),
        Pos2::new(
            rect.left() + (x + w) * rect.width(),
            rect.top() + (y + h) * rect.height(),
        ),
    )
}

fn hit_test(slide: &Value, rect: Rect, point: Pos2) -> Option<usize> {
    let elements = slide.get("elements").and_then(Value::as_array)?;
    for index in (0..elements.len()).rev() {
        if element_rect(slide, rect, index).contains(point) {
            return Some(index);
        }
    }
    None
}

fn startup_deck() -> Option<PathBuf> {
    let args: Vec<String> = env::args().skip(1).collect();
    let index = args.iter().position(|arg| arg == "--open-deck")?;
    args.get(index + 1).map(PathBuf::from)
}

fn headless_publish() -> Option<i32> {
    let args: Vec<String> = env::args().skip(1).collect();
    let index = args.iter().position(|arg| arg == "--publish-deck")?;
    let Some(path) = args.get(index + 1) else {
        eprintln!("gebruik: Signage Studio --publish-deck <bestand.pptx>");
        return Some(2);
    };
    let db_path = default_db_path();
    let mut db: Value = match fs::read_to_string(&db_path)
        .map_err(|err| err.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|err| err.to_string()))
    {
        Ok(db) => db,
        Err(err) => {
            eprintln!("database {} lezen: {err}", db_path.display());
            return Some(1);
        }
    };
    let mut session = deckcraft_engine::Session::new();
    if let Err(err) = session.execute(
        "file.open",
        &json!({"path": path.to_string()}),
    ) {
        eprintln!("deck openen: {err}");
        return Some(1);
    }
    let state = match session.doc() {
        Ok(state) => state,
        Err(err) => {
            eprintln!("deck openen: {err}");
            return Some(1);
        }
    };
    match publish_presentation(&mut db, &db_path, state, Path::new(path)) {
        Ok(message) => match write_db_atomic(&db_path, &db) {
            Ok(()) => {
                println!("{message}");
                Some(0)
            }
            Err(err) => {
                eprintln!("database opslaan: {err}");
                Some(1)
            }
        },
        Err(err) => {
            eprintln!("publiceren: {err}");
            Some(1)
        }
    }
}

fn main() -> eframe::Result<()> {
    if let Some(code) = headless_publish() {
        std::process::exit(code);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 780.0])
            .with_min_inner_size([1000.0, 620.0])
            .with_title("Signage Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "Signage Studio",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
