<#
  HookReplay Local Authenticode Code-Signing Helper

  Usage:
    1) Create self-signed Code Signing cert in CurrentUser\My and export .cer:
         powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-local.ps1 -InitCert
    2) Optional: Trust that cert under CurrentUser\Root & CurrentUser\TrustedPublisher (requires prompt):
         powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-local.ps1 -TrustCurrentUser
    3) Sign release binaries (release\HookReplay.exe and NSIS installer):
         powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-local.ps1
       Or specify a custom certificate thumbprint / file list / timestamp server:
         powershell -NoProfile -ExecutionPolicy Bypass -File scripts/sign-local.ps1 -Thumbprint <HASH> -Files release\HookReplay.exe
#>
param(
    [switch]$InitCert,
    [switch]$TrustCurrentUser,
    [string]$Thumbprint = "",
    [string[]]$Files = @(),
    [string]$TimestampServer = "",
    [string]$CertSubject = "CN=Sinterdial"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$signingDir = Join-Path $repoRoot ".signing"
$cerPath = Join-Path $signingDir "Sinterdial-Local.cer"

function Find-SigningCert {
    param([string]$Hash, [string]$SubjectName)
    $all = @(Get-ChildItem -Path "Cert:\CurrentUser\My" -CodeSigningCert -ErrorAction SilentlyContinue)
    if ($Hash) {
        $norm = ($Hash -replace "\s+", "").ToUpperInvariant()
        return $all | Where-Object { $_.Thumbprint.ToUpperInvariant() -eq $norm } | Select-Object -First 1
    }
    return $all | Where-Object { $_.Subject -eq $SubjectName } | Sort-Object NotAfter -Descending | Select-Object -First 1
}

if ($InitCert) {
    $existing = Find-SigningCert -Hash $Thumbprint -SubjectName $CertSubject
    if ($existing) {
        $cert = $existing
        Write-Output ("Found existing certificate: " + $cert.Thumbprint)
    } else {
        $cert = New-SelfSignedCertificate `
            -Type CodeSigningCert `
            -Subject $CertSubject `
            -CertStoreLocation "Cert:\CurrentUser\My" `
            -NotAfter (Get-Date).AddYears(5)
        Write-Output ("Created self-signed code signing certificate: " + $cert.Thumbprint)
    }
    if (-not (Test-Path $signingDir)) {
        New-Item -ItemType Directory -Path $signingDir -Force | Out-Null
    }
    Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
    Write-Output ("Exported public cert to: " + $cerPath)
    Write-Output "Next (optional for CurrentUser trust): npm run sign:local -- -TrustCurrentUser"
    Write-Output ("Or pass thumbprint to packager: HOOKREPLAY_SIGN_THUMBPRINT=" + $cert.Thumbprint)
    if (-not $Files -or $Files.Count -eq 0) {
        if (-not $TrustCurrentUser) {
            exit 0
        }
    }
}

if ($TrustCurrentUser) {
    if (-not (Test-Path $cerPath)) {
        $cert = Find-SigningCert -Hash $Thumbprint -SubjectName $CertSubject
        if (-not $cert) {
            throw "Certificate not found. Run with -InitCert first."
        }
        if (-not (Test-Path $signingDir)) {
            New-Item -ItemType Directory -Path $signingDir -Force | Out-Null
        }
        Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
    }
    Import-Certificate -FilePath $cerPath -CertStoreLocation "Cert:\CurrentUser\Root" | Out-Null
    Import-Certificate -FilePath $cerPath -CertStoreLocation "Cert:\CurrentUser\TrustedPublisher" | Out-Null
    Write-Output "Trusted certificate under CurrentUser\Root and CurrentUser\TrustedPublisher."
    if ((-not $Files -or $Files.Count -eq 0) -and -not $PSBoundParameters.ContainsKey("Files")) {
        exit 0
    }
}

$certToUse = Find-SigningCert -Hash $Thumbprint -SubjectName $CertSubject
if (-not $certToUse) {
    throw "No matching Code Signing certificate found in Cert:\CurrentUser\My. Run -InitCert first or pass -Thumbprint."
}

$targetFiles = @()
if ($Files -and $Files.Count -gt 0) {
    $targetFiles = $Files
} else {
    $releaseDir = Join-Path $repoRoot "release"
    $portable = Join-Path $releaseDir "HookReplay.exe"
    if (Test-Path $portable) {
        $targetFiles += $portable
    }
    $installers = @(Get-ChildItem -Path $releaseDir -Filter "HookReplay_*_x64-setup.exe" -ErrorAction SilentlyContinue)
    foreach ($inst in $installers) {
        $targetFiles += $inst.FullName
    }
}

if ($targetFiles.Count -eq 0) {
    throw "No target .exe files found to sign. Build release binaries first or specify -Files."
}

foreach ($file in $targetFiles) {
    if (-not (Test-Path $file)) {
        throw "Target file does not exist: $file"
    }
    $sigParams = @{
        FilePath      = $file
        Certificate   = $certToUse
        HashAlgorithm = "SHA256"
    }
    if ($TimestampServer -and $TimestampServer.Trim()) {
        $sigParams["TimestampServer"] = $TimestampServer.Trim()
    }
    $sig = Set-AuthenticodeSignature @sigParams
    Write-Output ("Signed: " + $file + " | Status=" + $sig.Status + " | Thumbprint=" + $certToUse.Thumbprint)
}
