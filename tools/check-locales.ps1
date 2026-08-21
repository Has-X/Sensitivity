[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Read-Json([string] $Path) {
    Get-Content (Join-Path $root $Path) -Raw | ConvertFrom-Json
}

function Properties($Object) {
    @($Object.PSObject.Properties)
}

function Assert-Catalog([string] $Name, [string[]] $Paths) {
    $catalogs = @($Paths | ForEach-Object { Read-Json $_ })
    $expected = @(Properties $catalogs[0] | ForEach-Object Name | Sort-Object)
    foreach ($index in 0..($catalogs.Count - 1)) {
        $actual = @(Properties $catalogs[$index] | ForEach-Object Name | Sort-Object)
        $missing = @($expected | Where-Object { $_ -notin $actual })
        $extra = @($actual | Where-Object { $_ -notin $expected })
        $empty = @(Properties $catalogs[$index] | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.Value) } | ForEach-Object Name)
        if ($missing.Count -or $extra.Count -or $empty.Count) {
            throw "$Name locale $($Paths[$index]) is inconsistent. Missing=$($missing -join ', '); extra=$($extra -join ', '); empty=$($empty -join ', ')"
        }
    }
    Write-Host "${Name}: $($expected.Count) keys, all locales complete"
}

Assert-Catalog 'CLI' @('locales/en.json', 'locales/hu.json', 'locales/es.json')
Assert-Catalog 'Portable GUI' @('crates/gui/locales/en.json', 'crates/gui/locales/hu.json', 'crates/gui/locales/es.json')

$aliases = Read-Json 'apps/windows/Sensitivity.WinUI/Resources/windows-keys.json'
$sources = @(Properties $aliases | ForEach-Object Value | Sort-Object -Unique)
foreach ($lang in 'en', 'hu', 'es') {
    $catalog = Read-Json "apps/windows/Sensitivity.WinUI/Resources/windows-$lang.json"
    $extraPath = "apps/windows/Sensitivity.WinUI/Resources/windows-$lang-extra.json"
    if (Test-Path (Join-Path $root $extraPath)) {
        $extra = Read-Json $extraPath
        foreach ($property in Properties $extra) { $catalog | Add-Member -NotePropertyName $property.Name -NotePropertyValue $property.Value -Force }
    }
    $missing = @($sources | Where-Object { $_ -notin @(Properties $catalog | ForEach-Object Name) })
    if ($missing.Count) { throw "Windows locale $lang is missing: $($missing -join '; ')" }
    $empty = @(Properties $catalog | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.Value) } | ForEach-Object Name)
    if ($empty.Count) { throw "Windows locale $lang has empty values: $($empty -join '; ')" }
}
Write-Host "Windows: $($sources.Count) source keys, all locales complete"
