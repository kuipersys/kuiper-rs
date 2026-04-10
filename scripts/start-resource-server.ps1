# start-resource-server.ps1
# Builds (optional) and starts the resource-server on http://localhost:8080.
#
# Usage:
#   .\scripts\start-resource-server.ps1
#   .\scripts\start-resource-server.ps1 -StorePath C:\my-store -LogLevel debug -NoBuild
#
# Environment variables set:
#   KUIPER_STORE_PATH  — path where the file-system store is created
#   RUST_LOG           — tracing log level

param(
    [string]$StorePath = (Join-Path $PSScriptRoot '..\kuiper-store'),
    [ValidateSet('error','warn','info','debug','trace')]
    [string]$LogLevel  = 'info',
    [switch]$NoBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Binary   = Join-Path $RepoRoot 'target\debug\resource-server.exe'

# ── Build ──────────────────────────────────────────────────────────────────────

if (-not $NoBuild) {
    Write-Host "Building resource-server…" -ForegroundColor Cyan
    Push-Location $RepoRoot
    cargo build -p resource-server
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}

if (-not (Test-Path $Binary)) {
    Write-Error "Binary not found at $Binary. Run without -NoBuild or run 'cargo build -p resource-server' first."
    exit 1
}

# ── Environment ────────────────────────────────────────────────────────────────

$env:KUIPER_STORE_PATH = $StorePath
$env:RUST_LOG          = $LogLevel

New-Item -ItemType Directory -Path $StorePath -Force | Out-Null

# ── Start ──────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Starting resource-server ===" -ForegroundColor Green
Write-Host "  binary : $Binary"
Write-Host "  store  : $StorePath"
Write-Host "  log    : $LogLevel"
Write-Host "  url    : http://localhost:8080"
Write-Host ""
Write-Host "Press Ctrl-C to stop." -ForegroundColor DarkGray
Write-Host ""

& $Binary
