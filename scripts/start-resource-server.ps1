# start-resource-server.ps1
# Builds (optional) and starts the resource-server on http://localhost:8080.
#
# Usage:
#   .\scripts\start-resource-server.ps1
#   .\scripts\start-resource-server.ps1 -StorePath C:\my-store -LogLevel debug -NoBuild
#   .\scripts\start-resource-server.ps1 -DocumentDbConnectionString "mongodb+srv://..." -DocumentDbDatabase kuiper
#
# Environment variables set:
#   KUIPER_STORE_PATH                      — path for the file-system store (fallback when no DocumentDB connection string is set)
#   KUIPER_DOCUMENTDB_CONNECTION_STRING    — MongoDB-compatible connection string for Azure Cosmos DB vCore
#   KUIPER_DOCUMENTDB_DATABASE             — target database name inside the DocumentDB cluster (default: kuiper)
#   RUST_LOG                               — tracing log level

param(
    [string]$StorePath = (Join-Path $PSScriptRoot '..\kuiper-store'),
    [string]$DocumentDbConnectionString = $env:KUIPER_DOCUMENTDB_CONNECTION_STRING,
    [string]$DocumentDbDatabase = ($env:KUIPER_DOCUMENTDB_DATABASE ?? 'kuiper'),
    [ValidateSet('error','warn','info','debug','trace')]
    [string]$LogLevel  = 'info',
    [switch]$NoBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ScopedEnvironmentValue {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    foreach ($scope in 'Process', 'User', 'Machine') {
        $value = [Environment]::GetEnvironmentVariable($Name, $scope)
        if (-not [string]::IsNullOrWhiteSpace($value)) {
            return $value
        }
    }

    return $null
}

if (-not $PSBoundParameters.ContainsKey('DocumentDbConnectionString')) {
    $DocumentDbConnectionString = Get-ScopedEnvironmentValue 'KUIPER_DOCUMENTDB_CONNECTION_STRING'
}

if (-not $PSBoundParameters.ContainsKey('DocumentDbDatabase')) {
    $DocumentDbDatabase = (Get-ScopedEnvironmentValue 'KUIPER_DOCUMENTDB_DATABASE') ?? 'kuiper'
}

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

$env:RUST_LOG = $LogLevel

if ($DocumentDbConnectionString) {
    $env:KUIPER_DOCUMENTDB_CONNECTION_STRING = $DocumentDbConnectionString
    $env:KUIPER_DOCUMENTDB_DATABASE          = $DocumentDbDatabase
    Remove-Item Env:KUIPER_STORE_PATH -ErrorAction SilentlyContinue
    Write-Host "  store  : DocumentDB (database: $DocumentDbDatabase)" -ForegroundColor Cyan
} else {
    Remove-Item Env:KUIPER_DOCUMENTDB_CONNECTION_STRING -ErrorAction SilentlyContinue
    Remove-Item Env:KUIPER_DOCUMENTDB_DATABASE -ErrorAction SilentlyContinue
    $env:KUIPER_STORE_PATH = $StorePath
    New-Item -ItemType Directory -Path $StorePath -Force | Out-Null
    Write-Host "  store  : FileSystem ($StorePath)" -ForegroundColor Yellow
    Write-Host "  reason : KUIPER_DOCUMENTDB_CONNECTION_STRING was not set in Process, User, or Machine environment" -ForegroundColor DarkYellow
}

# ── Start ──────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Starting resource-server ===" -ForegroundColor Green
Write-Host "  binary : $Binary"
Write-Host "  log    : $LogLevel"
Write-Host "  url    : http://localhost:8080"
Write-Host ""
Write-Host "Press Ctrl-C to stop." -ForegroundColor DarkGray
Write-Host ""

& $Binary
