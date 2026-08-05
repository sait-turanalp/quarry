# quarry installer for Windows
# Usage: irm https://quarry.dev/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "bartolli/codanna"
$InstallDir = if ($env:QUARRY_INSTALL_DIR) { $env:QUARRY_INSTALL_DIR } else { "$env:USERPROFILE\.local\bin" }

function Say($msg) { Write-Host "quarry: $msg" }
function Err($msg) { Write-Host "quarry: ERROR: $msg" -ForegroundColor Red; exit 1 }

# Detect platform
function Get-Platform {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64" { return "windows-x64" }
        default { Err "unsupported architecture: $arch (only x64 is supported)" }
    }
}

# Get latest release tag
function Get-LatestVersion {
    $response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    return $response.tag_name
}

# Main
function Main {
    $platform = Get-Platform
    $version = if ($env:QUARRY_VERSION) { $env:QUARRY_VERSION } else { Get-LatestVersion }

    Say "installing quarry $version for $platform"

    # Fetch manifest
    $manifestUrl = "https://github.com/$Repo/releases/download/$version/dist-manifest.json"
    try {
        $manifest = Invoke-RestMethod -Uri $manifestUrl
    } catch {
        Err "failed to fetch manifest from $manifestUrl"
    }

    # Find matching artifact
    $artifact = $manifest.artifacts | Where-Object {
        $_.platform -eq $platform
    } | Select-Object -First 1

    if (-not $artifact) {
        Err "no artifact found for $platform"
    }

    $url = $artifact.url
    $sha256 = $artifact.sha256
    $filename = $artifact.name

    # Download
    $tmpDir = New-Item -ItemType Directory -Path "$env:TEMP\quarry_install_$([System.Guid]::NewGuid().ToString('N').Substring(0,8))"
    $downloadPath = Join-Path $tmpDir $filename

    Say "downloading $filename"
    try {
        Invoke-WebRequest -Uri $url -OutFile $downloadPath -UseBasicParsing
    } catch {
        Err "download failed: $_"
    }

    # Verify checksum
    Say "verifying checksum"
    $actualHash = (Get-FileHash -Path $downloadPath -Algorithm SHA256).Hash.ToLower()
    if ($actualHash -ne $sha256) {
        Err "checksum mismatch: expected $sha256, got $actualHash"
    }

    # Extract
    Say "extracting"
    $extractDir = Join-Path $tmpDir "extracted"
    Expand-Archive -Path $downloadPath -DestinationPath $extractDir -Force

    # Install
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $binary = Get-ChildItem -Path $extractDir -Recurse -Filter "quarry.exe" | Select-Object -First 1
    if (-not $binary) {
        Err "quarry.exe not found in archive"
    }

    Copy-Item -Path $binary.FullName -Destination $InstallDir -Force
    Say "installed to $InstallDir\quarry.exe"

    # Cleanup
    Remove-Item -Path $tmpDir -Recurse -Force

    # PATH check
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($currentPath -notlike "*$InstallDir*") {
        Say ""
        Say "To add quarry to your PATH, run:"
        Say ""
        Say "  `$env:PATH = `"$InstallDir;`$env:PATH`""
        Say ""
        Say "Or permanently (requires restart):"
        Say ""
        Say "  [Environment]::SetEnvironmentVariable('PATH', `"$InstallDir;`$env:PATH`", 'User')"
    }
}

Main
