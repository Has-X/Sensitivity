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
        $artifacts = @(Properties $catalogs[$index] | Where-Object { [string]$_.Value -match '(__SENS|__СЕНС|ZZZPROT)' } | ForEach-Object Name)
        $placeholderMismatch = @()
        foreach ($source in Properties $catalogs[0]) {
            $expectedPlaceholders = @([regex]::Matches([string]$source.Value, '\{[^}]+\}') | ForEach-Object Value | Sort-Object)
            $actualProperty = $catalogs[$index].PSObject.Properties[$source.Name]
            if ($null -eq $actualProperty) { continue }
            $actualPlaceholders = @([regex]::Matches([string]$actualProperty.Value, '\{[^}]+\}') | ForEach-Object Value | Sort-Object)
            if (($expectedPlaceholders -join '|') -ne ($actualPlaceholders -join '|')) { $placeholderMismatch += $source.Name }
        }
        if ($missing.Count -or $extra.Count -or $empty.Count -or $artifacts.Count -or $placeholderMismatch.Count) {
            throw "$Name locale $($Paths[$index]) is inconsistent. Missing=$($missing -join ', '); extra=$($extra -join ', '); empty=$($empty -join ', '); artifacts=$($artifacts -join ', '); placeholder mismatch=$($placeholderMismatch -join ', ')"
        }
    }
    Write-Host "${Name}: $($expected.Count) keys, all locales complete"
}

$languages = @(Get-ChildItem (Join-Path $root 'locales') -Directory | Where-Object Name -ne '_keys' | Sort-Object Name | ForEach-Object Name)
$catalogPaths = { param($file) @($languages | ForEach-Object { "locales/$_/$file" }) }
Assert-Catalog 'CLI' (&$catalogPaths 'cli.json')
Assert-Catalog 'Portable GUI' (&$catalogPaths 'gui.json')

$aliases = Read-Json 'locales/_keys/windows.json'
$sources = @(Properties $aliases | ForEach-Object Value | Sort-Object -Unique)
$englishCatalog = Read-Json 'locales/en/windows.json'
$expectedWindows = @(Properties $englishCatalog | ForEach-Object Name | Sort-Object)
foreach ($lang in $languages) {
    $catalog = Read-Json "locales/$lang/windows.json"
    $actualWindows = @(Properties $catalog | ForEach-Object Name | Sort-Object)
    $missing = @($expectedWindows | Where-Object { $_ -notin $actualWindows })
    $extra = @($actualWindows | Where-Object { $_ -notin $expectedWindows })
    if ($missing.Count -or $extra.Count) { throw "Windows locale $lang is inconsistent. Missing=$($missing -join '; '); extra=$($extra -join '; ')" }
    $empty = @(Properties $catalog | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.Value) } | ForEach-Object Name)
    $artifacts = @(Properties $catalog | Where-Object { [string]$_.Value -match '(__SENS|__СЕНС|ZZZPROT)' } | ForEach-Object Name)
    $placeholderMismatch = @()
    foreach ($source in Properties $englishCatalog) {
        $expectedPlaceholders = @([regex]::Matches([string]$source.Value, '\{[^}]+\}') | ForEach-Object Value | Sort-Object)
        $actualProperty = $catalog.PSObject.Properties[$source.Name]
        if ($null -eq $actualProperty) { continue }
        $actualPlaceholders = @([regex]::Matches([string]$actualProperty.Value, '\{[^}]+\}') | ForEach-Object Value | Sort-Object)
        if (($expectedPlaceholders -join '|') -ne ($actualPlaceholders -join '|')) { $placeholderMismatch += $source.Name }
    }
    if ($empty.Count -or $artifacts.Count -or $placeholderMismatch.Count) { throw "Windows locale $lang has empty values, artifacts, or placeholder mismatches. Empty=$($empty -join '; '); artifacts=$($artifacts -join '; '); placeholders=$($placeholderMismatch -join '; ')" }
}
$unknownAliases = @($sources | Where-Object { $_ -notin $expectedWindows })
if ($unknownAliases.Count) { throw "Windows aliases reference unknown source keys: $($unknownAliases -join '; ')" }
Write-Host "Windows: $($expectedWindows.Count) source keys, all locales complete"
