# validate-kr.ps1
# End-to-end validation of kr against a local file-system store.
# Submits ResourceDefinitions and then creates / reads resources from them.
#
# Usage:
#   .\testing\validate-kr.ps1
#
# Requirements:
#   cargo build must have been run first (binary at target/debug/kr.exe)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Paths ─────────────────────────────────────────────────────────────────────

$ScriptDir   = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot    = Split-Path -Parent $ScriptDir
$Kr           = Join-Path $RepoRoot 'target\debug\kr.exe'
$FixturesDir = Join-Path $ScriptDir 'fixtures'
$StoreDir    = Join-Path $ScriptDir 'store'
$TempDir     = Join-Path $ScriptDir 'temp'

# ── Environment ───────────────────────────────────────────────────────────────

$env:KUIPER_STORE_PATH = $StoreDir

# Start clean each run
if (Test-Path $StoreDir) {
    Remove-Item -Recurse -Force $StoreDir
}
New-Item -ItemType Directory -Path $StoreDir | Out-Null

if (Test-Path $TempDir) {
    Remove-Item -Recurse -Force $TempDir
}
New-Item -ItemType Directory -Path $TempDir | Out-Null

# ── Helpers ───────────────────────────────────────────────────────────────────

$Passed = 0
$Failed = 0

function Invoke-Kr {
    param([string[]]$KrArgs)
    & $Kr @KrArgs 2>&1
}

function Assert-Success {
    param(
        [string]$TestName,
        [string[]]$KrArgs,
        [string]$Contains = $null
    )
    $output = Invoke-Kr $KrArgs
    $exitCode = $LASTEXITCODE

    $ok = $exitCode -eq 0
    if ($ok -and $Contains) {
        $ok = ($output -join "`n") -match [regex]::Escape($Contains)
    }

    if ($ok) {
        Write-Host "  PASS  $TestName" -ForegroundColor Green
        $script:Passed++
    } else {
        Write-Host "  FAIL  $TestName" -ForegroundColor Red
        Write-Host "        output: $($output -join ' | ')" -ForegroundColor DarkGray
        $script:Failed++
    }
}

function Assert-Failure {
    param(
        [string]$TestName,
        [string[]]$KrArgs
    )
    $output = Invoke-Kr $KrArgs
    $exitCode = $LASTEXITCODE

    if ($exitCode -ne 0) {
        Write-Host "  PASS  $TestName (expected failure)" -ForegroundColor Green
        $script:Passed++
    } else {
        Write-Host "  FAIL  $TestName (expected non-zero exit, got 0)" -ForegroundColor Red
        Write-Host "        output: $($output -join ' | ')" -ForegroundColor DarkGray
        $script:Failed++
    }
}

function Assert-FailureContaining {
    param(
        [string]$TestName,
        [string[]]$KrArgs,
        [string]$ErrorContains
    )
    $output = (Invoke-Kr $KrArgs) -join "`n"
    $exitCode = $LASTEXITCODE

    $rejected = $exitCode -ne 0
    $hasMessage = $output -match [regex]::Escape($ErrorContains)

    if ($rejected -and $hasMessage) {
        Write-Host "  PASS  $TestName (rejected: $ErrorContains)" -ForegroundColor Green
        $script:Passed++
    } elseif (-not $rejected) {
        Write-Host "  FAIL  $TestName (expected rejection but command succeeded)" -ForegroundColor Red
        Write-Host "        output: $output" -ForegroundColor DarkGray
        $script:Failed++
    } else {
        Write-Host "  FAIL  $TestName (rejected but wrong error)" -ForegroundColor Red
        Write-Host "        expected : $ErrorContains" -ForegroundColor DarkGray
        Write-Host "        actual   : $output" -ForegroundColor DarkGray
        $script:Failed++
    }
}

# ── Preflight ─────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== kr Validation ==" -ForegroundColor Cyan
Write-Host "  binary : $Kr"
Write-Host "  store  : $StoreDir"
Write-Host "  fixtures: $FixturesDir"
Write-Host ""

if (-not (Test-Path $Kr)) {
    Write-Host "ERROR: kr binary not found. Run 'cargo build' first." -ForegroundColor Red
    exit 1
}

# ── Section: Built-in commands ────────────────────────────────────────────────

Write-Host "--- Built-in commands ---" -ForegroundColor Yellow

Assert-Success "version command" @('version') -Contains 'version'
Assert-Success "echo command"   @('echo', '--param', 'message=hello') -Contains 'hello'

# ── Section: Register ResourceDefinitions ─────────────────────────────────────

Write-Host ""
Write-Host "--- Register ResourceDefinitions ---" -ForegroundColor Yellow

$vmDef     = Join-Path $FixturesDir 'definitions\virtualmachine.yaml'
$sensorDef = Join-Path $FixturesDir 'definitions\sensor.yaml'

Assert-Success "define VirtualMachine ResourceDefinition" `
    @('define', '-f', $vmDef, '-n', 'global') `
    -Contains 'ResourceDefinition'

Assert-Success "define Sensor ResourceDefinition" `
    @('define', '-f', $sensorDef, '-n', 'global') `
    -Contains 'ResourceDefinition'

# ── Section: Create resources from definitions ────────────────────────────────

Write-Host ""
Write-Host "--- Create resources ---" -ForegroundColor Yellow

$vmWeb   = Join-Path $FixturesDir 'resources\vm-web-server-01.yaml'
$vmDb    = Join-Path $FixturesDir 'resources\vm-db-server-01.yaml'
$sensorT = Join-Path $FixturesDir 'resources\sensor-temperature-01.yaml'
$sensorH = Join-Path $FixturesDir 'resources\sensor-humidity-01.yaml'

Assert-Success "create VirtualMachine web-server-01" `
    @('set', '-f', $vmWeb) `
    -Contains 'web-server-01'

Assert-Success "create VirtualMachine db-server-01" `
    @('set', '-f', $vmDb) `
    -Contains 'db-server-01'

Assert-Success "create Sensor temperature-sensor-01" `
    @('set', '-f', $sensorT) `
    -Contains 'temperature-sensor-01'

Assert-Success "create Sensor humidity-sensor-01" `
    @('set', '-f', $sensorH) `
    -Contains 'humidity-sensor-01'

# ── Section: Read back resources ──────────────────────────────────────────────

Write-Host ""
Write-Host "--- Read back resources ---" -ForegroundColor Yellow

Assert-Success "get VirtualMachine web-server-01" `
    @('get', 'compute.cloud-api.dev/v1alpha1/VirtualMachine/web-server-01', '-n', 'default') `
    -Contains 'web-server-01'

Assert-Success "get VirtualMachine db-server-01" `
    @('get', 'compute.cloud-api.dev/v1alpha1/VirtualMachine/db-server-01', '-n', 'default') `
    -Contains 'db-server-01'

Assert-Success "get Sensor temperature-sensor-01" `
    @('get', 'iot.cloud-api.dev/v1alpha1/Sensor/temperature-sensor-01', '-n', 'default') `
    -Contains 'temperature-sensor-01'

Assert-Success "get Sensor humidity-sensor-01" `
    @('get', 'iot.cloud-api.dev/v1alpha1/Sensor/humidity-sensor-01', '-n', 'default') `
    -Contains 'humidity-sensor-01'

# ── Section: List resources ───────────────────────────────────────────────────

Write-Host ""
Write-Host "--- List resources ---" -ForegroundColor Yellow

Assert-Success "list VirtualMachines" `
    @('list', 'compute.cloud-api.dev/v1alpha1/VirtualMachine', '-n', 'default') `
    -Contains 'web-server-01'

Assert-Success "list Sensors" `
    @('list', 'iot.cloud-api.dev/v1alpha1/Sensor', '-n', 'default') `
    -Contains 'temperature-sensor-01'

# ── Section: Update a resource (re-apply) ─────────────────────────────────────

Write-Host ""
Write-Host "--- Update (re-apply) ---" -ForegroundColor Yellow

Assert-Success "re-apply VirtualMachine web-server-01 (idempotent)" `
    @('set', '-f', $vmWeb) `
    -Contains 'web-server-01'

# ── Section: Delete a resource ────────────────────────────────────────────────

Write-Host ""
Write-Host "--- Delete resources ---" -ForegroundColor Yellow

Assert-Success "delete VirtualMachine db-server-01" `
    @('delete', 'compute.cloud-api.dev/v1alpha1/VirtualMachine/db-server-01', '-n', 'default')

# A get on a soft-deleted resource returns a Not Found / pending-deletion error (exit 1).
Assert-Failure "soft-deleted resource returns pending-deletion error" `
    @('get', 'compute.cloud-api.dev/v1alpha1/VirtualMachine/db-server-01', '-n', 'default')

# ── Section: Security guards ──────────────────────────────────────────────────

Write-Host ""
Write-Host "--- Security guards ---" -ForegroundColor Yellow

# 'set' (unprivileged) must be rejected when the apiVersion belongs to the reserved system group.
# 'define' (privileged) is required to write ResourceDefinitions.
$reservedPayload = @{
    apiVersion = "ext.api.cloud-api.dev/v1alpha1"
    kind       = "ResourceDefinition"
    metadata   = @{ name = "evil"; namespace = "global" }
    spec       = @{ group = "evil.example.com"; names = @{ kind = "Evil"; singular = "evil"; plural = "evils" }; scope = "Namespace"; versions = @() }
} | ConvertTo-Json -Compress

$reservedFile = Join-Path $TempDir 'evil-rd.json'
$reservedPayload | Set-Content $reservedFile

Assert-FailureContaining "unprivileged 'set' rejected for reserved apiVersion group" `
    @('set', '-f', $reservedFile, '-n', 'global') `
    -ErrorContains 'Forbidden'

# Attempt to write a resource with a reserved UID prefix via unprivileged 'set'.
# The spec is intentionally valid (passes schema) so the UID guard — not schema validation —
# is the gate being tested here.
$reservedUidPayload = @{
    apiVersion = "compute.cloud-api.dev/v1alpha1"
    kind       = "VirtualMachine"
    metadata   = @{ name = "spoofed-vm"; namespace = "default"; uid = "00000000-0000-0000-0000-000000000099" }
    spec       = @{ cpuCores = 2; memoryGb = 8; image = "ubuntu-22.04" }
} | ConvertTo-Json -Compress

$reservedUidFile = Join-Path $TempDir 'spoofed-uid.json'
$reservedUidPayload | Set-Content $reservedUidFile

Assert-FailureContaining "unprivileged 'set' rejected for reserved UID prefix" `
    @('set', '-f', $reservedUidFile, '-n', 'default') `
    -ErrorContains 'Forbidden'

# ── Section: Schema validation ────────────────────────────────────────────────

Write-Host ""
Write-Host "--- Schema validation (VirtualMachine) ---" -ForegroundColor Yellow

# Valid resource — all required fields present and correct types.
$validVm = @{
    apiVersion = "compute.cloud-api.dev/v1alpha1"
    kind       = "VirtualMachine"
    metadata   = @{ name = "vm-schema-valid"; namespace = "default" }
    spec       = @{ cpuCores = 2; memoryGb = 8; image = "ubuntu-22.04" }
} | ConvertTo-Json -Compress

$validVmFile = Join-Path $TempDir 'vm-schema-valid.json'
$validVm | Set-Content $validVmFile

Assert-Success "schema-valid VirtualMachine accepted" `
    @('set', '-f', $validVmFile) `
    -Contains 'vm-schema-valid'

# Missing required field 'image'.
$missingField = @{
    apiVersion = "compute.cloud-api.dev/v1alpha1"
    kind       = "VirtualMachine"
    metadata   = @{ name = "vm-bad-missing"; namespace = "default" }
    spec       = @{ cpuCores = 2; memoryGb = 8 }
} | ConvertTo-Json -Compress

$missingFieldFile = Join-Path $TempDir 'vm-bad-missing.json'
$missingField | Set-Content $missingFieldFile

Assert-FailureContaining "schema-invalid VirtualMachine rejected (missing required field)" `
    @('set', '-f', $missingFieldFile) `
    -ErrorContains 'schema validation'

# Wrong type: cpuCores is a string instead of integer.
$badType = @{
    apiVersion = "compute.cloud-api.dev/v1alpha1"
    kind       = "VirtualMachine"
    metadata   = @{ name = "vm-bad-type"; namespace = "default" }
    spec       = @{ cpuCores = "two"; memoryGb = 8; image = "ubuntu-22.04" }
} | ConvertTo-Json -Compress

$badTypeFile = Join-Path $TempDir 'vm-bad-type.json'
$badType | Set-Content $badTypeFile

Assert-FailureContaining "schema-invalid VirtualMachine rejected (wrong type)" `
    @('set', '-f', $badTypeFile) `
    -ErrorContains 'schema validation'

# Additional property not in schema.
$extraProp = @{
    apiVersion = "compute.cloud-api.dev/v1alpha1"
    kind       = "VirtualMachine"
    metadata   = @{ name = "vm-bad-extra"; namespace = "default" }
    spec       = @{ cpuCores = 2; memoryGb = 8; image = "ubuntu-22.04"; unknownField = "oops" }
} | ConvertTo-Json -Compress

$extraPropFile = Join-Path $TempDir 'vm-bad-extra.json'
$extraProp | Set-Content $extraPropFile

Assert-FailureContaining "schema-invalid VirtualMachine rejected (additional property)" `
    @('set', '-f', $extraPropFile) `
    -ErrorContains 'schema validation'

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Results: $Passed passed, $Failed failed ===" -ForegroundColor Cyan

if ($Failed -gt 0) {
    exit 1
}
exit 0
