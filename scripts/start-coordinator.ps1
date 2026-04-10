# start-coordinator.ps1
# Builds (optional) and starts the coordinator service.
# The coordinator connects to the resource-server WebSocket, subscribes to all
# resource events, and triggers reconciliation whenever a resource is marked for
# deletion (metadata.deletionTimestamp set).
#
# Usage:
#   .\scripts\start-coordinator.ps1
#   .\scripts\start-coordinator.ps1 -ServerUrl ws://remote-host:8080/ws -LogLevel debug -NoBuild
#
# The resource-server must be running before starting the coordinator.

param(
    [string]$ServerUrl = 'ws://127.0.0.1:8080/ws',
    [string]$StorePath = (Join-Path $PSScriptRoot '..\kuiper-store'),
    [ValidateSet('error','warn','info','debug','trace')]
    [string]$LogLevel  = 'info',
    [switch]$NoBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Binary   = Join-Path $RepoRoot 'target\debug\coordinator.exe'

# ── Build ──────────────────────────────────────────────────────────────────────

if (-not $NoBuild) {
    Write-Host "Building coordinator…" -ForegroundColor Cyan
    Push-Location $RepoRoot
    cargo build -p coordinator
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}

if (-not (Test-Path $Binary)) {
    Write-Error "Binary not found at $Binary. Run without -NoBuild or run 'cargo build -p coordinator' first."
    exit 1
}

# ── Environment ────────────────────────────────────────────────────────────────

$env:KUIPER_SERVER_WS_URL = $ServerUrl
$env:KUIPER_STORE_PATH    = $StorePath
$env:RUST_LOG             = $LogLevel

# ── Start ──────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Starting coordinator ===" -ForegroundColor Green
Write-Host "  binary     : $Binary"
Write-Host "  server url : $ServerUrl"
Write-Host "  store      : $StorePath"
Write-Host "  log        : $LogLevel"
Write-Host ""
Write-Host "The coordinator will reconnect automatically if the resource-server is not yet ready." -ForegroundColor DarkGray
Write-Host "Press Ctrl-C to stop." -ForegroundColor DarkGray
Write-Host ""

& $Binary
