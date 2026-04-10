# validate-reconciliation.ps1
# End-to-end validation of the HTTP API (resource-server).
#
# What this tests:
#   1. ResourceDefinition registration (via `kr define`, only WITHOUT -SkipDefine)
#   2. Schema-validated resource creation via the HTTP API
#   3. Correct soft-delete behaviour:
#        GET → 200 with deletionTimestamp after DELETE (record visible, flagged)
#        LIST → includes soft-deleted records with deletionTimestamp visible
#
# NOTE: Hard-delete (reconciliation) is performed by the coordinator — NOT this script.
#       `kr` is a standalone CLI that operates directly on the file store and is
#       NOT part of the HTTP API. This script must never call `kr` when -SkipDefine
#       is set; the only permitted `kr` usage is `kr define` to bootstrap the
#       ResourceDefinition before the server starts (i.e. without -SkipDefine).
#
# Prerequisites (run in order):
#   1. cargo build (builds kr + resource-server)
#   2. .\scripts\validate-reconciliation.ps1   — registers the ResourceDefinition
#      into the file store BEFORE the server starts.
#   3. .\scripts\start-resource-server.ps1     — start the server
#   4. .\scripts\validate-reconciliation.ps1 -SkipDefine  — run the API test suite
#
# Usage:
#   .\scripts\validate-reconciliation.ps1
#   .\scripts\validate-reconciliation.ps1 -SkipDefine -Server http://localhost:8080

param(
    [string]$Server           = 'http://127.0.0.1:8080',
    [string]$StorePath        = (Join-Path $PSScriptRoot '..\kuiper-store'),
    [switch]$SkipDefine,              # skip `kr define` (use if server already has the definition)
    [switch]$NoBuild                  # skip cargo build
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'  # we handle HTTP errors ourselves

$RepoRoot    = Split-Path -Parent $PSScriptRoot
$Kr          = Join-Path $RepoRoot 'target\debug\kr.exe'
$FixturesDir = Join-Path $RepoRoot 'testing\fixtures'
$VmDefFile   = Join-Path $FixturesDir 'definitions\virtualmachine.yaml'

$Passed = 0
$Failed = 0

# ── Helpers ────────────────────────────────────────────────────────────────────

function Write-Section([string]$title) {
    Write-Host ""
    Write-Host "--- $title ---" -ForegroundColor Yellow
}

function Pass([string]$name) {
    Write-Host "  PASS  $name" -ForegroundColor Green
    $script:Passed++
}

function Fail([string]$name, [string]$detail = '') {
    Write-Host "  FAIL  $name" -ForegroundColor Red
    if ($detail) { Write-Host "        $detail" -ForegroundColor DarkGray }
    $script:Failed++
}

function Invoke-Api {
    param(
        [string]$Method,
        [string]$Url,
        [string]$JsonBody = $null,
        [switch]$AllowFailure
    )
    try {
        $params = @{
            Method      = $Method
            Uri         = $Url
            Headers     = @{ 'Content-Type' = 'application/json'; 'Accept' = 'application/json' }
            ErrorAction = 'Stop'
        }
        if ($JsonBody) { $params['Body'] = $JsonBody }
        $response = Invoke-RestMethod @params
        return @{ Ok = $true; Status = 200; Body = $response }
    } catch {
        $statusCode = 0
        try { $statusCode = [int]$_.Exception.Response.StatusCode } catch {}
        $detail = ''
        try { $detail = $_.ErrorDetails.Message } catch {}
        return @{ Ok = $false; Status = $statusCode; Body = $null; Error = $detail }
    }
}

function Build-Url([string]$group, [string]$ns, [string]$kind, [string]$name = '') {
    # URL format: /api/{group}/{namespace}/{kind}[/{name}]
    if ($name) { "$Server/api/$group/$ns/$kind/$name" }
    else        { "$Server/api/$group/$ns/$kind" }
}

$VmGroup = 'compute.cloud-api.dev'
$VmNs    = 'default'
$VmKind  = 'VirtualMachine'

function New-VmBody([string]$name, [int]$cpu = 2, [int]$mem = 8) {
    @{
        apiVersion = "$VmGroup/v1alpha1"
        kind       = $VmKind
        metadata   = @{ name = $name; namespace = $VmNs }
        spec       = @{ cpuCores = $cpu; memoryGb = $mem; diskGb = 50; image = 'ubuntu-22.04' }
    } | ConvertTo-Json -Depth 10 -Compress
}

# ── Build ──────────────────────────────────────────────────────────────────────

if (-not $NoBuild) {
    Write-Host "Building binaries…" -ForegroundColor Cyan
    Push-Location $RepoRoot
    cargo build -p kr -p resource-server 2>&1 | Write-Host
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
    Pop-Location
}

# ── Header ─────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Reconciliation Validation ===" -ForegroundColor Cyan
Write-Host "  server  : $Server"
Write-Host "  store   : $StorePath"
Write-Host ""

# ── Phase 0: Preflight ─────────────────────────────────────────────────────────

Write-Section "Preflight"

$version = Invoke-Api -Method GET -Url "$Server/version" -AllowFailure
if ($version.Ok) {
    Pass "resource-server is reachable"
} else {
    Fail "resource-server is reachable" "GET $Server/version returned $($version.Status). Is the server running?"
    Write-Host ""
    Write-Host "Start the server first: .\scripts\start-resource-server.ps1" -ForegroundColor Yellow
    exit 1
}

# ── Phase 1: ResourceDefinition ────────────────────────────────────────────────

Write-Section "ResourceDefinition registration"

if ($SkipDefine) {
    Write-Host "  (skipped — -SkipDefine flag set)" -ForegroundColor DarkGray
} elseif (-not (Test-Path $VmDefFile)) {
    Write-Host "  (skipped — fixture not found at $VmDefFile)" -ForegroundColor DarkGray
} else {
    $env:KUIPER_STORE_PATH = $StorePath
    $out = & $Kr define -f $VmDefFile -n global 2>&1
    if ($LASTEXITCODE -eq 0) {
        Pass "VirtualMachine ResourceDefinition registered via kr define"
        Write-Host "  NOTE: restart the resource-server now so it loads the definition." -ForegroundColor DarkGray
        Write-Host "  Then re-run with -SkipDefine to proceed to the resource tests." -ForegroundColor DarkGray
        Write-Host ""
        Write-Host "  If the server was started AFTER kr define, continue without restarting." -ForegroundColor DarkGray
    } else {
        Fail "VirtualMachine ResourceDefinition registered via kr define" ($out -join ' | ')
    }
}

# ── Phase 2: Create resources ──────────────────────────────────────────────────

Write-Section "Create resources"

$vm1 = 'vm-reconcile-01'
$vm2 = 'vm-reconcile-02'

$r1 = Invoke-Api -Method PUT `
    -Url  (Build-Url $VmGroup $VmNs $VmKind $vm1) `
    -JsonBody (New-VmBody $vm1 4 16)
if ($r1.Ok) { Pass "PUT $vm1 → 200" } else { Fail "PUT $vm1" "HTTP $($r1.Status): $($r1.Error)" }

$r2 = Invoke-Api -Method PUT `
    -Url  (Build-Url $VmGroup $VmNs $VmKind $vm2) `
    -JsonBody (New-VmBody $vm2 2 8)
if ($r2.Ok) { Pass "PUT $vm2 → 200" } else { Fail "PUT $vm2" "HTTP $($r2.Status): $($r2.Error)" }

# Record the original UID of vm1 — used later to prove hard-delete.
$originalUid = $null
if ($r1.Ok -and $r1.Body.metadata.uid) {
    $originalUid = $r1.Body.metadata.uid
    Write-Host "  uid($vm1) = $originalUid" -ForegroundColor DarkGray
}

# ── Phase 3: Verify created ────────────────────────────────────────────────────

Write-Section "Verify resources exist"

$g1 = Invoke-Api -Method GET -Url (Build-Url $VmGroup $VmNs $VmKind $vm1)
if ($g1.Ok -and $g1.Body.metadata.name -eq $vm1) {
    Pass "GET $vm1 → 200 with correct name"
} else {
    Fail "GET $vm1" "HTTP $($g1.Status): $($g1.Error)"
}

$list1 = Invoke-Api -Method GET -Url (Build-Url $VmGroup $VmNs $VmKind)
if ($list1.Ok) {
    $names = @($list1.Body | ForEach-Object { $_.metadata.name })
    if ($names -contains $vm1 -and $names -contains $vm2) {
        Pass "LIST returns both resources ($($names.Count) items)"
    } else {
        Fail "LIST contains both resources" "got: $($names -join ', ')"
    }
} else {
    Fail "LIST resources" "HTTP $($list1.Status): $($list1.Error)"
}

# ── Phase 4: Soft-delete ───────────────────────────────────────────────────────

Write-Section "Soft-delete (DELETE)"

$d1 = Invoke-Api -Method DELETE -Url (Build-Url $VmGroup $VmNs $VmKind $vm1) -AllowFailure
if ($d1.Status -eq 204 -or $d1.Ok) {
    Pass "DELETE $vm1 → 204"
} else {
    Fail "DELETE $vm1" "HTTP $($d1.Status): $($d1.Error)"
}

# GET immediately after DELETE must return 200 with deletionTimestamp set.
# The record is visible but flagged as pending deletion.
$g2 = Invoke-Api -Method GET -Url (Build-Url $VmGroup $VmNs $VmKind $vm1) -AllowFailure
$hasDeletionTs = $null -ne $g2.Body.metadata.PSObject.Properties['deletionTimestamp']
if ($g2.Ok -and $hasDeletionTs) {
    Pass "GET $vm1 after DELETE → 200 with deletionTimestamp (soft-deleted, visible)"
} else {
    Fail "GET $vm1 after DELETE" "expected 200 with deletionTimestamp, got HTTP $($g2.Status) hasDeletionTs=$hasDeletionTs"
}

# LIST must still include vm1 — soft-deleted records remain visible.
# vm1 must have deletionTimestamp; vm2 must not.
$list2 = Invoke-Api -Method GET -Url (Build-Url $VmGroup $VmNs $VmKind)
if ($list2.Ok) {
    $vm1Item = $list2.Body | Where-Object { $_.metadata.name -eq $vm1 } | Select-Object -First 1
    $vm2Item = $list2.Body | Where-Object { $_.metadata.name -eq $vm2 } | Select-Object -First 1

    if ($null -ne $vm1Item) {
        $vm1HasDeletionTs = $null -ne $vm1Item.metadata.PSObject.Properties['deletionTimestamp']
        if ($vm1HasDeletionTs) {
            Pass "LIST: $vm1 present with deletionTimestamp"
        } else {
            Fail "LIST: $vm1 should have deletionTimestamp" "deletionTimestamp not found on item"
        }
    } else {
        Fail "LIST: $vm1 should still be visible after soft-delete" "not found in list"
    }

    if ($null -ne $vm2Item) {
        $vm2HasDeletionTs = $null -ne $vm2Item.metadata.PSObject.Properties['deletionTimestamp']
        if (-not $vm2HasDeletionTs) {
            Pass "LIST: $vm2 present without deletionTimestamp"
        } else {
            Fail "LIST: $vm2 should not have deletionTimestamp" "deletionTimestamp unexpectedly present"
        }
    } else {
        Fail "LIST: $vm2 should be visible" "not found in list"
    }
} else {
    Fail "LIST after partial delete" "HTTP $($list2.Status): $($list2.Error)"
}

Start-Sleep -Seconds 5  # wait a moment before cleanup to ensure timestamps differ

# ── Phase 5: Cleanup ───────────────────────────────────────────────────────────

Write-Section "Cleanup"

# Soft-delete any remaining test resources via the API.
# Hard-delete (removing files from the store) is the coordinator's responsibility —
# this script does NOT invoke `kr` or any out-of-band tooling.
foreach ($name in @($vm1, $vm2)) {
    $del = Invoke-Api -Method DELETE -Url (Build-Url $VmGroup $VmNs $VmKind $name) -AllowFailure
    if ($del.Status -eq 204 -or $del.Ok) {
        Write-Host "  Soft-deleted $name (coordinator will hard-delete on next reconcile)" -ForegroundColor DarkGray
    } else {
        Write-Host "  Could not delete $name (HTTP $($del.Status)) — may already be gone" -ForegroundColor DarkGray
    }
}

Write-Host "  (Hard-delete will be performed by the coordinator on its next reconcile pass.)" -ForegroundColor DarkGray

# ── Summary ────────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "=== Results: $Passed passed, $Failed failed ===" -ForegroundColor Cyan

if ($Failed -gt 0) { exit 1 }
exit 0
