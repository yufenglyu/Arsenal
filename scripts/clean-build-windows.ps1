<#
.SYNOPSIS
Preview or remove build intermediates; keep final executables and bundles.
.EXAMPLE
powershell -ExecutionPolicy Bypass -File scripts/clean-build-windows.ps1
.EXAMPLE
powershell -ExecutionPolicy Bypass -File scripts/clean-build-windows.ps1 -Run
#>
[CmdletBinding()]
param([switch]$Run)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\', '/')
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot 'src-tauri/Cargo.toml')) -or
    -not (Test-Path -LiteralPath (Join-Path $repoRoot 'package.json')) -or
    (Get-Content -LiteralPath (Join-Path $repoRoot 'package.json') -Raw | ConvertFrom-Json).name -ne 'arsenal') {
    throw 'Expected a Arsenal repository next to this script.'
}

function Assert-SafeTarget([string]$Path) {
    $absolute = [System.IO.Path]::GetFullPath($Path)
    if (-not $absolute.StartsWith($repoRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing cleanup outside the repository.'
    }
    $cursor = $absolute
    while ($cursor -ne $repoRoot) {
        if (Test-Path -LiteralPath $cursor) {
            if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw 'Refusing cleanup through a filesystem link.'
            }
        }
        $cursor = Split-Path -Parent $cursor
    }
}

function Assert-NoNestedLinks([string]$Path) {
    # Check without traversing junctions, including on Windows PowerShell 5.1.
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    if ((Get-Item -LiteralPath $Path -Force).PSIsContainer) { $pending.Push($Path) }
    while ($pending.Count -gt 0) {
        foreach ($item in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw 'Refusing cleanup of a directory containing filesystem links.'
            }
            if ($item.PSIsContainer) { $pending.Push($item.FullName) }
        }
    }
}

$targets = @('dist', '.vite', 'node_modules/.vite', 'node_modules/.vite-temp',
    'tsconfig.tsbuildinfo', 'tsconfig.node.tsbuildinfo', 'vite.config.js', 'vite.config.d.ts') |
    ForEach-Object { Join-Path $repoRoot $_ }
foreach ($relativeBase in @('src-tauri/target', 'target')) {
    $base = Join-Path $repoRoot $relativeBase
    Assert-SafeTarget $base
    if (-not (Test-Path -LiteralPath $base -PathType Container)) { continue }
    $profiles = @((Join-Path $base 'debug'), (Join-Path $base 'release'))
    foreach ($child in Get-ChildItem -LiteralPath $base -Directory -Force) {
        if ($child.Name -in @('debug', 'release')) { continue }
        $profiles += Join-Path $child.FullName 'debug'
        $profiles += Join-Path $child.FullName 'release'
    }
    foreach ($profile in $profiles) {
        foreach ($cache in @('.fingerprint', 'build', 'deps', 'incremental')) {
            $targets += Join-Path $profile $cache
        }
    }
}

$existing = @()
foreach ($target in ($targets | Select-Object -Unique)) {
    Assert-SafeTarget $target
    if (-not (Test-Path -LiteralPath $target)) { continue }
    $existing += $target
}
# Validate every deletion before making changes.
if ($Run) { foreach ($target in $existing) { Assert-NoNestedLinks $target } }
foreach ($target in $existing) {
    Write-Host ('  ' + $target.Substring($repoRoot.Length + 1))
    if ($Run) {
        Assert-SafeTarget $target
        Remove-Item -LiteralPath $target -Recurse -Force
    }
}
if ($Run) { Write-Host "Removed $($existing.Count) intermediate path(s)." }
else { Write-Host "Preview: $($existing.Count) path(s). Pass -Run to delete." }
Write-Host 'Kept final binaries, bundles, release-assets, dependencies and application data. Rebuild caches will be regenerated.'
