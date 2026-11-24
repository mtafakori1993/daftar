#!/usr/bin/env bash
# Package Daftar for Linux: portable TAR.GZ, portable ZIP, DEB and AppImage.
#
# Usage: scripts/package-linux.sh [--no-build] [--require-appimage]
#
# Produces, under dist/:
#   daftar-<version>-linux-<arch>.tar.gz
#   daftar-<version>-linux-<arch>-portable.zip
#   daftar_<version>_<debarch>.deb
#   daftar-<version>-linux-<arch>.AppImage
#   SHA256SUMS
set -euo pipefail

# shellcheck source=scripts/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

BUILD=1
REQUIRE_APPIMAGE=0
BUILD_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-build) BUILD=0 ;;
        --require-appimage) REQUIRE_APPIMAGE=1 ;;
        --no-tests) BUILD_ARGS+=(--no-tests) ;;
        --no-clippy) BUILD_ARGS+=(--no-clippy) ;;
        --no-strip) BUILD_ARGS+=(--no-strip) ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

if [[ "${BUILD}" == 1 ]]; then
    "${REPO_ROOT}/scripts/build-linux.sh" ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}
fi

VERSION="$(app_version)"
ARCH="$(host_arch)"
STEM="$(artifact_stem linux "${ARCH}")"
BIN="${REPO_ROOT}/target/release/${APP_BIN}"
require_file "${BIN}"
require_file "${REPO_ROOT}/docs/linux/daftar.desktop"
require_file "${REPO_ROOT}/docs/linux/io.github.mtafakori1993.Daftar.metainfo.xml"
require_file "${REPO_ROOT}/assets/icons/daftar-256.png"
require_file "${REPO_ROOT}/assets/icons/daftar-512.png"

mkdir -p "${DIST_DIR}"
rm -rf "${DIST_DIR:?}/${STEM}" "${DIST_DIR:?}/${STEM}-portable" \
       "${DIST_DIR:?}/${STEM}.tar.gz" "${DIST_DIR:?}/${STEM}-portable.zip" \
       "${DIST_DIR:?}/${STEM}.AppImage" "${DIST_DIR:?}/${STEM}.AppDir" \
       "${DIST_DIR:?}/deb-root" "${DIST_DIR:?}/daftar_${VERSION}_"*.deb

info "staging portable tree"
stage_common "${DIST_DIR}/${STEM}" "${BIN}"

info "staging portable tree (self-contained data directory)"
stage_common "${DIST_DIR}/${STEM}-portable" "${BIN}"
printf 'Daftar portable mode\n' > "${DIST_DIR}/${STEM}-portable/daftar.portable"
mkdir -p "${DIST_DIR}/${STEM}-portable/data"

info "packaging ${STEM}.tar.gz"
tar -C "${DIST_DIR}" -czf "${DIST_DIR}/${STEM}.tar.gz" "${STEM}"
TAR_LISTING="$(tar -tzf "${DIST_DIR}/${STEM}.tar.gz")"
grep -q "${STEM}/${APP_BIN}$" <<<"${TAR_LISTING}" || die "tar.gz missing executable"

info "packaging ${STEM}-portable.zip"
(cd "${DIST_DIR}" && zip -qr "${STEM}-portable.zip" "${STEM}-portable")
ZIP_LISTING="$(unzip -l "${DIST_DIR}/${STEM}-portable.zip")"
grep -q "daftar.portable" <<<"${ZIP_LISTING}" || die "portable zip missing daftar.portable marker"

# ---------------------------------------------------------------------------- DEB
debarch() {
    case "$1" in
        x86_64) echo "amd64" ;;
        aarch64) echo "arm64" ;;
        *) echo "$1" ;;
    esac
}
DEBARCH="$(debarch "${ARCH}")"
DEB_NAME="${PKG_ID}_${VERSION}_${DEBARCH}.deb"
DEB_ROOT="${DIST_DIR}/deb-root"
info "packaging ${DEB_NAME}"
rm -rf "${DEB_ROOT}"
mkdir -p "${DEB_ROOT}/DEBIAN" \
         "${DEB_ROOT}/usr/bin" \
         "${DEB_ROOT}/usr/share/applications" \
         "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps" \
         "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps" \
         "${DEB_ROOT}/usr/share/metainfo" \
         "${DEB_ROOT}/usr/share/doc/${PKG_ID}"
install -Dm755 "${BIN}" "${DEB_ROOT}/usr/bin/${APP_BIN}"
install -Dm644 "${REPO_ROOT}/docs/linux/daftar.desktop" "${DEB_ROOT}/usr/share/applications/daftar.desktop"
install -Dm644 "${REPO_ROOT}/docs/linux/io.github.mtafakori1993.Daftar.metainfo.xml" \
    "${DEB_ROOT}/usr/share/metainfo/io.github.mtafakori1993.Daftar.metainfo.xml"
install -Dm644 "${REPO_ROOT}/assets/icons/daftar-256.png" "${DEB_ROOT}/usr/share/icons/hicolor/256x256/apps/daftar.png"
install -Dm644 "${REPO_ROOT}/assets/icons/daftar-512.png" "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps/daftar.png"
install -Dm644 "${REPO_ROOT}/README.md" "${DEB_ROOT}/usr/share/doc/${PKG_ID}/README.md"
install -Dm644 "${REPO_ROOT}/LICENSE" "${DEB_ROOT}/usr/share/doc/${PKG_ID}/copyright"
if [[ -f "${REPO_ROOT}/README.fa.md" ]]; then
    install -Dm644 "${REPO_ROOT}/README.fa.md" "${DEB_ROOT}/usr/share/doc/${PKG_ID}/README.fa.md"
fi
cp -r "${REPO_ROOT}/samples" "${DEB_ROOT}/usr/share/doc/${PKG_ID}/samples"

cat > "${DEB_ROOT}/DEBIAN/control" <<EOF
Package: ${PKG_ID}
Version: ${VERSION}
Section: editors
Priority: optional
Architecture: ${DEBARCH}
Maintainer: Daftar contributors <daftar@example.invalid>
Installed-Size: $(du -sk "${DEB_ROOT}/usr" | cut -f1)
Depends: libc6, libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon0, libgl1, libegl1
Description: The World's Best Markdown Reader
 Daftar is a native Rust desktop Markdown reader and knowledge studio with
 first-class right-to-left support for Persian and Arabic, Mermaid diagrams,
 SQLite/FTS5 knowledge library, PDF/PNG export and portable mode.
EOF

cat > "${DEB_ROOT}/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
exit 0
EOF
chmod 755 "${DEB_ROOT}/DEBIAN/postinst"
cat > "${DEB_ROOT}/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
exit 0
EOF
chmod 755 "${DEB_ROOT}/DEBIAN/prerm"

dpkg-deb --build --root-owner-group "${DEB_ROOT}" "${DIST_DIR}/${DEB_NAME}" >/dev/null
rm -rf "${DEB_ROOT}"
require_file "${DIST_DIR}/${DEB_NAME}"

# ------------------------------------------------------------------------ AppImage
APPIMAGE_NAME="${STEM}.AppImage"
APPDIR="${DIST_DIR}/${STEM}.AppDir"
appimagetool_for() {
    local arch="$1" cache="${REPO_ROOT}/.cache"
    mkdir -p "${cache}"
    local tool="${cache}/appimagetool-${arch}.AppImage"
    if [[ ! -x "${tool}" ]]; then
        local url="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${arch}.AppImage"
        info "downloading appimagetool (${arch})"
        curl -fL --retry 3 --connect-timeout 20 -o "${tool}" "${url}" || return 1
        chmod +x "${tool}"
    fi
    echo "${tool}"
}

make_appimage() {
    local tool
    tool="$(appimagetool_for "${ARCH}")" || return 1
    rm -rf "${APPDIR}"
    mkdir -p "${APPDIR}/usr/bin" "${APPDIR}/usr/share/applications" \
             "${APPDIR}/usr/share/icons/hicolor/512x512/apps" \
             "${APPDIR}/usr/share/metainfo"
    install -Dm755 "${BIN}" "${APPDIR}/usr/bin/${APP_BIN}"
    install -Dm644 "${REPO_ROOT}/docs/linux/daftar.desktop" "${APPDIR}/daftar.desktop"
    install -Dm644 "${REPO_ROOT}/docs/linux/daftar.desktop" "${APPDIR}/usr/share/applications/daftar.desktop"
    install -Dm644 "${REPO_ROOT}/docs/linux/io.github.mtafakori1993.Daftar.metainfo.xml" \
        "${APPDIR}/usr/share/metainfo/io.github.mtafakori1993.Daftar.metainfo.xml"
    install -Dm644 "${REPO_ROOT}/assets/icons/daftar-512.png" "${APPDIR}/daftar.png"
    install -Dm644 "${REPO_ROOT}/assets/icons/daftar-512.png" "${APPDIR}/usr/share/icons/hicolor/512x512/apps/daftar.png"
    ln -sf usr/bin/daftar "${APPDIR}/AppRun"
    (cd "${DIST_DIR}" && ARCH="${ARCH}" VERSION="${VERSION}" "${tool}" --appimage-extract-and-run \
        "${STEM}.AppDir" "${APPIMAGE_NAME}") || return 1
    [[ -s "${DIST_DIR}/${APPIMAGE_NAME}" ]] || return 1
}

if make_appimage; then
    info "packaged ${APPIMAGE_NAME}"
else
    if [[ "${REQUIRE_APPIMAGE}" == 1 ]]; then
        die "AppImage generation failed but --require-appimage was requested"
    fi
    info "AppImage skipped (appimagetool unavailable); tar.gz/zip/deb are complete"
fi

# ------------------------------------------------------------------------ validation
info "validating artifacts"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "${REPO_ROOT}/docs/linux/daftar.desktop" || die "desktop entry is invalid"
fi
if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net \
        "${REPO_ROOT}/docs/linux/io.github.mtafakori1993.Daftar.metainfo.xml" >/dev/null \
        || info "appstream validation reported issues (non-fatal)"
fi
DEB_INFO="$(dpkg-deb --info "${DIST_DIR}/${DEB_NAME}")"
grep -q "Package: ${PKG_ID}" <<<"${DEB_INFO}" || die "deb metadata missing"
DEB_FILES="$(dpkg-deb --contents "${DIST_DIR}/${DEB_NAME}")"
grep -q "usr/bin/${APP_BIN}" <<<"${DEB_FILES}" || die "deb missing executable"
[[ "$(dpkg-deb --field "${DIST_DIR}/${DEB_NAME}" Architecture)" == "${DEBARCH}" ]] || die "deb architecture mismatch"

smoke_test "${DIST_DIR}/${STEM}/${APP_BIN}"
smoke_test_portable "${DIST_DIR}/${STEM}-portable/${APP_BIN}"
if [[ -f "${DIST_DIR}/${APPIMAGE_NAME}" ]]; then
    smoke_test "${DIST_DIR}/${STEM}.AppDir/usr/bin/${APP_BIN}"
fi

shopt -s nullglob
CHECK_FILES=("${STEM}.tar.gz" "${STEM}-portable.zip" "${DEB_NAME}")
if [[ -f "${DIST_DIR}/${APPIMAGE_NAME}" ]]; then
    CHECK_FILES+=("${APPIMAGE_NAME}")
fi
shopt -u nullglob
write_checksums "${DIST_DIR}" "${CHECK_FILES[@]}"

info "artifacts:"
shopt -s nullglob
for artifact in "${DIST_DIR}/${STEM}.tar.gz" "${DIST_DIR}/${STEM}-portable.zip" \
                "${DIST_DIR}/${DEB_NAME}" "${DIST_DIR}/${APPIMAGE_NAME}"; do
    [[ -f "${artifact}" ]] && ls -lh "${artifact}"
done
shopt -u nullglob
info "checksums written to ${DIST_DIR}/SHA256SUMS"
