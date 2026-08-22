[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $IsccPath,
    [switch] $SkipInstaller,
    [switch] $IncludeArm64,
    [switch] $SkipUiSmoke
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $OutputDirectory = Join-Path $root "local-artifacts\preflight-win-x64-$stamp"
}

function Invoke-Checked([string] $File, [string[]] $Arguments) {
    $output = & $File @Arguments 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "Command failed ($LASTEXITCODE): $File $($Arguments -join ' ')`n$output" }
}

function Assert-File([string] $Path) {
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Required release file is missing: $Path" }
}

Invoke-Checked cargo @('fmt', '--all', '--', '--check')
Invoke-Checked pwsh @('-NoProfile', '-File', 'tools/check-locales.ps1')
Invoke-Checked cargo @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')
Invoke-Checked cargo @('test', '--workspace', '--locked')
Invoke-Checked cargo @('check', '--manifest-path', 'fuzz/Cargo.toml', '--locked')
Invoke-Checked cargo @('build', '--workspace', '--release', '--locked')
Invoke-Checked cargo @('build', '-p', 'sensitivity', '--release', '--target', 'x86_64-pc-windows-msvc', '--locked')

$publishDirectory = Join-Path $OutputDirectory 'native-publish'
New-Item -ItemType Directory -Force -Path $publishDirectory | Out-Null
Invoke-Checked dotnet @(
    'publish', 'apps/windows/Sensitivity.WinUI/Sensitivity.WinUI.csproj', '-c', 'Release', '-r', 'win-x64',
    '-p:Platform=x64', '--self-contained', 'true', '-p:DebugType=None', '-p:DebugSymbols=false', '-o', $publishDirectory
)

Copy-Item 'target/x86_64-pc-windows-msvc/release/sensitivity.exe' (Join-Path $publishDirectory 'sensitivity-cli.exe') -Force
foreach ($relative in @('Sensitivity.exe', 'sensitivity-cli.exe', 'Sensitivity.pri', 'Resources/locales/en/windows.json', 'Assets/SensitivityIcon.png')) {
    Assert-File (Join-Path $publishDirectory $relative)
}

if (!$SkipUiSmoke) {
    $app = Join-Path $publishDirectory 'Sensitivity.exe'
    $appProcess = Start-Process -FilePath $app -WorkingDirectory $publishDirectory -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 5
    if ($appProcess.HasExited) { throw "Sensitivity exited during the startup smoke test with code $($appProcess.ExitCode)." }
    Stop-Process -Id $appProcess.Id -Force
}

$cli = Join-Path $publishDirectory 'sensitivity-cli.exe'
Invoke-Checked $cli @('--version')
foreach ($language in Get-ChildItem 'locales' -Directory | Where-Object Name -ne '_keys' | Select-Object -ExpandProperty Name) {
    $env:SENSITIVITY_LANG = $language
    Invoke-Checked $cli @('--help')
}
Remove-Item Env:SENSITIVITY_LANG -ErrorAction SilentlyContinue

if (!$SkipInstaller) {
    if ([string]::IsNullOrWhiteSpace($IsccPath)) {
        $IsccPath = Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 7\ISCC.exe'
    }
    Assert-File $IsccPath
    $installerDirectory = Join-Path $OutputDirectory 'installer'
    New-Item -ItemType Directory -Force -Path $installerDirectory | Out-Null
    $version = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).packages |
        Where-Object name -eq 'sensitivity' | Select-Object -First 1 -ExpandProperty version
    Invoke-Checked $IsccPath @(
        "/DAppVersion=$version", '/DAppArchitecture=x64', "/DSourceDir=$publishDirectory",
        "/DOutputDir=$installerDirectory", 'installer/Sensitivity.iss'
    )
    Assert-File (Join-Path $installerDirectory 'Sensitivity-Setup-x64.exe')
}

if ($IncludeArm64) {
    Invoke-Checked cargo @('build', '-p', 'sensitivity', '--release', '--target', 'aarch64-pc-windows-msvc', '--locked')
    $armPublishDirectory = Join-Path $OutputDirectory 'native-publish-arm64'
    New-Item -ItemType Directory -Force -Path $armPublishDirectory | Out-Null
    Invoke-Checked dotnet @(
        'publish', 'apps/windows/Sensitivity.WinUI/Sensitivity.WinUI.csproj', '-c', 'Release', '-r', 'win-arm64',
        '-p:Platform=ARM64', '--self-contained', 'true', '-p:DebugType=None', '-p:DebugSymbols=false', '-o', $armPublishDirectory
    )
    Copy-Item 'target/aarch64-pc-windows-msvc/release/sensitivity.exe' (Join-Path $armPublishDirectory 'sensitivity-cli.exe') -Force
    foreach ($relative in @('Sensitivity.exe', 'sensitivity-cli.exe', 'Sensitivity.pri', 'Resources/locales/en/windows.json', 'Assets/SensitivityIcon.png')) {
        Assert-File (Join-Path $armPublishDirectory $relative)
    }
    if (!$SkipInstaller) {
        $armInstallerDirectory = Join-Path $OutputDirectory 'installer-arm64'
        New-Item -ItemType Directory -Force -Path $armInstallerDirectory | Out-Null
        Invoke-Checked $IsccPath @(
            "/DAppVersion=$version", '/DAppArchitecture=arm64', "/DSourceDir=$armPublishDirectory",
            "/DOutputDir=$armInstallerDirectory", 'installer/Sensitivity.iss'
        )
        Assert-File (Join-Path $armInstallerDirectory 'Sensitivity-Setup-arm64.exe')
    }
}

$hashes = Get-ChildItem $OutputDirectory -Recurse -File | Get-FileHash -Algorithm SHA256 |
    ForEach-Object { "{0}  {1}" -f $_.Hash, $_.Path }
$hashes | Set-Content (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Encoding utf8
Write-Host "Preflight passed. Output: $OutputDirectory"
Write-Host "SHA-256 manifest: $(Join-Path $OutputDirectory 'SHA256SUMS.txt')"
