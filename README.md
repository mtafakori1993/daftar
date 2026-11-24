# Daftar

A native Rust desktop Markdown reader and knowledge studio. No JavaScript, WebView,
Electron or Tauri: a single compiled binary built on egui/eframe and cosmic-text.

## Features

- **Reader**: headings, tables, task lists, footnotes, code highlighting (syntect),
  images (raster and SVG), quotes and callouts, reading progress and time remaining.
- **RTL-first**: Persian/Arabic shaping with ligatures, Unicode bidi, automatic direction
  detection per document, and mixed-direction documents. Text selection, highlights and
  bookmarks use source-anchored ranges that survive file edits.
- **Diagrams**: native Mermaid subset — flowchart/graph, sequence, class, state, ER,
  gantt, pie, mindmap and journey — with pan/zoom and PNG export.
- **Knowledge**: SQLite/FTS5 library with collections, categories, tags, full-text search,
  highlights, bookmarks, flashcards and study roadmaps.
- **Export**: PDF (embedded TrueType fonts with ToUnicode), PNG cards, batch export, and
  versioned library archives (ZIP with SQLite snapshot and manifest).
- **Interface languages**: English and Persian (فارسی) with a live switcher in Settings.
- **Portable mode**: run from a USB drive with all data beside the executable.

## Build

Requires Rust 1.85+ and a C toolchain (SQLite is bundled).

```bash
cargo build --release -p daftar-app
./target/release/daftar --version
./target/release/daftar --check     # data directory + font integrity report
```

Run the test suite and lints:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Run

```bash
./target/release/daftar
# optional overrides
DAFTAR_DATA_DIR=/path/to/data ./target/release/daftar
DAFTAR_PORTABLE=1 ./target/release/daftar
```

Data lives in the platform data directory (`~/.local/share/daftar` on Linux,
`%APPDATA%\Daftar` on Windows, `~/Library/Application Support/Daftar` on macOS) unless
portable mode is selected.

## Portable packages

A `daftar.portable` marker next to the executable enables portable mode permanently: the
database, settings, imported fonts and the default library live in `data/` beside the
binary. `scripts/package.sh` builds the release binary and produces:

- `dist/daftar-<version>-linux-x86_64.tar.gz` — standard package
- `dist/daftar-<version>-linux-x86_64-portable.zip` — portable package with marker
- `dist/SHA256SUMS`

## Keyboard shortcuts

- `Ctrl+O` open file, `Ctrl+Shift+O` add folder, `Ctrl+K` command palette
- `Ctrl+F` search, `Ctrl+B` toggle library panel, `Ctrl+Shift+B` toggle outline
- `Ctrl+Shift+H` highlight selection, `Ctrl+D` bookmark, `Ctrl+Shift+C` flashcard
- `Ctrl+Shift+R` cycle document direction, `Ctrl+Shift+T` toggle theme
- Study: `Space` flip, `→`/`←` next/previous, `L` learned

Shortcuts are configurable in Settings → Keyboard shortcuts.

## Interface language

Settings → Language switches between English and فارسی immediately and persists to
`settings.toml`. Markdown content direction is independent of the interface language.

## License

MIT — see `LICENSE`. Bundled fonts: Inter, Vazirmatn and JetBrains Mono under the SIL
Open Font License; see `assets/fonts/*-LICENSE.txt` and `assets/fonts/*-OFL.txt`.