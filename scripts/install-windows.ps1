# Copyright (C) 2026 HasX
# Licensed under the GNU AGPL v3.0. See LICENSE file for details.

[CmdletBinding()]
param(
    [string]$SourceDir = $PSScriptRoot,
    [switch]$NoPath,
    [switch]$NoShortcut
)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $SourceDir).Path
$installRoot = Join-Path $env:LOCALAPPDATA 'Sensitivity'
$binDir = Join-Path $installRoot 'bin'
$romDir = Join-Path $installRoot 'roms'
$fileSources = @{
    'sensitivity.exe' = @('sensitivity.exe', 'sensitivity-windows-x86_64.exe')
    'sensitivity-gui.exe' = @('sensitivity-gui.exe', 'sensitivity-gui-windows-x86_64.exe')
}

$resolvedFiles = @{}
foreach ($destinationName in $fileSources.Keys) {
    $candidate = $fileSources[$destinationName] |
        ForEach-Object { Join-Path $source $_ } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
        Select-Object -First 1
    if (-not $candidate) {
        throw "Missing $destinationName in $source. Expected one of: $($fileSources[$destinationName] -join ', ')"
    }
    $resolvedFiles[$destinationName] = $candidate
}

New-Item -ItemType Directory -Force -Path $binDir, $romDir | Out-Null
foreach ($destinationName in $resolvedFiles.Keys) {
    Copy-Item -LiteralPath $resolvedFiles[$destinationName] -Destination (Join-Path $binDir $destinationName) -Force
}

if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $pathEntries = @($userPath -split ';' | Where-Object { $_ })
    if ($pathEntries -notcontains $binDir) {
        [Environment]::SetEnvironmentVariable('Path', ($pathEntries + $binDir -join ';'), 'User')
    }
    $env:Path = "$binDir;$env:Path"
}

if (-not $NoShortcut) {
    $startMenu = [Environment]::GetFolderPath('Programs')
    $shortcutPath = Join-Path $startMenu 'Sensitivity.lnk'
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($shortcutPath)
    $shortcut.TargetPath = Join-Path $binDir 'sensitivity-gui.exe'
    $shortcut.WorkingDirectory = $binDir
    $shortcut.Description = 'Sensitivity Recovery ROM validation and sideload'
    $shortcut.Save()
}

Write-Host "Sensitivity installed to $installRoot"
Write-Host "ROM storage: $romDir"
if (-not $NoPath) {
    Write-Host 'Open a new terminal to use sensitivity from PATH.'
}
