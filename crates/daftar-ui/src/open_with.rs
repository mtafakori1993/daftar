//! "Open With" — discovering installed applications and launching them with a file.
//!
//! The launch path never goes through a shell: commands are parsed into a program plus
//! argument list (following the Desktop Entry Specification quoting rules on Linux) and
//! executed with [`std::process::Command`], so file names containing spaces, quotes,
//! `&`, `$` or non-ASCII characters are always passed as a single argument. Launch-time
//! failures (missing application, permission denied, non-zero exit) are returned or
//! reported through a shared queue instead of failing silently.

use std::path::{Path, PathBuf};
use std::process::Child;

/// A launchable application offered in the "Open With" dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// Display name.
    pub name: String,
    /// Launcher description: a Linux desktop `Exec` line, a Windows `.lnk`/`.exe` path
    /// or a user-typed command line, or a macOS application bundle path.
    pub exec: String,
    /// Preferred working directory (Linux desktop `Path=` key).
    pub workdir: Option<PathBuf>,
}

// ---------------------------------------------------------------------------------------
// Command-line parsing and field codes (pure, cross-platform, unit-tested)
// ---------------------------------------------------------------------------------------

/// Split a command line into tokens following the Desktop Entry Specification quoting
/// rules: whitespace separates tokens, double quotes group arguments, a backslash
/// escapes the next character and single quotes are tolerated as literal grouping.
pub fn split_exec(exec: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\' | '`' | '$')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => current.push('\\'),
                        },
                        Some(other) => current.push(other),
                        None => return Err("unterminated double quote in command".into()),
                    }
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(other) => current.push(other),
                        None => return Err("unterminated single quote in command".into()),
                    }
                }
            }
            '\\' => {
                started = true;
                // Only the characters listed by the Desktop Entry Specification are
                // escapable outside quotes; anything else keeps its backslash so that
                // Windows-style paths survive intact.
                match chars.peek().copied() {
                    Some('"' | '\\' | '`' | '$') => {
                        current.push(chars.next().expect("peeked char exists"));
                    }
                    _ => current.push('\\'),
                }
            }
            c if c.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                started = true;
                current.push(c);
            }
        }
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Expand desktop field codes in `tokens`, substituting `file` for `%f`, `%F`, `%u` and
/// `%U`, dropping informational codes (`%k`, `%i`, `%d`, `%D`, `%n`, `%N`, `%v`) and
/// appending the file path when the line contains no file field code at all.
pub fn expand_field_codes(tokens: Vec<String>, file: &Path) -> Vec<String> {
    let file_string = file.to_string_lossy().to_string();
    let mut expanded = Vec::with_capacity(tokens.len() + 1);
    let mut substituted = false;
    for token in tokens {
        match token.as_str() {
            "%f" | "%F" | "%u" | "%U" => {
                substituted = true;
                expanded.push(file_string.clone());
            }
            "%k" | "%i" | "%d" | "%D" | "%n" | "%N" | "%v" => {}
            "%%" => expanded.push("%".into()),
            _ => expanded.push(token),
        }
    }
    if !substituted {
        expanded.push(file_string);
    }
    expanded
}

/// Turn a raw launcher string and a target file into `(program, arguments)`.
pub fn plan_command(exec: &str, file: &Path) -> Result<(String, Vec<String>), String> {
    let tokens = split_exec(exec)?;
    let Some(program) = tokens.first() else {
        return Err("the command is empty".into());
    };
    if program.starts_with('%') {
        return Err("the command does not name an application".into());
    }
    let args = expand_field_codes(tokens, file);
    Ok((args[0].clone(), args[1..].to_vec()))
}

// ---------------------------------------------------------------------------------------
// Desktop entry parsing and Linux application directories
// ---------------------------------------------------------------------------------------

/// The interesting subset of a `[Desktop Entry]` group.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: Option<String>,
    pub exec: Option<String>,
    pub path: Option<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub is_application: bool,
}

/// Parse the `[Desktop Entry]` group of a `.desktop` file. Localised keys such as
/// `Name[fa]` are ignored in favour of the canonical `Name`.
pub fn parse_desktop_entry(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut in_header = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_header = line.eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }
        if !in_header {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => entry.is_application = value.eq_ignore_ascii_case("Application"),
            "Name" => entry.name = Some(value.to_string()).filter(|v| !v.is_empty()),
            "Exec" => entry.exec = Some(value.to_string()).filter(|v| !v.is_empty()),
            "Path" => entry.path = Some(value.to_string()).filter(|v| !v.is_empty()),
            "NoDisplay" => entry.no_display = value.eq_ignore_ascii_case("true"),
            "Hidden" => entry.hidden = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    entry
}

/// Whether `name` refers to Daftar itself; re-opening a library file inside Daftar is
/// not useful, so the application is hidden from its own "Open With" list.
fn is_self(name: &str) -> bool {
    name.eq_ignore_ascii_case("daftar")
}

/// Scan one `applications` directory and return `(desktop id, entry)` pairs for every
/// visible application with an executable command line.
pub fn desktop_apps_in_dir(dir: &Path) -> Vec<(String, AppEntry)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for item in entries.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let parsed = parse_desktop_entry(&text);
        let Some(exec) = parsed.exec.filter(|e| !e.trim().is_empty()) else {
            continue;
        };
        if !parsed.is_application || parsed.no_display || parsed.hidden {
            continue;
        }
        let fallback = id.to_string();
        let name = parsed.name.unwrap_or(fallback);
        if is_self(&name) || is_self(id) {
            continue;
        }
        found.push((
            id.to_string(),
            AppEntry {
                name,
                exec,
                workdir: parsed.path.map(PathBuf::from),
            },
        ));
    }
    found
}

/// Merge application lists, keeping the first occurrence per id (XDG precedence) and
/// dropping duplicates by display name.
fn merge_apps(lists: Vec<Vec<(String, AppEntry)>>) -> Vec<AppEntry> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut apps = Vec::new();
    for list in lists {
        for (id, entry) in list {
            let id_key = id.to_lowercase();
            let name_key = entry.name.to_lowercase();
            if !seen_ids.insert(id_key) || !seen_names.insert(name_key) {
                continue;
            }
            apps.push(entry);
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Look a program up on `PATH` (honouring `PATHEXT` on Windows) without spawning a shell.
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return candidate.exists().then(|| candidate.to_path_buf());
    }
    if program.contains('/') || program.contains('\\') {
        let relative = std::env::current_dir().ok()?.join(candidate);
        return relative.exists().then_some(relative);
    }
    let path_var = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|ext| ext.trim().to_ascii_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        let plain = dir.join(program);
        if plain.is_file() {
            return Some(plain);
        }
        for extension in &extensions {
            if extension.is_empty() {
                continue;
            }
            let with_extension = dir.join(format!("{program}{extension}"));
            if with_extension.is_file() {
                return Some(with_extension);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------------------
// Application discovery per platform
// ---------------------------------------------------------------------------------------

/// Discover installed applications, sorted by display name.
pub fn discover() -> Vec<AppEntry> {
    #[cfg(target_os = "linux")]
    {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            if !data_home.is_empty() {
                dirs.push(PathBuf::from(data_home));
            }
        } else if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share"));
        }
        dirs.push(PathBuf::from("/usr/local/share"));
        if let Some(data_dirs) = std::env::var_os("XDG_DATA_DIRS") {
            for dir in std::env::split_paths(&data_dirs) {
                if !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
        } else {
            dirs.push(PathBuf::from("/usr/share"));
        }
        // Flatpak applications export their desktop files into these extra roots.
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/flatpak/exports/share"));
        }
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
        let lists = dirs
            .into_iter()
            .map(|dir| desktop_apps_in_dir(&dir.join("applications")))
            .collect();
        merge_apps(lists)
    }
    #[cfg(target_os = "windows")]
    {
        let mut lists: Vec<Vec<(String, AppEntry)>> = Vec::new();
        let mut menu_dirs: Vec<PathBuf> = Vec::new();
        if let Some(program_data) = std::env::var_os("ProgramData") {
            menu_dirs
                .push(PathBuf::from(program_data).join("Microsoft/Windows/Start Menu/Programs"));
        }
        if let Some(app_data) = std::env::var_os("APPDATA") {
            menu_dirs.push(PathBuf::from(app_data).join("Microsoft/Windows/Start Menu/Programs"));
        }
        lists.push(start_menu_shortcuts(&menu_dirs));
        let mut known: Vec<(String, AppEntry)> = Vec::new();
        for program in ["notepad", "notepad++", "code", "subl", "wordpad"] {
            if let Some(resolved) = find_on_path(program) {
                let name = resolved
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(program)
                    .to_string();
                if is_self(&name) {
                    continue;
                }
                known.push((
                    name.to_lowercase(),
                    AppEntry {
                        name,
                        exec: resolved.to_string_lossy().to_string(),
                        workdir: None,
                    },
                ));
            }
        }
        lists.push(known);
        merge_apps(lists)
    }
    #[cfg(target_os = "macos")]
    {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join("Applications"));
        }
        dirs.push(PathBuf::from("/Applications"));
        dirs.push(PathBuf::from("/System/Applications"));
        let mut list: Vec<(String, AppEntry)> = Vec::new();
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for item in entries.flatten() {
                let path = item.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if is_self(stem) {
                    continue;
                }
                list.push((
                    stem.to_lowercase(),
                    AppEntry {
                        name: stem.to_string(),
                        exec: path.to_string_lossy().to_string(),
                        workdir: None,
                    },
                ));
            }
        }
        merge_apps(vec![list])
    }
}

#[cfg(target_os = "windows")]
fn start_menu_shortcuts(dirs: &[PathBuf]) -> Vec<(String, AppEntry)> {
    use walkdir::WalkDir;
    let mut found: Vec<(String, AppEntry)> = Vec::new();
    for dir in dirs {
        for item in WalkDir::new(dir)
            .follow_links(false)
            .max_depth(6)
            .into_iter()
            .flatten()
        {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lnk") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if is_self(stem) {
                continue;
            }
            found.push((
                stem.to_lowercase(),
                AppEntry {
                    name: stem.to_string(),
                    exec: path.to_string_lossy().to_string(),
                    workdir: None,
                },
            ));
        }
    }
    found
}

// ---------------------------------------------------------------------------------------
// Launching
// ---------------------------------------------------------------------------------------

/// Launch `app` with `file` as its argument and return the child process so callers can
/// observe launch failures. The child is spawned without a shell.
pub fn launch(app: &AppEntry, file: &Path) -> Result<Child, String> {
    let working_dir = preferred_working_dir(app, file);
    #[cfg(target_os = "linux")]
    {
        let (program, args) = plan_command(&app.exec, file)?;
        let mut command = std::process::Command::new(&program);
        command.args(&args);
        if let Some(dir) = &working_dir {
            command.current_dir(dir);
        }
        command
            .spawn()
            .map_err(|error| describe_spawn_error(&program, error))
    }
    #[cfg(target_os = "windows")]
    {
        if app.exec.to_lowercase().ends_with(".lnk") {
            // ShellExecute-through-`start` is the only way to pass a document argument to
            // a shortcut without COM; every dynamic argument is fully quoted and paths
            // cannot contain quotes on Windows, so no metacharacter escapes the quotes.
            let mut command = std::process::Command::new("cmd");
            use std::os::windows::process::CommandExt;
            command.raw_arg(format!(
                "/C start \"\" \"{}\" \"{}\"",
                app.exec,
                file.display()
            ));
            return command
                .spawn()
                .map_err(|error| describe_spawn_error("cmd", error));
        }
        let (program, args) = plan_command(&app.exec, file)?;
        let resolved = find_on_path(&program)
            .ok_or_else(|| format!("application `{program}` was not found"))?;
        let mut command = std::process::Command::new(resolved);
        command.args(&args);
        if let Some(dir) = &working_dir {
            command.current_dir(dir);
        }
        command
            .spawn()
            .map_err(|error| describe_spawn_error(&program, error))
    }
    #[cfg(target_os = "macos")]
    {
        if app.exec.to_lowercase().ends_with(".app") {
            return std::process::Command::new("open")
                .args(["-a", &app.exec])
                .arg(file)
                .spawn()
                .map_err(|error| describe_spawn_error("open", error));
        }
        let (program, args) = plan_command(&app.exec, file)?;
        if Path::new(&program).exists() || find_on_path(&program).is_some() {
            let mut command = std::process::Command::new(&program);
            command.args(&args);
            if let Some(dir) = &working_dir {
                command.current_dir(dir);
            }
            return command
                .spawn()
                .map_err(|error| describe_spawn_error(&program, error));
        }
        // Fall back to LaunchServices, which resolves installed applications by name.
        std::process::Command::new("open")
            .arg("-a")
            .arg(&program)
            .arg(file)
            .spawn()
            .map_err(|error| describe_spawn_error("open", error))
    }
}

/// The directory the launched application starts in: the desktop `Path=` when valid,
/// otherwise the file's parent directory.
fn preferred_working_dir(app: &AppEntry, file: &Path) -> Option<PathBuf> {
    if let Some(dir) = &app.workdir {
        if dir.is_dir() {
            return Some(dir.clone());
        }
    }
    file.parent().map(Path::to_path_buf)
}

fn describe_spawn_error(program: &str, error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => format!("application `{program}` was not found"),
        std::io::ErrorKind::PermissionDenied => {
            format!("permission denied while starting `{program}`")
        }
        _ => format!("could not start `{program}`: {error}"),
    }
}

// ---------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn split_exec_handles_plain_commands() {
        assert_eq!(
            split_exec("code --new-window").unwrap(),
            vec!["code", "--new-window"]
        );
        assert_eq!(split_exec("  gedit  ").unwrap(), vec!["gedit"]);
        assert!(split_exec("").unwrap().is_empty());
    }

    #[test]
    fn split_exec_groups_quoted_arguments() {
        assert_eq!(
            split_exec("\"My App/bin/edit\" -x").unwrap(),
            vec!["My App/bin/edit", "-x"]
        );
        assert_eq!(split_exec("'My App' -x").unwrap(), vec!["My App", "-x"]);
    }

    #[test]
    fn split_exec_unescapes_double_quotes() {
        assert_eq!(
            split_exec("say \"a \\\"quoted\\\" word\"").unwrap(),
            vec!["say", "a \"quoted\" word"]
        );
        assert_eq!(
            split_exec("c:\\\\tools\\\\edit.exe").unwrap(),
            vec!["c:\\tools\\edit.exe"]
        );
    }

    #[test]
    fn split_exec_rejects_unterminated_quotes() {
        assert!(split_exec("\"unterminated").is_err());
        assert!(split_exec("'unterminated").is_err());
    }

    #[test]
    fn field_codes_are_replaced_with_the_file() {
        for code in ["%f", "%F", "%u", "%U"] {
            let path = file("/tmp/a b/متن.md");
            let args = expand_field_codes(vec!["edit".into(), code.into()], &path);
            assert_eq!(
                args,
                vec!["edit".to_string(), path.to_string_lossy().to_string()],
                "code {code}"
            );
        }
    }

    #[test]
    fn file_is_appended_without_field_codes() {
        let path = file("/tmp/some file.md");
        assert_eq!(
            expand_field_codes(vec!["vim".into(), "-f".into()], &path),
            vec!["vim", "-f", "/tmp/some file.md"]
        );
    }

    #[test]
    fn informational_field_codes_are_dropped() {
        let path = file("/tmp/x.md");
        let args = expand_field_codes(
            vec!["app".into(), "%k".into(), "%i".into(), "%v".into()],
            &path,
        );
        assert_eq!(args, vec!["app", "/tmp/x.md"]);
    }

    #[test]
    fn escaped_percent_stays_literal() {
        let args = expand_field_codes(vec!["app".into(), "%%".into()], &file("/tmp/x.md"));
        assert_eq!(args, vec!["app", "%", "/tmp/x.md"]);
    }

    #[test]
    fn plan_command_returns_program_and_arguments() {
        let path = file("/tmp/weird &name€.md");
        let (program, args) = plan_command("term -e \"My Editor\" %f", &path).unwrap();
        assert_eq!(program, "term");
        assert_eq!(
            args,
            vec![
                "-e".to_string(),
                "My Editor".to_string(),
                path.to_string_lossy().to_string()
            ]
        );
    }

    #[test]
    fn plan_command_appends_the_file_by_default() {
        let (program, args) = plan_command("notepad", &file("C:\\Temp\\note.md")).unwrap();
        assert_eq!(program, "notepad");
        assert_eq!(args, vec!["C:\\Temp\\note.md"]);
    }

    #[test]
    fn plan_command_rejects_empty_and_program_less_commands() {
        assert!(plan_command("", &file("a.md")).is_err());
        assert!(plan_command("   ", &file("a.md")).is_err());
        assert!(plan_command("%f", &file("a.md")).is_err());
    }

    #[test]
    fn desktop_entries_are_parsed() {
        let text = "\
[Desktop Entry]
Type=Application
Name=Editor
Name[fa]=ویرایشگر
Exec=/opt/My App/edit %f
Path=/opt
NoDisplay=false
X-Custom=ignored
";
        let entry = parse_desktop_entry(text);
        assert_eq!(entry.name.as_deref(), Some("Editor"));
        assert_eq!(entry.exec.as_deref(), Some("/opt/My App/edit %f"));
        assert_eq!(entry.path.as_deref(), Some("/opt"));
        assert!(entry.is_application);
        assert!(!entry.no_display);
        assert!(!entry.hidden);
    }

    #[test]
    fn hidden_or_non_application_entries_are_detected() {
        let hidden = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nName=H\nExec=x\nNoDisplay=true\n",
        );
        assert!(hidden.no_display);
        let link = parse_desktop_entry("[Desktop Entry]\nType=Link\nName=L\nURL=x\n");
        assert!(!link.is_application);
        assert!(link.exec.is_none());
        let localised_only =
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName[fa]=فارسی\nExec=x %f\n");
        assert_eq!(localised_only.name, None);
    }

    #[test]
    fn discovery_scans_desktop_directories_with_dedupe() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first/applications");
        let second = temp.path().join("second/applications");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(
            first.join("editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=Zed Editor\nExec=zed %f\nPath=/opt/zed\n",
        )
        .unwrap();
        std::fs::write(
            first.join("hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden Tool\nExec=hidden %f\nNoDisplay=true\n",
        )
        .unwrap();
        std::fs::write(
            first.join("link.desktop"),
            "[Desktop Entry]\nType=Link\nName=Just a link\nURL=http://example.com\n",
        )
        .unwrap();
        std::fs::write(
            second.join("editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=Second Editor\nExec=other %f\n",
        )
        .unwrap();
        std::fs::write(
            second.join("notes.desktop"),
            "[Desktop Entry]\nType=Application\nName=Notes\nExec=notes %f\n",
        )
        .unwrap();

        let apps = merge_apps(vec![
            desktop_apps_in_dir(&first),
            desktop_apps_in_dir(&second),
        ]);
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Notes", "Zed Editor"]);
        let zed = &apps[1];
        assert_eq!(zed.exec, "zed %f");
        assert_eq!(zed.workdir, Some(PathBuf::from("/opt/zed")));
    }

    #[test]
    fn self_entries_are_filtered() {
        assert!(is_self("daftar"));
        assert!(is_self("Daftar"));
        assert!(!is_self("daftarpad"));
    }

    #[cfg(unix)]
    #[test]
    fn path_lookup_finds_existing_tools() {
        assert!(find_on_path("sh").is_some());
    }

    #[test]
    fn path_lookup_reports_missing_programs() {
        assert!(find_on_path("definitely-not-a-real-tool-xyz").is_none());
    }
}
