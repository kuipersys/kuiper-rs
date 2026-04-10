# api-client.ps1
# Interactive CLI for the kuiper resource-server REST API.
#
# URL format: http://{server}/api/{group}/{namespace}/{kind}[/{name}]
#
# Usage (interactive menu):
#   .\scripts\api-client.ps1
#
# Usage (non-interactive / scripted):
#   .\scripts\api-client.ps1 -Action version
#   .\scripts\api-client.ps1 -Action list   -Group mygroup -Namespace default -Kind Widget
#   .\scripts\api-client.ps1 -Action get    -Group mygroup -Namespace default -Kind Widget -Name my-widget
#   .\scripts\api-client.ps1 -Action put    -Group mygroup -Namespace default -Kind Widget -Name my-widget -Body '{"apiVersion":"mygroup/v1","kind":"Widget","metadata":{"name":"my-widget","namespace":"default"},"spec":{"color":"blue"}}'
#   .\scripts\api-client.ps1 -Action delete -Group mygroup -Namespace default -Kind Widget -Name my-widget

param(
    [ValidateSet('menu','version','get','list','put','delete')]
    [string]$Action    = 'menu',
    [string]$Server    = 'http://127.0.0.1:8080',
    [string]$Group     = '',
    [string]$Namespace = 'default',
    [string]$Kind      = '',
    [string]$Name      = '',
    [string]$Body      = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'   # let us handle HTTP errors ourselves

# ── Helpers ────────────────────────────────────────────────────────────────────

function Show-Json([string]$raw) {
    try {
        $raw | ConvertFrom-Json | ConvertTo-Json -Depth 20
    } catch {
        $raw
    }
}

function Invoke-Api {
    param(
        [string]$Method,
        [string]$Url,
        [string]$JsonBody = $null
    )

    Write-Host ""
    Write-Host "  $Method $Url" -ForegroundColor DarkGray

    try {
        $params = @{
            Method  = $Method
            Uri     = $Url
            Headers = @{ 'Content-Type' = 'application/json'; 'Accept' = 'application/json' }
        }
        if ($JsonBody) { $params['Body'] = $JsonBody }

        $response = Invoke-RestMethod @params
        Write-Host ""
        Write-Host "  HTTP 2xx OK" -ForegroundColor Green
        $response | ConvertTo-Json -Depth 20 | Write-Host
    } catch {
        $statusCode = $_.Exception.Response.StatusCode.value__
        $detail     = $_.ErrorDetails.Message
        Write-Host ""
        Write-Host "  HTTP $statusCode" -ForegroundColor Red
        if ($detail) { Write-Host "  $detail" -ForegroundColor DarkGray }
    }
    Write-Host ""
}

function Build-Url {
    param([string]$g, [string]$ns, [string]$k, [string]$n = '')
    if ($n) { "$Server/api/$g/$ns/$k/$n" } else { "$Server/api/$g/$ns/$k" }
}

function Prompt-Value([string]$label, [string]$default = '') {
    $hint = if ($default) { " [$default]" } else { '' }
    $val  = Read-Host "  $label$hint"
    if (-not $val -and $default) { $default } else { $val }
}

# ── Actions ────────────────────────────────────────────────────────────────────

function Do-Version {
    Write-Host "─── GET /version ───────────────────────────────────────────" -ForegroundColor Cyan
    Invoke-Api -Method GET -Url "$Server/version"
}

function Do-List([string]$g, [string]$ns, [string]$k) {
    Write-Host "─── LIST $g / $ns / $k ─────────────────────────────────────" -ForegroundColor Cyan
    Invoke-Api -Method GET -Url (Build-Url $g $ns $k)
}

function Do-Get([string]$g, [string]$ns, [string]$k, [string]$n) {
    Write-Host "─── GET $g / $ns / $k / $n ──────────────────────────────────" -ForegroundColor Cyan
    Invoke-Api -Method GET -Url (Build-Url $g $ns $k $n)
}

function Do-Put([string]$g, [string]$ns, [string]$k, [string]$n, [string]$json) {
    Write-Host "─── PUT $g / $ns / $k / $n ──────────────────────────────────" -ForegroundColor Cyan
    Invoke-Api -Method PUT -Url (Build-Url $g $ns $k $n) -JsonBody $json
}

function Do-Delete([string]$g, [string]$ns, [string]$k, [string]$n) {
    Write-Host "─── DELETE $g / $ns / $k / $n ───────────────────────────────" -ForegroundColor Cyan
    Invoke-Api -Method DELETE -Url (Build-Url $g $ns $k $n)
}

# ── Example body generator ─────────────────────────────────────────────────────

function Example-Body([string]$g, [string]$k, [string]$ns, [string]$n) {
    @{
        apiVersion = "$g/v1"
        kind       = $k
        metadata   = @{ name = $n; namespace = $ns }
        spec       = @{ description = "created via api-client.ps1" }
    } | ConvertTo-Json -Depth 5 -Compress
}

# ── Interactive menu ────────────────────────────────────────────────────────────

function Show-Menu {
    while ($true) {
        Write-Host ""
        Write-Host "╔══════════════════════════════════════════════╗" -ForegroundColor Cyan
        Write-Host "║       kuiper resource-server API client      ║" -ForegroundColor Cyan
        Write-Host "║  server: $($Server.PadRight(35))║" -ForegroundColor Cyan
        Write-Host "╠══════════════════════════════════════════════╣" -ForegroundColor Cyan
        Write-Host "║  1  Get server version                       ║"
        Write-Host "║  2  List resources                           ║"
        Write-Host "║  3  Get a resource                           ║"
        Write-Host "║  4  Create / update a resource (PUT)         ║"
        Write-Host "║  5  Delete a resource                        ║"
        Write-Host "║  q  Quit                                     ║"
        Write-Host "╚══════════════════════════════════════════════╝" -ForegroundColor Cyan
        Write-Host ""

        $choice = Read-Host "Select"

        switch ($choice.Trim().ToLower()) {
            '1' {
                Do-Version
            }
            '2' {
                $g  = Prompt-Value 'Group    ' 'mygroup'
                $ns = Prompt-Value 'Namespace' 'default'
                $k  = Prompt-Value 'Kind     ' 'Widget'
                Do-List $g $ns $k
            }
            '3' {
                $g  = Prompt-Value 'Group    ' 'mygroup'
                $ns = Prompt-Value 'Namespace' 'default'
                $k  = Prompt-Value 'Kind     ' 'Widget'
                $n  = Prompt-Value 'Name     '
                Do-Get $g $ns $k $n
            }
            '4' {
                $g  = Prompt-Value 'Group    ' 'mygroup'
                $ns = Prompt-Value 'Namespace' 'default'
                $k  = Prompt-Value 'Kind     ' 'Widget'
                $n  = Prompt-Value 'Name     '

                $example = Example-Body $g $k $ns $n
                Write-Host ""
                Write-Host "  Example body (press Enter to use, or type your own JSON):" -ForegroundColor DarkGray
                Write-Host "  $example" -ForegroundColor DarkGray
                Write-Host ""
                $json = Read-Host "  Body"
                if (-not $json) { $json = $example }

                Do-Put $g $ns $k $n $json
            }
            '5' {
                $g  = Prompt-Value 'Group    ' 'mygroup'
                $ns = Prompt-Value 'Namespace' 'default'
                $k  = Prompt-Value 'Kind     ' 'Widget'
                $n  = Prompt-Value 'Name     '
                Do-Delete $g $ns $k $n
            }
            { $_ -in 'q','quit','exit' } {
                Write-Host "Bye." -ForegroundColor DarkGray
                return
            }
            default {
                Write-Host "  Unknown option '$choice'." -ForegroundColor Yellow
            }
        }
    }
}

# ── Entry point ─────────────────────────────────────────────────────────────────

switch ($Action) {
    'menu'    { Show-Menu }
    'version' { Do-Version }
    'list'    { Do-List   $Group $Namespace $Kind }
    'get'     { Do-Get    $Group $Namespace $Kind $Name }
    'put'     { Do-Put    $Group $Namespace $Kind $Name $Body }
    'delete'  { Do-Delete $Group $Namespace $Kind $Name }
}
