//! Portable path resolution.
//!
//! Portable mode is selected by a `daftar.portable` marker next to the executable (included in
//! the release archive). All configuration, the database, fonts and the default library then
//! live under `data/` beside the executable, so the app can run from a USB drive without
//! installation or system-wide changes. `DAFTAR_DATA_DIR` overrides the data directory.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub exe_dir: PathBuf,
    pub data_dir: PathBuf,
    pub library_root: PathBuf,
    pub settings_path: PathBuf,
    pub db_path: PathBuf,
    pub fonts_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub portable: bool,
}

impl AppPaths {
    pub fn resolve(cli: &CliOptions) -> Result<Self, String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("Cannot determine executable path: {e}"))?;
        let exe_dir = exe
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let marker = exe_dir.join("daftar.portable");
        let mut portable = marker.exists()
            || std::env::var("DAFTAR_PORTABLE").ok().as_deref() == Some("1")
            || cli.portable;

        if let Some(dir) = &cli.data_dir {
            if let Err(error) = std::fs::create_dir_all(dir) {
                return Err(format!("Cannot create data directory {}: {error}", dir.display()));
            }
            return Ok(Self::assemble(exe_dir, dir.clone(), portable));
        }

        if let Ok(dir) = std::env::var("DAFTAR_DATA_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Cannot create data directory {}: {e}", dir.display()))?;
            return Ok(Self::assemble(exe_dir, dir, portable));
        }

        if cli.portable && !marker.exists() {
            // Creating the marker makes portability persistent for subsequent launches.
            match std::fs::write(&marker, "Daftar portable mode\n") {
                Ok(()) => portable = true,
                Err(error) => {
                    return Err(format!(
                        "Cannot enable portable mode: cannot write {}: {error}",
                        marker.display()
                    ));
                }
            }
        }

        let data_dir = if portable {
            exe_dir.join("data")
        } else {
            platform_data_dir()?
        };
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Cannot create data directory {}: {e}", data_dir.display()))?;
        Ok(Self::assemble(exe_dir, data_dir, portable))
    }

    fn assemble(exe_dir: PathBuf, data_dir: PathBuf, portable: bool) -> Self {
        let library_root = data_dir.join("library");
        let settings_path = data_dir.join("settings.toml");
        let db_path = data_dir.join("daftar.db");
        let fonts_dir = data_dir.join("fonts");
        let backups_dir = data_dir.join("backups");
        let logs_dir = data_dir.join("logs");
        for dir in [&library_root, &fonts_dir, &backups_dir, &logs_dir] {
            let _ = std::fs::create_dir_all(dir);
        }
        Self {
            exe_dir,
            data_dir,
            library_root,
            settings_path,
            db_path,
            fonts_dir,
            backups_dir,
            logs_dir,
            portable,
        }
    }

    pub fn to_app_config(&self) -> daftar_ui::AppConfig {
        daftar_ui::AppConfig {
            data_dir: self.data_dir.clone(),
            library_root: self.library_root.clone(),
            settings_path: self.settings_path.clone(),
            db_path: self.db_path.clone(),
            fonts_dir: self.fonts_dir.clone(),
            backups_dir: self.backups_dir.clone(),
            portable: self.portable,
        }
    }
}

fn platform_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(appdata).join("Daftar"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join("Daftar"));
        }
        Err("Neither APPDATA nor USERPROFILE is set.".into())
    }
    #[cfg(target_os = "macos")]
    {
        let home = home_dir().ok_or("Cannot determine home directory")?;
        Ok(home
            .join("Library")
            .join("Application Support")
            .join("Daftar"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg).join("daftar"));
            }
        }
        let home = home_dir().ok_or("Cannot determine home directory")?;
        Ok(home.join(".local").join("share").join("daftar"))
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[derive(Debug, Default, Clone)]
pub struct CliOptions {
    pub portable: bool,
    pub data_dir: Option<PathBuf>,
    pub check: bool,
}

pub fn parse_cli() -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--portable" => options.portable = true,
            "--check" => options.check = true,
            "--version" | "-V" => {
                println!("daftar {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "Daftar {}\n\nUsage: daftar [--portable] [--data-dir PATH] [--check]\n\n\
                     --portable        Store data beside the executable\n\
                     --data-dir PATH   Override the data directory\n\
                     --check           Validate paths/database and exit\n",
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            "--data-dir" => {
                let Some(value) = args.next() else {
                    return Err("--data-dir requires a path".into());
                };
                options.data_dir = Some(PathBuf::from(value));
            }
            other => return Err(format!("Unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// Portable archive layout check used by tests/build scripts.
pub fn is_portable_layout(exe_dir: &Path) -> bool {
    exe_dir.join("daftar.portable").exists()
}
