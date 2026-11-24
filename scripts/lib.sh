#!/usr/bin/env bash
# Shared helpers for Daftar release scripts.
#
# shellcheck shell=bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${HOME}/.cargo/bin:${PATH}"

APP_BIN="daftar"
PKG_ID="daftar"
DIST_DIR="${REPO_ROOT}/dist"

info() { printf '==> %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
require_cmd() { command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"; }
require_file() { [[ -e "$1" ]] || die "missing required file: $1"; }

# Single source of truth for the version: the workspace Cargo.toml.
app_version() {
    sed -n 's/^version = "\(.*\)"$/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1
}

# Normalised host architecture used in artifact names.
host_arch() {
    case "$(uname -m)" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
        *) uname -m ;;
    esac
}

# daftar-<version>-<os>-<arch>
artifact_stem() {
    local os="$1" arch="$2"
    echo "${PKG_ID}-$(app_version)-${os}-${arch}"
}

# Files every distribution ships next to the executable.
stage_common() {
    local root="$1" bin="$2"
    install -Dm755 "$bin" "${root}/${APP_BIN}"
    install -Dm644 "${REPO_ROOT}/README.md" "${root}/README.md"
    install -Dm644 "${REPO_ROOT}/LICENSE" "${root}/LICENSE"
    if [[ -f "${REPO_ROOT}/README.fa.md" ]]; then
        install -Dm644 "${REPO_ROOT}/README.fa.md" "${root}/README.fa.md"
    fi
    mkdir -p "${root}/samples" "${root}/assets"
    cp -r "${REPO_ROOT}/samples/." "${root}/samples/"
    cp -r "${REPO_ROOT}/assets/fonts" "${root}/assets/fonts"
    cp -r "${REPO_ROOT}/assets/icons" "${root}/assets/icons"
    install -Dm644 "${REPO_ROOT}/docs/linux/daftar.desktop" "${root}/daftar.desktop"
}

# Write SHA256SUMS for the given files inside `dir` (basenames only).
write_checksums() {
    local dir="$1"
    shift
    (cd "$dir" && sha256sum "$@" > SHA256SUMS)
}

# Smoke-test a staged executable: --version must report the expected version and
# --check must report database integrity, with data kept inside a temp directory.
smoke_test() {
    local bin="$1"
    local tmp
    tmp="$(mktemp -d)"
    local version
    version="$("$bin" --version 2>&1 | head -n1)"
    [[ "$version" == "${APP_BIN} $(app_version)" ]] || die "unexpected --version output: ${version}"
    local check
    check="$(DAFTAR_DATA_DIR="${tmp}" "$bin" --check 2>&1 || true)"
    printf '%s\n' "$check" | grep -q "integrity: ok" || die "--check failed for ${bin}:\n${check}"
    rm -rf "$tmp"
    info "smoke test passed: ${version}"
}

# Verify a portable layout actually resolves data beside the executable.
smoke_test_portable() {
    local bin="$1"
    local out
    out="$("$bin" --check 2>&1 || true)"
    printf '%s\n' "$out" | grep -q "portable: true" || die "portable mode not detected for ${bin}:\n${out}"
    printf '%s\n' "$out" | grep -q "integrity: ok" || die "portable integrity check failed:\n${out}"
    info "portable smoke test passed"
}
