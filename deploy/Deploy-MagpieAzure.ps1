<#
.SYNOPSIS
    Deploy, configure Entra ID SSO, verify, or remove Magpie Gateway on Microsoft Azure (Australia).
.DESCRIPTION
    Enterprise PowerShell operator workflow for Magpie Gateway on Azure Container Apps, Azure Key Vault,
    Microsoft Entra ID, and Microsoft Sentinel. Supports splatted parameters and -WhatIf previews.
.EXAMPLE
    $magpie = @{
        TenantId       = '<tenant-guid>'
        SubscriptionId = '<subscription-guid>'
        NamePrefix     = 'magpieagency'
        Location       = 'australiaeast'
        PublicUrl      = 'https://magpie.agency.gov.au'
    }
    ./deploy/Deploy-MagpieAzure.ps1 @magpie -Step Preflight,Entra,Infra,Verify
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [string]$TenantId,

    [Parameter(Mandatory = $true)]
    [string]$SubscriptionId,

    [ValidatePattern('^[a-z0-9]{3,14}$')]
    [string]$NamePrefix = 'magpie',

    [ValidateSet('australiaeast', 'australiasoutheast', 'australiacentral', 'australiacentral2')]
    [string]$Location = 'australiaeast',

    [string]$PublicUrl = '',

    [ValidateSet('log-only', 'enforce')]
    [string]$GuardrailMode = 'log-only',

    [string]$AzureOpenAiResourceId = '',

    [switch]$EnableSentinel = $true,

    [switch]$InternalOnly = $true,

    [ValidateSet('Preflight', 'Entra', 'Infra', 'Verify', 'Destroy')]
    [string[]]$Step = @('Preflight', 'Infra', 'Verify')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$TfDir = Join-Path $PSScriptRoot 'terraform/azure'

function Write-Check([string]$Status, [string]$Label, [string]$Detail) {
    Write-Host ("  [{0,-4}] {1,-26} : {2}" -f $Status, $Label, $Detail)
}

if ($Step -contains 'Preflight') {
    Write-Host "`n=== Step 1: Preflight ===" -ForegroundColor Cyan
    foreach ($bin in @('az', 'terraform')) {
        if (-not (Get-Command $bin -ErrorAction SilentlyContinue)) {
            throw "Required tool '$bin' was not found on PATH."
        }
        Write-Check 'ok' $bin (Get-Command $bin).Source
    }
    az account set --subscription $SubscriptionId
    $acct = az account show | ConvertFrom-Json
    if ($acct.tenantId -ne $TenantId) {
        throw "Active Azure CLI tenant ($($acct.tenantId)) does not match -TenantId ($TenantId). Run 'az login --tenant $TenantId'."
    }
    Write-Check 'ok' 'Azure subscription' "$($acct.name) ($SubscriptionId)"
    Write-Check 'ok' 'Australian region' $Location
}

$EntraClientId = $env:MAGPIE_OIDC_CLIENT_ID
$EntraClientSecret = $env:MAGPIE_OIDC_CLIENT_SECRET

if ($Step -contains 'Entra') {
    Write-Host "`n=== Step 2: Microsoft Entra ID App Registration ===" -ForegroundColor Cyan
    $displayName = "Magpie Gateway ($NamePrefix)"
    $redirectUri = if ($PublicUrl) { "$($PublicUrl.TrimEnd('/'))/api/auth/callback" } else { "https://ca-$NamePrefix.azurecontainerapps.io/api/auth/callback" }

    if ($PSCmdlet.ShouldProcess($displayName, "Create/update Entra ID app registration with Magpie.* App Roles")) {
        $existing = az ad app list --display-name $displayName | ConvertFrom-Json
        if ($existing.Count -gt 0) {
            $EntraClientId = $existing[0].appId
            Write-Check 'ok' 'Existing Entra app' $EntraClientId
        } else {
            $app = az ad app create `
                --display-name $displayName `
                --sign-in-audience AzureADMyOrg `
                --web-redirect-uris $redirectUri | ConvertFrom-Json
            $EntraClientId = $app.appId
            az ad sp create --id $EntraClientId | Out-Null
            Write-Check 'ok' 'Created Entra app' $EntraClientId
        }
        if (-not $EntraClientSecret) {
            $cred = az ad app credential reset --id $EntraClientId --display-name 'magpie-portal' --years 1 | ConvertFrom-Json
            $EntraClientSecret = $cred.password
            Write-Check 'ok' 'Client secret' 'Generated (stored in Azure Key Vault by Terraform; not printed)'
        }
    }
}

if ($Step -contains 'Infra') {
    Write-Host "`n=== Step 3: Terraform Infrastructure ($Location) ===" -ForegroundColor Cyan
    Push-Location $TfDir
    try {
        terraform init -upgrade
        $tfArgs = @(
            "-var=name_prefix=$NamePrefix",
            "-var=location=$Location",
            "-var=guardrail_mode=$GuardrailMode",
            "-var=public_url=$PublicUrl",
            "-var=entra_tenant_id=$TenantId",
            "-var=entra_client_id=$EntraClientId",
            "-var=entra_client_secret=$EntraClientSecret",
            "-var=azure_openai_resource_id=$AzureOpenAiResourceId",
            "-var=enable_sentinel=$($EnableSentinel.IsPresent.ToString().ToLower())",
            "-var=enable_internal_only=$($InternalOnly.IsPresent.ToString().ToLower())"
        )
        if ($WhatIfPreference) {
            terraform plan @tfArgs
        } elseif ($PSCmdlet.ShouldProcess("rg-$NamePrefix-$Location", 'Apply Magpie Gateway Azure Terraform')) {
            terraform apply -auto-approve @tfArgs
        }
    } finally {
        Pop-Location
    }
}

if ($Step -contains 'Verify') {
    Write-Host "`n=== Step 4: Verification ===" -ForegroundColor Cyan
    Push-Location $TfDir
    try {
        $out = terraform output -json | ConvertFrom-Json
        Write-Check 'ok' 'Resource Group' $out.resource_group_name.value
        Write-Check 'ok' 'Portal FQDN' $out.portal_fqdn.value
        Write-Check 'ok' 'Key Vault' $out.key_vault_uri.value
        Write-Check 'ok' 'Managed Identity' $out.managed_identity_client_id.value
        if ($out.sentinel_dce_endpoint.value) {
            Write-Check 'ok' 'Sentinel DCE' $out.sentinel_dce_endpoint.value
            Write-Check 'ok' 'Sentinel DCR ID' $out.sentinel_dcr_immutable_id.value
        }
        $target = if ($PublicUrl) { $PublicUrl.TrimEnd('/') } else { "https://$($out.portal_fqdn.value)" }
        try {
            $resp = Invoke-WebRequest -Uri "$target/healthz" -TimeoutSec 10 -UseBasicParsing
            Write-Check 'PASS' 'Portal /healthz' "HTTP $($resp.StatusCode)"
        } catch {
            Write-Check 'WARN' 'Portal /healthz' 'Not reachable from this workstation (expected if -InternalOnly is enabled)'
        }
    } finally {
        Pop-Location
    }
}

if ($Step -contains 'Destroy') {
    Write-Host "`n=== Destroy Magpie Gateway ($NamePrefix) ===" -ForegroundColor Red
    Push-Location $TfDir
    try {
        if ($PSCmdlet.ShouldProcess("rg-$NamePrefix-$Location", 'Destroy Magpie Gateway Azure resources')) {
            terraform destroy -var="name_prefix=$NamePrefix" -var="location=$Location"
        }
    } finally {
        Pop-Location
    }
}
