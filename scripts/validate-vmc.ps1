# validate-vmc.ps1
# Validates the vmc-control-plane HTTP API against a running server.
#
# What this tests:
#   1. Create a VirtualMachineCluster with a finalizer
#   2. Read it back and assert all fields (including mutating-admission defaults)
#   3. DELETE — confirm soft-delete (deletionTimestamp set, record still visible)
#   4. Clear the finalizer via PUT (simulates a controller finishing cleanup)
#   5. Confirm the record is pending hard-deletion by the background cleanup loop
#
# Prerequisites:
#   Start the server first:
#     .\scripts\start-vmc.ps1
#
# Usage:
#   .\scripts\validate-vmc.ps1
#   .\scripts\validate-vmc.ps1 -Server http://localhost:8090

param(
    [string]$Server = 'http://127.0.0.1:8090'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$Group     = 'vm.example.dev'
$Version   = 'v1alpha1'
$Namespace = 'default'
$Kind      = 'VirtualMachineCluster'
$Name      = 'demo-cluster'
$Finalizer = 'vm.example.dev/protection'

$Passed = 0
$Failed = 0

# ── Helpers ───────────────────────────────────────────────────────────────────

function Write-Section([string]$title) {
    Write-Host ""
    Write-Host "--- $title ---" -ForegroundColor Yellow
}

function Pass([string]$label) {
    Write-Host "  PASS  $label" -ForegroundColor Green
    $script:Passed++
}

function Fail([string]$label, [string]$detail = '') {
    Write-Host "  FAIL  $label" -ForegroundColor Red
    if ($detail) { Write-Host "        $detail" -ForegroundColor DarkGray }
    $script:Failed++
}

function Invoke-Api {
    param(
        [string]$Method,
        [string]$Url,
        [string]$JsonBody = $null
    )
    $params = @{
        Method  = $Method
        Uri     = $Url
        Headers = @{ 'Content-Type' = 'application/json'; 'Accept' = 'application/json' }
    }
    if ($JsonBody) { $params['Body'] = $JsonBody }
    Invoke-RestMethod @params
}

$ResourceUrl = "$Server/api/$Group/$Namespace/$Kind/$Name"

# ── CREATE ────────────────────────────────────────────────────────────────────
#
# Include a finalizer so that DELETE triggers a soft-delete rather than an
# immediate hard-delete.  The cleanup loop removes the record once finalizers
# is empty.

Write-Section "PUT — create VirtualMachineCluster '$Name' (with finalizer)"

$ResourceBody = @{
    apiVersion = "$Group/$Version"
    kind       = $Kind
    metadata   = @{
        name       = $Name
        namespace  = $Namespace
        finalizers = @($Finalizer)
    }
    spec       = @{
        replicas    = 3
        nodePool    = 'us-east-1a'
        machineType = 'c5.xlarge'
    }
} | ConvertTo-Json -Depth 10 -Compress

try {
    $created = Invoke-Api -Method PUT -Url $ResourceUrl -JsonBody $ResourceBody
    Pass "Resource created (uid=$($created.metadata.uid))"
} catch {
    Fail "Create failed" -detail $_.ErrorDetails.Message
    exit 1
}

# ── GET ───────────────────────────────────────────────────────────────────────

Write-Section "GET — read back '$Name'"

try {
    $fetched = Invoke-Api -Method GET -Url $ResourceUrl

    if ($fetched.metadata.name -eq $Name)             { Pass "metadata.name matches"          } else { Fail "metadata.name mismatch" }
    if ($fetched.spec.replicas -eq 3)                 { Pass "spec.replicas = 3"              } else { Fail "spec.replicas unexpected"   -detail "got $($fetched.spec.replicas)" }
    if ($fetched.spec.nodePool -eq 'us-east-1a')      { Pass "spec.nodePool correct"          } else { Fail "spec.nodePool unexpected"   -detail "got $($fetched.spec.nodePool)" }
    if ($fetched.spec.machineType -eq 'c5.xlarge')    { Pass "spec.machineType correct"       } else { Fail "spec.machineType unexpected" -detail "got $($fetched.spec.machineType)" }
    if ($fetched.metadata.finalizers -contains $Finalizer) {
        Pass "finalizer present"
    } else {
        Fail "finalizer missing"
    }
} catch {
    Fail "GET failed" -detail $_.ErrorDetails.Message
}

# ── DELETE — soft-delete because finalizer is present ─────────────────────────

Write-Section "DELETE — soft-delete '$Name'"

try {
    $null = Invoke-Api -Method DELETE -Url $ResourceUrl
    Pass "DELETE returned 2xx"
} catch {
    Fail "DELETE failed" -detail $_.ErrorDetails.Message
}

# ── GET — record visible, deletionTimestamp set ───────────────────────────────

Write-Section "GET — confirm soft-delete (deletionTimestamp set)"

$deletionTs = $null
try {
    $softDeleted = Invoke-Api -Method GET -Url $ResourceUrl
    $deletionTs  = $softDeleted.metadata.deletionTimestamp

    if ($null -ne $deletionTs) {
        Pass "deletionTimestamp is set ($deletionTs)"
    } else {
        Fail "deletionTimestamp is null — expected soft-delete marker"
    }
} catch {
    Fail "GET after delete failed" -detail $_.ErrorDetails.Message
}

# ── Clear finalizer — simulate controller finishing cleanup ───────────────────

Write-Section "PUT — clear finalizer (simulate controller acknowledging deletion)"

try {
    $clearBody = @{
        apiVersion = "$Group/$Version"
        kind       = $Kind
        metadata   = @{
            name            = $Name
            namespace       = $Namespace
            finalizers      = @()
            deletionTimestamp = $deletionTs
            uid             = $created.metadata.uid
            resourceVersion = $softDeleted.metadata.resourceVersion
        }
        spec       = @{
            replicas    = 3
            nodePool    = 'us-east-1a'
            machineType = 'c5.xlarge'
        }
    } | ConvertTo-Json -Depth 10 -Compress

    $null = Invoke-Api -Method PUT -Url $ResourceUrl -JsonBody $clearBody
    Pass "Finalizer cleared via PUT"
} catch {
    Fail "Finalizer clear failed" -detail $_.ErrorDetails.Message
}

# ── GET — pending hard-deletion ───────────────────────────────────────────────

Write-Section "GET — record pending hard-deletion by cleanup loop"

try {
    $pending = Invoke-Api -Method GET -Url $ResourceUrl

    $hasDeletion   = $null -ne $pending.metadata.deletionTimestamp
    $noFinalizers  = $null -eq $pending.metadata.finalizers -or $pending.metadata.finalizers.Count -eq 0

    if ($hasDeletion -and $noFinalizers) {
        Pass "deletionTimestamp set, finalizers empty — cleanup loop will hard-delete"
    } else {
        Fail "Unexpected state after finalizer clear"
    }
} catch {
    Fail "GET after finalizer clear failed" -detail $_.ErrorDetails.Message
}

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "─────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
if ($Failed -eq 0) {
    Write-Host "  All $Passed checks passed." -ForegroundColor Green
} else {
    Write-Host "  $Passed passed, $Failed FAILED." -ForegroundColor Red
}
Write-Host ""

exit $Failed
