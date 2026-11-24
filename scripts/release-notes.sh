#!/usr/bin/env bash
# Generate release notes for a Daftar version.
#
# Usage: scripts/release-notes.sh [--version X.Y.Z] [--tag vX.Y.Z] [--out FILE] [--artifacts DIR]
#
# The version defaults to the workspace Cargo.toml version; the tag defaults to v<version>.
set -euo pipefail

# shellcheck source=scripts/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

REPO_URL="https://github.com/mtafakori1993/daftar"
VERSION="$(app_version)"
TAG=""
OUT=""
ARTIFACTS=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) VERSION="${2:-}"; shift ;;
        --tag) TAG="${2:-}"; shift ;;
        --out) OUT="${2:-}"; shift ;;
        --artifacts) ARTIFACTS="${2:-}"; shift ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done
[[ -n "${TAG}" ]] || TAG="v${VERSION}"
[[ -n "${OUT}" ]] || OUT="${REPO_ROOT}/docs/releases/${TAG}.md"

LINUX_STEM="daftar-${VERSION}-linux-x86_64"
WIN_STEM="daftar-${VERSION}-windows-x86_64"
DEB_ASSET="daftar_${VERSION}_amd64.deb"
DOWNLOAD="${REPO_URL}/releases/download/${TAG}"

# Notes for the previous tag, when the repository has one.
CHANGES=""
if git -C "${REPO_ROOT}" rev-parse -q --verify "refs/tags/${TAG}" >/dev/null 2>&1; then
    PREV="$(git -C "${REPO_ROOT}" describe --tags --abbrev=0 "${TAG}^" 2>/dev/null || true)"
    if [[ -n "${PREV}" ]]; then
        CHANGES="$(git -C "${REPO_ROOT}" log --no-merges --pretty='- %s' "${PREV}..${TAG}" || true)"
    fi
fi
if [[ -z "${CHANGES}" ]]; then
    CHANGES="- First public release of Daftar."
fi

mkdir -p "$(dirname "${OUT}")"
cat > "${OUT}" <<EOF
# Daftar ${TAG}

**The World's Best Markdown Reader** — بهترین Markdown Reader جهان

Daftar is a native Rust desktop Markdown reader and knowledge studio: no JavaScript,
WebView, Electron or Tauri, just one compiled binary with first-class Persian/Arabic
right-to-left support.

## What's new

${CHANGES}

## Downloads — Linux x86_64

| File | Description |
| --- | --- |
| [\`${LINUX_STEM}.tar.gz\`](${DOWNLOAD}/${LINUX_STEM}.tar.gz) | Portable TAR.GZ (recommended) |
| [\`${LINUX_STEM}-portable.zip\`](${DOWNLOAD}/${LINUX_STEM}-portable.zip) | Portable ZIP with \`daftar.portable\` marker |
| [\`${DEB_ASSET}\`](${DOWNLOAD}/${DEB_ASSET}) | Debian/Ubuntu package (application menu entry, uninstallable) |
| [\`${LINUX_STEM}.AppImage\`](${DOWNLOAD}/${LINUX_STEM}.AppImage) | AppImage, runs without installation |

## Downloads — Windows x86_64

| File | Description |
| --- | --- |
| [\`${WIN_STEM}-setup.exe\`](${DOWNLOAD}/${WIN_STEM}-setup.exe) | Installer (per-user, no administrator required) |
| [\`${WIN_STEM}-portable.zip\`](${DOWNLOAD}/${WIN_STEM}-portable.zip) | Portable ZIP, no installation needed |
| [\`SHA256SUMS\`](${DOWNLOAD}/SHA256SUMS) | Checksums for every artifact |

## Installation — Linux

**Debian / Ubuntu (DEB)**

\`\`\`bash
sudo apt install ./${DEB_ASSET}
daftar
\`\`\`

The package installs \`/usr/bin/daftar\`, a desktop entry and icons, so Daftar appears in
the GNOME/KDE application menu. Remove it with \`sudo apt remove daftar\`.

**AppImage**

\`\`\`bash
chmod +x ${LINUX_STEM}.AppImage
./${LINUX_STEM}.AppImage
\`\`\`

**Portable TAR.GZ**

\`\`\`bash
tar -xzf ${LINUX_STEM}.tar.gz
./${LINUX_STEM}/daftar
\`\`\`

## Installation — Windows

**Installer (\`${WIN_STEM}-setup.exe\`)**

1. Run the installer (a per-user installation is used, so no administrator rights are required).
2. Choose the installation directory; a Start Menu shortcut and an uninstaller are created.
3. Launch **Daftar** from the Start Menu.

**Portable ZIP (\`${WIN_STEM}-portable.zip\`)**

1. Extract the archive anywhere (USB drive, network share, ...).
2. Run \`daftar.exe\` — no installation and no administrator rights required.

## Portable usage

Portable builds keep everything beside the executable: add a \`daftar.portable\` marker
file next to \`daftar\`/\`daftar.exe\` and the database, settings, fonts and the default
library live in a \`data/\` directory in the same folder. The portable packages already
contain that marker.

\`\`\`bash
# Linux: everything stays inside the extracted folder
./daftar-X.Y.Z-linux-x86_64-portable/daftar
\`\`\`

You can also override the data location explicitly with \`DAFTAR_DATA_DIR\` or the
\`--data-dir <path>\` flag, and force portable mode with \`DAFTAR_PORTABLE=1\` or
\`--portable\`.

## Verify your download

\`\`\`bash
sha256sum -c SHA256SUMS     # Linux
Get-FileHash -Algorithm SHA256 .\\<file>   # Windows PowerShell
\`\`\`

## Known limitations

- Mermaid support is a native subset; unsupported directives show an explicit error
  instead of a fake diagram.
- Mathematical expressions use a TeX-to-Unicode conversion rather than a full MathJax engine.
- PDF export embeds full OFL fonts instead of subsetting them (larger files, correct shaping).
- Table text is rendered, but text selection inside tables is not supported.
- The AppImage relies on the system's X11/Wayland/OpenGL libraries; there is no bundled
  runtime beyond the application itself.

## License

MIT — see [LICENSE](${REPO_URL}/blob/main/LICENSE). Bundled fonts: Inter, Vazirmatn and
JetBrains Mono under the SIL Open Font License.
EOF

info "wrote ${OUT}"

if [[ -n "${ARTIFACTS}" ]]; then
    missing=0
    for asset in "${LINUX_STEM}.tar.gz" "${LINUX_STEM}-portable.zip" "${DEB_ASSET}" \
                 "${LINUX_STEM}.AppImage" "${WIN_STEM}-portable.zip" "${WIN_STEM}-setup.exe"; do
        [[ -f "${ARTIFACTS}/${asset}" ]] || { info "note: ${asset} not present in ${ARTIFACTS} (documented only)"; missing=$((missing + 1)); }
    done
    [[ "${missing}" == 0 ]] || info "${missing} artifact(s) not built locally; they are produced by the release workflow"
fi
