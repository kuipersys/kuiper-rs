# start-vmc.ps1
# Builds (optional) and starts the vmc-control-plane on http://localhost:8090.
#
# Usage:
#   .\scripts\start-vmc.ps1
#   .\scripts\start-vmc.ps1 -StorePath C:\my-store -LogLevel debug -NoBuild

param(
    [string]$StorePath = (Join-Path $PSScriptRoot '..\stores\vmc'),
    [ValidateSet('error', 'warn', 'info', 'debug', 'trace')]
    [string]$LogLevel = 'info',
    [switch]$NoBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Binary   = Join-Path $RepoRoot 'target\debug\vmc-control-plane.exe'

# ── Build ──────────────────────────────────────────────────────────────────────

if (-not $NoBuild) {
    Write-Host "Building vmc-control-plane..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    cargo build -p vmc-control-plane
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}

if (-not (Test-Path $Binary)) {
    Write-Error "Binary not found at '$Binary'. Run without -NoBuild or run 'cargo build -p vmc-control-plane' first."
    exit 1
}

# ── Environment ────────────────────────────────────────────────────────────────

$env:KUIPER_STORE_PATH = $StorePath
$env:RUST_LOG          = $LogLevel
$env:VMC_PORT          = '8090'

New-Item -ItemType Directory -Path $StorePath -Force | Out-Null

# ── Start ──────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Starting vmc-control-plane ===" -ForegroundColor Green
Write-Host "  binary : $Binary"
Write-Host "  store  : $StorePath"
Write-Host "  log    : $LogLevel"
Write-Host "  url    : http://localhost:8090"
Write-Host ""
Write-Host "Press Ctrl-C to stop." -ForegroundColor DarkGray
Write-Host ""

& $Binary
