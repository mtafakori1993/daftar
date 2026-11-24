# Package Daftar for Windows: portable ZIP, Inno Setup installer and optional MSI.
#
# Usage: pwsh -File scripts/package-windows.ps1 [-NoBuild] [-PortableOnly]
#
# Produces, under dist\:
#   daftar-<version>-windows-x86_64-portable.zip
#   daftar-<version>-windows-x86_64-setup.exe
#   daftar-<version>-windows-x86_64.msi          (only when WiX is available)
#   SHA256SUMS

[CmdletBinding()]
param(
    [switch]$NoBuild,
    [switch]$PortableOnly,
    # Forwarded to build-windows.ps1 so the release workflow can skip the gates it already ran.
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

$Version = Get-AppVersion
$Arch = "x86_64"
$Stem = "daftar-$Version-windows-$Arch"
$Dist = Join-Path $RepoRoot "dist"
$Bin = Join-Path $RepoRoot "target\release\daftar.exe"

if (-not $NoBuild) {
    $buildArgs = @()
    if ($NoTests) { $buildArgs += "-NoTests" }
    if ($NoClippy) { $buildArgs += "-NoClippy" }
    & (Join-Path $RepoRoot "scripts\build-windows.ps1") @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "build-windows.ps1 failed" }
}
if (-not (Test-Path $Bin)) { throw "missing $Bin" }

$stage = Join-Path $Dist $Stem
$portable = Join-Path $Dist "$Stem-portable"
Remove-Item -Recurse -Force $stage, $portable -ErrorAction SilentlyContinue
Remove-Item -Force (Join-Path $Dist "$Stem-portable.zip") -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage, $portable | Out-Null

function Copy-Payload {
    param([string]$Root)
    Copy-Item $Bin (Join-Path $Root "daftar.exe")
    Copy-Item (Join-Path $RepoRoot "README.md") $Root
    Copy-Item (Join-Path $RepoRoot "LICENSE") $Root
    $faReadme = Join-Path $RepoRoot "README.fa.md"
    if (Test-Path $faReadme) { Copy-Item $faReadme $Root }
    Copy-Item (Join-Path $RepoRoot "samples") $Root -Recurse
    Copy-Item (Join-Path $RepoRoot "assets") $Root -Recurse
}

Copy-Payload $stage
Copy-Payload $portable
# Portable mode: data lives next to the executable, no installation or admin rights.
Set-Content -Path (Join-Path $portable "daftar.portable") -Value "Daftar portable mode`n" -NoNewline
New-Item -ItemType Directory -Force -Path (Join-Path $portable "data") | Out-Null

Write-Host "==> packaging $Stem-portable.zip"
# Archive the folder itself so the ZIP expands into daftar-<version>-windows-x86_64-portable/
Compress-Archive -Path $portable -DestinationPath (Join-Path $Dist "$Stem-portable.zip") -Force

$artifacts = @("$Stem-portable.zip")

if (-not $PortableOnly) {
    $iscc = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    if (-not $iscc) {
        $cmd = Get-Command iscc.exe -ErrorAction SilentlyContinue
        if ($cmd) { $iscc = $cmd.Source }
    }

    if ($iscc) {
        Write-Host "==> building installer with Inno Setup ($iscc)"
        & $iscc "/DMyAppVersion=$Version" "/DSourceDir=$stage" "/DOutputDir=$Dist" `
            (Join-Path $RepoRoot "docs\windows\daftar.iss")
        if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
        $artifacts += "$Stem-setup.exe"
    }
    else {
        Write-Host "==> Inno Setup not found; installer skipped"
    }

    # Optional MSI via WiX (only when the toolchain is present).
    $wix = Get-Command wix.exe -ErrorAction SilentlyContinue
    if ($wix) {
        Write-Host "==> building MSI with WiX"
        $wxs = Join-Path $Dist "daftar.wxs"
        $componentRefs = @()
        $fileElements = @()
        $id = 1
        Get-ChildItem -Recurse -File $stage | ForEach-Object {
            $rel = $_.FullName.Substring($stage.Length + 1)
            $componentId = "cmp$id"
            $componentRefs += "<ComponentRef Id=`"$componentId`" />"
            $fileElements += "<Component Id=`"$componentId`" Guid=`"*`"><File Id=`"f$id`" Source=`"$($_.FullName)`" KeyPath=`"yes`" /></Component>"
            $id++
        }
        $wxsContent = @"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="Daftar" Manufacturer="Daftar contributors" Version="$Version" UpgradeCode="b0f2a5c4-77c1-4b47-9c2f-2a9b7c9e1c11" Scope="perUser">
    <MajorUpgrade DowngradeErrorMessage="A newer version of Daftar is already installed." />
    <StandardDirectory Id="ProgramMenuFolder">
      <Component Id="StartMenuShortcut" Guid="*">
        <Shortcut Id="DaftarShortcut" Name="Daftar" Target="[INSTALLFOLDER]daftar.exe" WorkingDirectory="INSTALLFOLDER" Description="The World's Best Markdown Reader" />
      </Component>
    </StandardDirectory>
    <StandardDirectory Id="ProgramFiles6432Folder">
      <Directory Id="INSTALLFOLDER" Name="Daftar">
        $($fileElements -join "`n        ")
      </Directory>
    </StandardDirectory>
  </Package>
</Wix>
"@
        Set-Content -Path $wxs -Value $wxsContent
        & $wix.Source build $wxs -o (Join-Path $Dist "$Stem.msi")
        if ($LASTEXITCODE -eq 0) {
            $artifacts += "$Stem.msi"
        }
        else {
            Write-Host "==> WiX build failed; MSI skipped"
        }
    }
}

Write-Host "==> writing checksums"
$lines = foreach ($a in $artifacts) {
    $path = Join-Path $Dist $a
    if (Test-Path $path) {
        $hash = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
        "$hash  $a"
    }
}
Set-Content -Path (Join-Path $Dist "SHA256SUMS") -Value ($lines -join "`n")

Write-Host "==> artifacts:"
foreach ($a in $artifacts) {
    $path = Join-Path $Dist $a
    if (Test-Path $path) {
        $item = Get-Item $path
        Write-Host ("    {0} ({1:N1} MB)" -f $item.Name, ($item.Length / 1MB))
    }
}
