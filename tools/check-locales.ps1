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

Assert-Catalog 'CLI' @('locales/en/cli.json', 'locales/hu/cli.json', 'locales/es/cli.json')
Assert-Catalog 'Portable GUI' @('locales/en/gui.json', 'locales/hu/gui.json', 'locales/es/gui.json')

$aliases = Read-Json 'locales/_keys/windows.json'
$sources = @(Properties $aliases | ForEach-Object Value | Sort-Object -Unique)
$englishCatalog = Read-Json 'locales/en/windows.json'
$expectedWindows = @(Properties $englishCatalog | ForEach-Object Name | Sort-Object)
foreach ($lang in 'en', 'hu', 'es') {
    $catalog = Read-Json "locales/$lang/windows.json"
    $actualWindows = @(Properties $catalog | ForEach-Object Name | Sort-Object)
    $missing = @($expectedWindows | Where-Object { $_ -notin $actualWindows })
    $extra = @($actualWindows | Where-Object { $_ -notin $expectedWindows })
    if ($missing.Count -or $extra.Count) { throw "Windows locale $lang is inconsistent. Missing=$($missing -join '; '); extra=$($extra -join '; ')" }
    $empty = @(Properties $catalog | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.Value) } | ForEach-Object Name)
    if ($empty.Count) { throw "Windows locale $lang has empty values: $($empty -join '; ')" }
}
$unknownAliases = @($sources | Where-Object { $_ -notin $expectedWindows })
if ($unknownAliases.Count) { throw "Windows aliases reference unknown source keys: $($unknownAliases -join '; ')" }
Write-Host "Windows: $($expectedWindows.Count) source keys, all locales complete"
