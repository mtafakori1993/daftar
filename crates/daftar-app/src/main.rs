#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod paths;

use std::io::Write;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use daftar_core::settings::AppSettings;
use daftar_ui::{AppConfig, DaftarApp};
use eframe::egui;
use paths::{AppPaths, parse_cli};

fn main() {
    if let Err(error) = run() {
        eprintln!("Daftar failed to start: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = parse_cli().map_err(anyhow::Error::msg)?;
    let paths = AppPaths::resolve(&cli).map_err(anyhow::Error::msg)?;
    init_logging(&paths)?;

    if cli.check {
        let store = daftar_store::Store::open(&paths.db_path)
            .with_context(|| format!("opening {}", paths.db_path.display()))?;
        let libraries = store.libraries()?;
        let integrity = store.integrity_check()?;
        println!("Daftar {} — environment check", env!("CARGO_PKG_VERSION"));
        println!("portable: {}", paths.portable);
        println!("data:     {}", paths.data_dir.display());
        println!("library:  {}", paths.library_root.display());
        println!("database: {} (integrity: {integrity})", paths.db_path.display());
        println!("libraries: {}", libraries.len());
        for library in libraries {
            println!("  - {} — {}", library.name, library.root_path);
        }
        let engine = daftar_text::TextEngine::new(Some(paths.fonts_dir.clone()), "en-US");
        println!("fonts:     {} families ({} bundled)", engine.families().len(), engine.bundled_families().len());
        return Ok(());
    }

    let settings = load_settings(&paths.settings_path);
    let width = settings.window.width.max(760.0);
    let height = settings.window.height.max(560.0);
    let icon = app_icon();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Daftar")
            .with_inner_size([width, height])
            .with_min_inner_size([760.0, 560.0])
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    let config: AppConfig = paths.to_app_config();
    eframe::run_native(
        "Daftar",
        native_options,
        Box::new(move |cc| {
            let app = DaftarApp::new(cc, config).map_err(|e| {
                Box::new(std::io::Error::other(format!("{e:#}"))) as Box<dyn std::error::Error + Send + Sync>
            })?;
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;
    Ok(())
}

fn load_settings(path: &std::path::Path) -> AppSettings {
    match std::fs::read_to_string(path) {
        Ok(text) => AppSettings::from_toml(&text).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

fn init_logging(paths: &AppPaths) -> Result<()> {
    let log_path = paths.logs_dir.join("daftar.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    let tee = TeeWriter {
        file: file.map(Mutex::new),
    };
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    );
    builder.target(env_logger::Target::Pipe(Box::new(tee)));
    let _ = builder.try_init();
    if let Some(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok()
    {
        let _ = writeln!(
            &file,
            "\n=== Daftar {} starting (portable: {}) ===",
            env!("CARGO_PKG_VERSION"),
            paths.portable
        );
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default_hook(info);
    }));
    Ok(())
}

struct TeeWriter {
    file: Option<Mutex<std::fs::File>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
        Ok(())
    }
}

fn app_icon() -> egui::IconData {
    load_icon().unwrap_or_else(fallback_icon)
}

fn load_icon() -> Option<egui::IconData> {
    const PNG: &[u8] = include_bytes!("../../../assets/icons/daftar.png");
    let source = image::load_from_memory(PNG).ok()?.to_rgba8();
    let size = 256u32;
    let resized = image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
    Some(egui::IconData {
        rgba: resized.into_raw(),
        width: size,
        height: size,
    })
}

fn fallback_icon() -> egui::IconData {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let inside = x > 6 && x < size - 6 && y > 6 && y < size - 6;
            if inside {
                rgba[i] = 0x26;
                rgba[i + 1] = 0x80;
                rgba[i + 2] = 0xEB;
                rgba[i + 3] = 0xFF;
                let line = (y > 16 && y < 21 && x > 14 && x < 50)
                    || (y > 28 && y < 33 && x > 14 && x < 42)
                    || (y > 40 && y < 45 && x > 14 && x < 47);
                if line {
                    rgba[i] = 0xFF;
                    rgba[i + 1] = 0xFF;
                    rgba[i + 2] = 0xFF;
                }
            }
        }
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}
