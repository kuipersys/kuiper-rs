# create-vmc-resource.ps1
# Creates a single VirtualMachineCluster resource WITHOUT including any
# finalizers in the request body, then reads it back to confirm that the
# mutating admission controller injected the protection finalizer automatically.
#
# Prerequisites:
#   Start the server first:
#     .\scripts\start-vmc.ps1
#
# Usage:
#   .\scripts\create-vmc-resource.ps1
#   .\scripts\create-vmc-resource.ps1 -Name my-cluster -Server http://localhost:8090

param(
    [string]$Server    = 'http://127.0.0.1:8090',
    [string]$Namespace = 'default',
    [string]$Name      = 'demo-cluster'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$Group   = 'vm.example.dev'
$Version = 'v1alpha1'
$Kind    = 'VirtualMachineCluster'

$ExpectedFinalizer = 'vm.example.dev/protection'

$Passed = 0
$Failed = 0

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

$ResourceUrl = "$Server/api/$Group/$Namespace/$Kind/$Name"

# ── PUT (no finalizers in request) ────────────────────────────────────────────

Write-Section "PUT — create '$Name' (no finalizers sent)"

# Deliberately omit 'finalizers' from the request — the mutating admission
# controller must inject 'vm.example.dev/protection' on the server side.
$Body = @{
    apiVersion = "$Group/$Version"
    kind       = $Kind
    metadata   = @{
        name      = $Name
        namespace = $Namespace
    }
    spec       = @{
        replicas    = 2
        nodePool    = 'eu-west-1b'
        machineType = 'c5.2xlarge'
    }
} | ConvertTo-Json -Depth 10 -Compress

Write-Host "  Request body does not include 'finalizers'." -ForegroundColor DarkGray

try {
    $created = Invoke-RestMethod -Method PUT -Uri $ResourceUrl `
        -Headers @{ 'Content-Type' = 'application/json'; 'Accept' = 'application/json' } `
        -Body $Body

    Write-Host ""
    Write-Host "  Response:" -ForegroundColor DarkGray
    $created | ConvertTo-Json -Depth 10 | Write-Host

    Pass "PUT returned 2xx"

    # ── Validate admission-injected fields ────────────────────────────────────

    Write-Section "Admission-controller assertions"

    if ($created.metadata.finalizers -contains $ExpectedFinalizer) {
        Pass "finalizer '$ExpectedFinalizer' injected by mutating admission"
    } else {
        $actual = ($created.metadata.finalizers -join ', ')
        Fail "Expected finalizer not found" -detail "got: [$actual]"
    }

    if ($null -ne $created.metadata.uid -and $created.metadata.uid -ne '00000000-0000-0000-0000-000000000000') {
        Pass "uid assigned ($($created.metadata.uid))"
    } else {
        Fail "uid not assigned"
    }

    if ($created.spec.replicas -eq 2)          { Pass "spec.replicas preserved (2)"       } else { Fail "spec.replicas wrong"   -detail "got $($created.spec.replicas)" }
    if ($created.spec.nodePool -eq 'eu-west-1b') { Pass "spec.nodePool preserved"         } else { Fail "spec.nodePool wrong"   -detail "got $($created.spec.nodePool)" }
    if ($created.spec.machineType -eq 'c5.2xlarge') { Pass "spec.machineType preserved"   } else { Fail "spec.machineType wrong" -detail "got $($created.spec.machineType)" }

} catch {
    $statusCode = $_.Exception.Response.StatusCode.value__
    $detail     = $_.ErrorDetails.Message
    Fail "PUT failed (HTTP $statusCode)" -detail $detail
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
