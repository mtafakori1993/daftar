#!/usr/bin/env bash
# Build the Daftar release binary for Linux and run quality gates.
#
# Usage: scripts/build-linux.sh [--no-tests] [--no-clippy] [--no-strip] [--target <triple>]
set -euo pipefail

# shellcheck source=scripts/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

RUN_TESTS=1
RUN_CLIPPY=1
STRIP=1
TARGET=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-tests) RUN_TESTS=0 ;;
        --no-clippy) RUN_CLIPPY=0 ;;
        --no-strip) STRIP=0 ;;
        --target) TARGET="${2:-}"; shift ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

cd "${REPO_ROOT}"

if [[ "${RUN_TESTS}" == 1 ]]; then
    info "cargo test --workspace"
    cargo test --workspace
fi

if [[ "${RUN_CLIPPY}" == 1 ]]; then
    info "cargo clippy --workspace --all-targets"
    # Warnings are reported but do not fail the build; clippy errors do.
    cargo clippy --workspace --all-targets
fi

info "cargo build --release -p daftar-app"
if [[ -n "${TARGET}" ]]; then
    cargo build --release -p daftar-app --target "${TARGET}"
    BIN="${REPO_ROOT}/target/${TARGET}/release/${APP_BIN}"
else
    cargo build --release -p daftar-app
    BIN="${REPO_ROOT}/target/release/${APP_BIN}"
fi

require_file "${BIN}"
if [[ "${STRIP}" == 1 ]] && ! [[ "${TARGET}" == *"-windows-"* ]]; then
    info "stripping symbols"
    strip --strip-unneeded "${BIN}" 2>/dev/null || info "strip unavailable, keeping symbols"
fi

info "built ${BIN} ($(app_version))"
