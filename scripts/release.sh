#!/usr/bin/env bash
# Prepare and publish a Daftar release.
#
# Safety rules (see the release documentation):
#   * refuses to run on a dirty git working tree,
#   * refuses to move an existing tag or overwrite an existing release,
#   * never force-pushes,
#   * runs tests and clippy before tagging,
#   * builds and validates the Linux artifacts locally.
#
# Usage:
#   scripts/release.sh                      # full local release: checks, tag, push (CI publishes)
#   scripts/release.sh --dry-run            # checks + local packaging, no tag/push
#   scripts/release.sh --no-push            # create the tag locally, do not push
#   scripts/release.sh --skip-package       # skip local artifact build (CI builds/publishes)
#   scripts/release.sh --version 0.2.0      # override the version (must match Cargo.toml)
set -euo pipefail

# shellcheck source=scripts/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

DRY_RUN=0
PUSH=1
DO_PACKAGE=1
VERSION_OVERRIDE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=1; PUSH=0 ;;
        --no-push) PUSH=0 ;;
        --skip-package) DO_PACKAGE=0 ;;
        --version) VERSION_OVERRIDE="${2:-}"; shift ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

cd "${REPO_ROOT}"
require_cmd git

VERSION="$(app_version)"
TAG="v${VERSION}"
if [[ -n "${VERSION_OVERRIDE}" && "${VERSION_OVERRIDE}" != "${VERSION}" ]]; then
    die "requested version ${VERSION_OVERRIDE} does not match Cargo.toml version ${VERSION}"
fi
info "releasing ${TAG} (version from Cargo.toml)"

# ---------------------------------------------------------------- pre-flight checks
if ! git diff-index --quiet HEAD --; then
    git status --short
    die "git working tree is dirty; commit or stash your changes before releasing"
fi
if [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
    git status --short
    die "untracked files present; commit or ignore them before releasing"
fi

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [[ "${BRANCH}" != "main" && "${BRANCH}" != "master" ]]; then
    info "warning: releasing from branch '${BRANCH}'"
fi

if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null 2>&1; then
    die "tag ${TAG} already exists; bump the version instead of moving an existing tag"
fi

if command -v gh >/dev/null 2>&1 && git remote get-url origin >/dev/null 2>&1; then
    if gh release view "${TAG}" >/dev/null 2>&1; then
        die "GitHub release ${TAG} already exists; refusing to overwrite it"
    fi
fi

# -------------------------------------------------------------------- quality gates
info "cargo fmt --check (informational)"
cargo fmt --all -- --check || info "rustfmt reported differences (not fatal)"

info "cargo test --workspace"
cargo test --workspace

info "cargo clippy --workspace --all-targets"
cargo clippy --workspace --all-targets

# ------------------------------------------------------------------- local artifacts
if [[ "${DO_PACKAGE}" == 1 ]]; then
    info "building and packaging Linux artifacts"
    "${REPO_ROOT}/scripts/package-linux.sh" --no-build
fi

info "generating release notes"
"${REPO_ROOT}/scripts/release-notes.sh" --version "${VERSION}" --tag "${TAG}" \
    --artifacts "${DIST_DIR}" --out "${REPO_ROOT}/docs/releases/${TAG}.md"

if [[ "${DRY_RUN}" == 1 ]]; then
    info "dry run: skipping commit, tag and push"
    info "notes: docs/releases/${TAG}.md"
    info "artifacts: ${DIST_DIR}"
    exit 0
fi

# ------------------------------------------------------------------------ tag & push
info "creating annotated tag ${TAG}"
git tag -a "${TAG}" -m "Daftar ${TAG} — The World's Best Markdown Reader"

if [[ "${PUSH}" == 1 ]]; then
    require_cmd git
    info "pushing branch and tag (this triggers the release workflow)"
    # Never force-push.
    git push origin "${BRANCH}"
    git push origin "${TAG}"
    info "pushed ${TAG}; GitHub Actions will build Windows and Linux artifacts and publish the release"
    if command -v gh >/dev/null 2>&1; then
        gh run list --workflow release.yml --limit 3 || true
    fi
else
    info "tag ${TAG} created locally; push it with: git push origin ${TAG}"
fi
