<#
.SYNOPSIS
    Install the Actionbook CLI binary on Windows.

.DESCRIPTION
    Downloads the Actionbook CLI binary from GitHub Releases, verifies its
    SHA-256 checksum, and installs it to the specified directory.

.PARAMETER Version
    Install a specific version (e.g. 0.8.1 or v0.8.1). Defaults to latest.

.PARAMETER BinDir
    Directory to install the binary into.
    Defaults to $env:USERPROFILE\.actionbook\bin

.EXAMPLE
    irm https://raw.githubusercontent.com/actionbook/actionbook/main/install.ps1 | iex

.EXAMPLE
    & .\install.ps1 -Version 0.8.1 -BinDir C:\tools
#>

[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$BinDir = ""
)

$ErrorActionPreference = "Stop"

$Repo = "actionbook/actionbook"
$BinaryName = "actionbook.exe"

# On Windows PowerShell 5.1, Invoke-WebRequest defaults to the IE parser; pass
# -UseBasicParsing to avoid that dependency.  On pwsh 6+ the flag is accepted
# but unnecessary.  Build a reusable splat so every call stays compatible.
$WebRequestParams = @{}
if ($PSVersionTable.PSVersion.Major -le 5) {
    $WebRequestParams["UseBasicParsing"] = $true
}

# ── Defaults ──────────────────────────────────────────────────────────────────
if (-not $BinDir) {
    $BinDir = Join-Path $env:USERPROFILE ".actionbook\bin"
}

# ── Helpers ───────────────────────────────────────────────────────────────────
function Write-Info($msg) { Write-Host "  [info]  $msg" }
function Write-Err($msg) { Write-Host "  [error] $msg" -ForegroundColor Red; exit 1 }

# ── Detect architecture ──────────────────────────────────────────────────────
function Get-Platform {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64"   { return "win32-x64" }
        "Arm64" { return "win32-arm64" }
        default { Write-Err "Unsupported architecture: $arch. Only x64 and arm64 are supported." }
    }
}

# ── Resolve version ──────────────────────────────────────────────────────────
function Resolve-Version {
    if ($Version) {
        # Strip leading 'v' if present
        $script:Version = $Version -replace '^v', ''
    } else {
        Write-Info "Fetching latest release version..."
        # Paginate up to 5 pages (100 per page) to find the latest CLI release
        # in case non-CLI releases push it off the first page.
        $latest = $null
        for ($page = 1; $page -le 5 -and -not $latest; $page++) {
            try {
                $releases = Invoke-RestMethod `
                    -Uri "https://api.github.com/repos/$Repo/releases?per_page=100&page=$page" `
                    -Headers @{ "User-Agent" = "actionbook-installer" }
            } catch {
                Write-Err "Failed to fetch releases from GitHub: $_"
            }
            if (-not $releases -or $releases.Count -eq 0) { break }
            $latest = $releases | Where-Object { $_.tag_name -match '^actionbook-cli-v' } | Select-Object -First 1
        }
        if (-not $latest) {
            Write-Err "Could not determine latest release version."
        }

        $script:Version = $latest.tag_name -replace '^actionbook-cli-v', ''
    }

    $script:Tag = "actionbook-cli-v$($script:Version)"
    Write-Info "Version: $($script:Version) (tag: $($script:Tag))"
}

# ── Download and verify ──────────────────────────────────────────────────────
function Install-Binary {
    param([string]$Platform)

    $assetName = "actionbook-${Platform}.exe"
    $downloadUrl = "https://github.com/$Repo/releases/download/$Tag/$assetName"
    $checksumsUrl = "https://github.com/$Repo/releases/download/$Tag/SHA256SUMS"

    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

    try {
        Write-Info "Downloading $assetName..."
        try {
            Invoke-WebRequest -Uri $downloadUrl -OutFile (Join-Path $tmpDir $assetName) @WebRequestParams
        } catch {
            Write-Err "Failed to download binary from $downloadUrl`n$_"
        }

        Write-Info "Downloading SHA256SUMS..."
        try {
            Invoke-WebRequest -Uri $checksumsUrl -OutFile (Join-Path $tmpDir "SHA256SUMS") @WebRequestParams
        } catch {
            Write-Err "Failed to download checksums from $checksumsUrl`n$_"
        }

        Write-Info "Verifying checksum..."
        $checksumsContent = Get-Content (Join-Path $tmpDir "SHA256SUMS") -Raw
        $expectedLine = ($checksumsContent -split "`n") | Where-Object { $_ -match "\s+$([regex]::Escape($assetName))$" } | Select-Object -First 1
        if (-not $expectedLine) {
            Write-Err "No checksum entry found for $assetName in SHA256SUMS."
        }
        $expected = ($expectedLine -split '\s+')[0]

        $actual = (Get-FileHash -Path (Join-Path $tmpDir $assetName) -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) {
            Write-Err "Checksum mismatch!`n  Expected: $expected`n  Actual:   $actual"
        }
        Write-Info "Checksum verified."

        # Install
        if (-not (Test-Path $BinDir)) {
            New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
        }
        $destPath = Join-Path $BinDir $BinaryName
        Move-Item -Path (Join-Path $tmpDir $assetName) -Destination $destPath -Force
        Write-Info "Installed $BinaryName to $destPath"
    } finally {
        Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
    }
}

# ── PATH guidance ─────────────────────────────────────────────────────────────
function Test-InPath {
    $pathDirs = $env:PATH -split ';'
    $normalizedBinDir = [System.IO.Path]::GetFullPath($BinDir)
    foreach ($dir in $pathDirs) {
        if ($dir -and [System.IO.Path]::GetFullPath($dir) -eq $normalizedBinDir) {
            return $true
        }
    }
    return $false
}

function Show-PathGuidance {
    if (-not (Test-InPath)) {
        Write-Host ""
        Write-Host "  WARNING: $BinDir is not in your PATH." -ForegroundColor Yellow
        Write-Host "  Add it by running:"
        Write-Host ""
        Write-Host "    [Environment]::SetEnvironmentVariable('PATH', `"$BinDir;`$env:PATH`", 'User')" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "  Then restart your terminal."
    }
}

# ── Main ──────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "  Actionbook CLI Installer"
Write-Host "  ========================"
Write-Host ""

$platform = Get-Platform
Write-Info "Platform: $platform"

Resolve-Version
Install-Binary -Platform $platform
Show-PathGuidance

Write-Host ""
Write-Info "Done! Run 'actionbook --help' to get started."
Write-Host ""
