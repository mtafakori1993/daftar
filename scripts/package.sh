#!/usr/bin/env bash
# Build release artifacts for Daftar.
#
# Produces, under dist/:
#   daftar-<version>-<os>-<arch>.tar.gz            standard package
#   daftar-<version>-<os>-<arch>-portable.zip      portable package (daftar.portable marker)
#   SHA256SUMS
#
# Usage: scripts/package.sh [--no-build]
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:${PATH}"

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)"
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
NAME="daftar-${VERSION}-${OS}-${ARCH}"
DIST="dist"
BIN="target/release/daftar"

if [[ "${1:-}" != "--no-build" ]]; then
    echo "==> Building release binary"
    cargo build --release -p daftar-app
fi

if [[ ! -x "${BIN}" ]]; then
    echo "error: ${BIN} is missing; run without --no-build" >&2
    exit 1
fi

rm -rf "${DIST:?}/${NAME}" "${DIST:?}/${NAME}-portable" "${DIST:?}/${NAME}.tar.gz" "${DIST:?}/${NAME}-portable.zip"
mkdir -p "${DIST}/${NAME}" "${DIST}/${NAME}-portable"

stage() {
    local root="$1"
    install -Dm755 "${BIN}" "${root}/daftar"
    install -Dm644 README.md "${root}/README.md"
    install -Dm644 LICENSE "${root}/LICENSE"
    cp -r assets "${root}/assets"
    cp -r samples "${root}/samples"
}

stage "${DIST}/${NAME}"
stage "${DIST}/${NAME}-portable"

# Portable mode: data lives beside the executable.
printf 'Daftar portable mode\n' > "${DIST}/${NAME}-portable/daftar.portable"
mkdir -p "${DIST}/${NAME}-portable/data"

echo "==> Packaging ${NAME}.tar.gz"
tar -C "${DIST}" -czf "${DIST}/${NAME}.tar.gz" "${NAME}"

echo "==> Packaging ${NAME}-portable.zip"
(cd "${DIST}" && zip -qr "${NAME}-portable.zip" "${NAME}-portable")

echo "==> Writing checksums"
(cd "${DIST}" && sha256sum "${NAME}.tar.gz" "${NAME}-portable.zip" > SHA256SUMS)

echo "==> Smoke test"
"${DIST}/${NAME}/daftar" --version
check_output="$("${DIST}/${NAME}/daftar" --check 2>&1 || true)"
printf '%s\n' "${check_output}" | sed -n '1,6p'
printf '%s' "${check_output}" | grep -q "integrity: ok" || { echo "error: integrity check failed" >&2; exit 1; }

echo
echo "Artifacts:"
ls -lh "${DIST}/${NAME}.tar.gz" "${DIST}/${NAME}-portable.zip" "${DIST}/SHA256SUMS"