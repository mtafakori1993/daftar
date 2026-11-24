# Build the Daftar release binary on Windows and run quality gates.
#
# Usage: pwsh -File scripts/build-windows.ps1 [-NoTests] [-NoClippy]

[CmdletBinding()]
param(
    [switch]$NoTests,
    [switch]$NoClippy
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $RepoRoot

function Get-AppVersion {
    $line = Select-String -Path (Join-Path $RepoRoot "Cargo.toml") -Pattern '^version = "(.*)"' | Select-Object -First 1
    if (-not $line) { throw "cannot determine version from Cargo.toml" }
    return $line.Matches[0].Groups[1].Value
}

if (-not $NoTests) {
    Write-Host "==> cargo test --workspace"
    cargo test --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
}

if (-not $NoClippy) {
    Write-Host "==> cargo clippy --workspace --all-targets"
    cargo clippy --workspace --all-targets
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }
}

Write-Host "==> cargo build --release -p daftar-app"
cargo build --release -p daftar-app
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$bin = Join-Path $RepoRoot "target\release\daftar.exe"
if (-not (Test-Path $bin)) { throw "missing $bin" }

Write-Host "==> built $bin (version $(Get-AppVersion))"

# Smoke test: --version and --check with an isolated data directory.
$version = & $bin --version
if ($LASTEXITCODE -ne 0 -or $version -notmatch [regex]::Escape((Get-AppVersion))) {
    throw "unexpected --version output: $version"
}
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("daftar-smoke-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $env:DAFTAR_DATA_DIR = $tmp
    $check = & $bin --check 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $check -notmatch "integrity: ok") {
        throw "--check failed:`n$check"
    }
    Write-Host "==> smoke test passed: $version"
}
finally {
    Remove-Item Env:\DAFTAR_DATA_DIR -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
