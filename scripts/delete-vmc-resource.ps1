# delete-vmc-resource.ps1
# Soft-deletes a VirtualMachineCluster resource.
#
# Prerequisites:
#   .\scripts\start-vmc.ps1
#
# Usage:
#   .\scripts\delete-vmc-resource.ps1
#   .\scripts\delete-vmc-resource.ps1 -Name my-cluster -Server http://localhost:8090

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

$Url = "$Server/api/$Group/$Namespace/$Kind/$Name"

Write-Host "DELETE $Url" -ForegroundColor Cyan

try {
    $response = Invoke-RestMethod -Method DELETE -Uri $Url `
        -Headers @{ 'Accept' = 'application/json' }

    Write-Host ""
    Write-Host "  HTTP 2xx OK" -ForegroundColor Green
    if ($response) {
        $response | ConvertTo-Json -Depth 10 | Write-Host
    }
} catch {
    $statusCode = $_.Exception.Response.StatusCode.value__
    $detail     = $_.ErrorDetails.Message
    Write-Host ""
    Write-Host "  HTTP $statusCode" -ForegroundColor Red
    if ($detail) { Write-Host "  $detail" -ForegroundColor DarkGray }
    exit 1
}
