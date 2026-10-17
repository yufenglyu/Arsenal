param(
  [string]$Version,
  [string]$OutputDir = "release-assets",
  [switch]$SkipTests,
  [switch]$SkipInstall,
  [switch]$SkipBuild,
  [switch]$Clean,
  [switch]$Run,
  [switch]$All,
  [switch]$LibraryOnly,
  [switch]$VersionOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function RepoRoot {
  $scriptDir = Split-Path -Parent $PSCommandPath
  return (Resolve-Path (Join-Path $scriptDir "..")).Path
}

function Run {
  param([string]$Command, [string[]]$Arguments)
  Write-Host ">> $Command $($Arguments -join ' ')" -ForegroundColor Cyan
  & $Command @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed with exit code $LASTEXITCODE`: $Command $($Arguments -join ' ')"
  }
}

function Local-Bin {
  param([string]$Root, [string]$Name)
  $suffix = if (Test-IsWindows) { ".cmd" } else { "" }
  return Join-Path $Root "node_modules/.bin/$Name$suffix"
}

function Read-JsonFile {
  param([string]$Path)
  return (Read-TextFile $Path) | ConvertFrom-Json
}

function Read-TextFile {
  param([string]$Path)
  return [System.IO.File]::ReadAllText((Resolve-Path $Path).Path, [System.Text.Encoding]::UTF8)
}

function Write-TextFile {
  param([string]$Path, [string]$Value)
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText((Resolve-Path $Path).Path, $Value, $utf8NoBom)
}

function Replace-Text {
  param([string]$Path, [string]$Pattern, [string]$Replacement)
  $content = Read-TextFile $Path
  if (-not [regex]::IsMatch($content, $Pattern)) {
    throw "Pattern not found in $Path`: $Pattern"
  }
  $updated = [regex]::Replace($content, $Pattern, $Replacement)
  if ($updated -ne $content) {
    Write-TextFile $Path $updated
  }
}

function Normalize-Version {
  param([string]$InputVersion)
  $normalized = $InputVersion.Trim() -replace '^v', ''
  if ($normalized -notmatch '^\d+\.\d+\.\d+([-.+][0-9A-Za-z.-]+)?$') {
    throw "Version must look like SemVer, for example 0.10.1 or v0.10.1. Got: $InputVersion"
  }
  return $normalized
}

function Detect-Platform {
  if (Test-IsWindows) { return "windows" }
  if (Test-IsMacOS) { return "macos" }
  if (Test-IsLinux) { return "linux" }
  throw "Unsupported OS."
}

function Assert-WindowsHost {
  param([bool]$SkipActualBuild)
  if ($SkipActualBuild) { return }

  $hostPlatform = Detect-Platform
  if ($hostPlatform -ne "windows") {
    throw "Cannot package Windows assets on $hostPlatform. Tauri desktop bundles are host-specific; run this script on Windows."
  }
}

function Write-BuildOutputGuide {
  param([string]$Root, [string]$OutDir)
  Write-Host ""
  Write-Host "Build output layout:" -ForegroundColor Yellow
  Write-Host "  $(Join-Path $Root 'dist')"
  Write-Host "    Vite frontend build consumed by Tauri. Do not upload this as a desktop release."
  Write-Host "  $(Join-Path $Root 'src-tauri/target')"
  Write-Host "    Cargo/Tauri build cache plus raw bundle output. Useful for diagnostics, not the curated release folder."
  Write-Host "  $OutDir"
  Write-Host "    Final renamed release assets produced by this script. Use this directory for manual distribution."
  Write-Host ""
}

function Write-ReleaseAssetSummary {
  param([string]$OutDir)
  Write-Host ""
  Write-Host "Final release assets:" -ForegroundColor Green
  $assets = Get-ChildItem -Path $OutDir -File -ErrorAction SilentlyContinue | Sort-Object Name
  if ($assets.Count -eq 0) {
    Write-Host "  (none)"
  } else {
    foreach ($asset in $assets) {
      Write-Host "  $($asset.FullName)"
    }
  }
  Write-Host ""
}

function Test-IsWindows {
  $var = Get-Variable -Name IsWindows -ErrorAction SilentlyContinue
  if ($null -ne $var) { return [bool]$var.Value }
  return $env:OS -eq "Windows_NT"
}

function Test-IsMacOS {
  $var = Get-Variable -Name IsMacOS -ErrorAction SilentlyContinue
  if ($null -ne $var) { return [bool]$var.Value }
  return $false
}

function Test-IsLinux {
  $var = Get-Variable -Name IsLinux -ErrorAction SilentlyContinue
  if ($null -ne $var) { return [bool]$var.Value }
  return -not (Test-IsWindows)
}

function Assert-Compatible-AppIdentity {
  param([string]$Root)
  $tauriPath = Join-Path $Root "src-tauri/tauri.conf.json"
  $tauri = Read-JsonFile $tauriPath

  if ($tauri.identifier -ne "com.iamzhihuix.skillsmanage") {
    throw "Refusing to package: Tauri identifier changed. Keeping com.iamzhihuix.skillsmanage preserves existing app data."
  }
  if ($tauri.productName -ne "SkillsHub") {
    throw "Refusing to package: productName must remain SkillsHub. Keep the identifier and upgrade code stable for older installs."
  }
  if ($tauri.bundle.windows.wix.upgradeCode -ne "28d41c68-f4a4-5134-b959-34babea58f7f") {
    throw "Refusing to package: Windows Wix upgradeCode changed. Keeping it preserves MSI upgrades from older versions."
  }

  $pathUtils = Read-TextFile (Join-Path $Root "src-tauri/src/path_utils.rs")
  $lib = Read-TextFile (Join-Path $Root "src-tauri/src/lib.rs")
  if ($pathUtils -notmatch 'join\("\.skillshub"\)' -or $pathUtils -notmatch 'legacy_app_data_dir' -or $lib -notmatch 'migrate_legacy_app_data_if_needed') {
    throw "Refusing to package: app data must default to ~/.skillshub and keep legacy ~/.skillsmanage migration support."
  }
}

function Update-VersionFiles {
  param([string]$Root, [string]$NextVersion)

  Replace-Text `
    -Path (Join-Path $Root "package.json") `
    -Pattern '("version"\s*:\s*)"[^"]+"' `
    -Replacement "`${1}`"$NextVersion`""

  Replace-Text `
    -Path (Join-Path $Root "src-tauri/tauri.conf.json") `
    -Pattern '("version"\s*:\s*)"[^"]+"' `
    -Replacement "`${1}`"$NextVersion`""

  Replace-Text `
    -Path (Join-Path $Root "src-tauri/Cargo.toml") `
    -Pattern '(?ms)(\[package\]\s+name = "skillshub"\s+)version = "[^"]+"' `
    -Replacement "`${1}version = `"$NextVersion`""

  Replace-Text `
    -Path (Join-Path $Root "src/pages/SettingsView.tsx") `
    -Pattern 'const APP_VERSION = "[^"]+";' `
    -Replacement "const APP_VERSION = `"$NextVersion`";"

  $metainfoPath = Join-Path $Root "src-tauri/bundle/linux/com.iamzhihuix.skillsmanage.metainfo.xml"
  $metainfo = Read-TextFile $metainfoPath
  if ($metainfo -notmatch "<release version=`"$([regex]::Escape($NextVersion))`"") {
    $today = Get-Date -Format "yyyy-MM-dd"
    $releaseLine = "    <release version=`"$NextVersion`" date=`"$today`"/>"
    $metainfo = [regex]::Replace($metainfo, "(?m)^(\s*<releases>\s*)$", "`${1}`n$releaseLine`n", 1)
    Write-TextFile $metainfoPath $metainfo
  }

}

function Ensure-Dependencies {
  param([string]$Root)
  if ($SkipInstall) { return }
  if (-not (Test-Path (Join-Path $Root "node_modules"))) {
    Run "pnpm" @("install", "--frozen-lockfile")
  }
}

function Run-Checks {
  param([string]$Root)
  if ($SkipTests) { return }

  $tscCmd = Local-Bin $Root "tsc"
  if (Test-Path $tscCmd) {
    Run $tscCmd @("--noEmit")
  } else {
    Run "pnpm" @("typecheck")
  }
  Run "cargo" @("test", "--manifest-path", (Join-Path $Root "src-tauri/Cargo.toml"), "--no-run", "--message-format", "short", "-q")
}

function Build-Frontend {
  param([string]$Root)
  $viteCmd = Local-Bin $Root "vite"
  if (-not (Test-Path $viteCmd)) {
    throw "Vite executable not found at $viteCmd. Run pnpm install first."
  }
  Run $viteCmd @("build")
}

function Build-App {
  param([string]$Root)
  if ($SkipBuild) { return }

  Build-Frontend -Root $Root
  Invoke-PreparePackagedConfig -Root $Root

  $tauriCmd = Local-Bin $Root "tauri"
  if (-not (Test-Path $tauriCmd)) {
    throw "Tauri CLI executable not found at $tauriCmd. Run pnpm install first."
  }

  $skipBeforeBuildPath = Join-Path ([System.IO.Path]::GetTempPath()) "skillshub-tauri-build-$PID.json"
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($skipBeforeBuildPath, '{"build":{"beforeBuildCommand":""}}', $utf8NoBom)
  try {
    Run $tauriCmd @("build", "--target", "x86_64-pc-windows-msvc", "--bundles", "msi", "--no-sign", "--ci", "--config", $skipBeforeBuildPath)
  } finally {
    Remove-Item -LiteralPath $skipBeforeBuildPath -Force -ErrorAction SilentlyContinue
  }
}

function Copy-WindowsAssets {
  param([string]$Root, [string]$NextVersion, [string]$OutDir)
  $targetRoot = Join-Path $Root "src-tauri/target/x86_64-pc-windows-msvc/release"
  if (-not (Test-Path $targetRoot)) {
    $targetRoot = Join-Path $Root "src-tauri/target/release"
  }
  Write-Host "Tauri raw bundle output: $(Join-Path $targetRoot 'bundle')" -ForegroundColor DarkGray
  $msiDir = Join-Path $targetRoot "bundle/msi"
  $msiCandidates = @(Get-ChildItem -Path $msiDir -Recurse -Filter *.msi -ErrorAction SilentlyContinue)
  if ($msiCandidates.Count -eq 0) { throw "Windows MSI bundle not found under $targetRoot." }
  $msi = $msiCandidates |
    Where-Object { $_.Name -match [regex]::Escape($NextVersion) } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
  if ($null -eq $msi) {
    $msi = $msiCandidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  }
  Write-Host "Using MSI: $($msi.FullName) (LastWriteTime $($msi.LastWriteTime))" -ForegroundColor DarkGray
  Copy-Item $msi.FullName (Join-Path $OutDir "skillshub_${NextVersion}_windows_x64.msi") -Force

  $exe = Join-Path $targetRoot "skillshub.exe"
  $configDir = Join-Path $Root "src-tauri/resources/packaged-config"
  New-SkillshubPortableZip `
    -ExePath $exe `
    -ConfigDir $configDir `
    -ZipPath (Join-Path $OutDir "skillshub_${NextVersion}_windows_x64.zip")
}

function Invoke-PreparePackagedConfig {
  param([string]$Root)
  $dest = Join-Path $Root "src-tauri/resources/packaged-config"
  New-Item -ItemType Directory -Force -Path $dest | Out-Null
  $manifest = Join-Path $Root "src-tauri/Cargo.toml"
  Write-Host ">> cargo run --manifest-path $manifest --bin prepare_config_dir --release --features prepare-config -- $dest" -ForegroundColor Cyan
  & cargo run --manifest-path $manifest --bin prepare_config_dir --release --features prepare-config -- $dest
  if ($LASTEXITCODE -ne 0) {
    throw "Failed to prepare default .skillshub config directory"
  }
}

function New-SkillshubPortableZip {
  param(
    [string]$ExePath,
    [string]$ConfigDir,
    [string]$ZipPath
  )
  if (-not (Test-Path $ExePath)) {
    throw "Windows executable not found at $ExePath"
  }
  if (-not (Test-Path (Join-Path $ConfigDir "config.json"))) {
    throw "Packaged config directory not found at $ConfigDir. Run prepare_config_dir first."
  }

  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem

  $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("skillshub-portable-" + [guid]::NewGuid().ToString("n"))
  New-Item -ItemType Directory -Force -Path $stage | Out-Null
  try {
    Copy-Item $ExePath (Join-Path $stage "skillshub.exe") -Force
    Copy-Item $ConfigDir (Join-Path $stage ".skillshub") -Recurse -Force
    $keep = Join-Path $stage ".skillshub/.gitkeep"
    if (Test-Path $keep) {
      Remove-Item $keep -Force
    }
    if (Test-Path $ZipPath) {
      Remove-Item $ZipPath -Force
    }
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
      $stage,
      $ZipPath,
      [System.IO.Compression.CompressionLevel]::Optimal,
      $false
    )
  } finally {
    $stagePath = [System.IO.Path]::GetFullPath($stage)
    $tempPrefix = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $stagePath.StartsWith($tempPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to remove staging directory outside temporary root"
    }
    Remove-Item -LiteralPath $stagePath -Recurse -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-CleanArtifacts {
  param([string]$Root)
# Safety: refuse to run outside the SkillsHub repo
if (-not ((Test-Path (Join-Path $Root 'package.json')) -and (Test-Path (Join-Path $Root 'src-tauri\Cargo.toml')))) {
    Write-Host "ERROR: package.json / src-tauri/Cargo.toml not found under $Root"
    Write-Host "       This script must run inside the SkillsHub repository."
    exit 1
}

$DefaultTargets = @(
    'src-tauri\target\debug'
    'src-tauri\target\x86_64-pc-windows-msvc'
    'src-tauri\target\flycheck0'
    'dist'
    'node_modules\.vite'
)

$ExtraTargets = @(
    'src-tauri\target\release'
    'node_modules'
)

$Targets = @($DefaultTargets)
if ($All) { $Targets += $ExtraTargets }

function Get-DirSize {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $null }
    # robocopy in list-only mode is much faster than Get-ChildItem -Recurse on huge trees
    $output = robocopy $Path NULL /L /S /NJH /BYTES /NP /NFL /NDL /NC 2>$null
    $bytesLine = $output | Where-Object { $_ -match '^\s*Bytes\s*:' } | Select-Object -Last 1
    if ($bytesLine -match '^\s*Bytes\s*:\s*(\d+)') {
        return [int64]$Matches[1]
    }
    # fallback: slow but reliable
    return (Get-ChildItem -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum
}

function Format-Size {
    param([long]$Bytes)
    if ($Bytes -ge 1GB) { return ('{0:N2} GB' -f ($Bytes / 1GB)) }
    elseif ($Bytes -ge 1MB) { return ('{0:N2} MB' -f ($Bytes / 1MB)) }
    elseif ($Bytes -ge 1KB) { return ('{0:N2} KB' -f ($Bytes / 1KB)) }
    return "$Bytes B"
}

if ($All) {
    Write-Host 'Mode: FULL clean (includes release build and node_modules)'
} else {
    Write-Host 'Mode: default clean (keeps target/release and node_modules)'
}
if (-not $Run) {
    Write-Host 'Mode: DRY-RUN (nothing will be deleted; pass -Run to execute)'
}
Write-Host "Repo: $Root"
Write-Host ('-' * 59)

$totalBytes = [long]0
$existing = @()

foreach ($rel in $Targets) {
    $rootPath = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\')
    $abs = [System.IO.Path]::GetFullPath((Join-Path $rootPath $rel))
    if (-not $abs.StartsWith($rootPath + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing cleanup outside repository"
    }
    if (-not (Test-Path $abs)) {
        Write-Host ("  [skip] {0,-45} (not present)" -f $rel)
        continue
    }
    $ancestor = $abs
    while ($ancestor -ne $rootPath) {
        if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            throw "Refusing cleanup through a filesystem link"
        }
        $ancestor = Split-Path -Parent $ancestor
    }
    $bytes = Get-DirSize $abs
    $size = Format-Size $bytes
    Write-Host ("  [   OK] {0,-45} {1}" -f $rel, $size)
    $totalBytes += $bytes
    $existing += $abs
}

Write-Host ('-' * 59)
Write-Host ("Total reclaimable: {0} across {1} path(s)" -f (Format-Size $totalBytes), $existing.Count)

if (-not $Run) {
    Write-Host ''
    Write-Host 'Dry-run only. To actually delete, run:'
    Write-Host '  .\scripts\package-release-windows.ps1 -Clean -Run          (default targets)'
    Write-Host '  .\scripts\package-release-windows.ps1 -Clean -Run -All     (also release + node_modules)'
    exit 0
}

if ($existing.Count -eq 0) {
    Write-Host 'Nothing to delete.'
    exit 0
}

Write-Host ''
Write-Host 'Deleting...'
foreach ($abs in $existing) {
    Write-Host "  Remove-Item -Recurse -Force $abs"
    Remove-Item -LiteralPath $abs -Recurse -Force
}

Write-Host ("Done. Reclaimed about {0}." -f (Format-Size $totalBytes))
if ($All) {
    Write-Host "Reminder: run 'pnpm install' before the next dev/build."
}

}

if ($LibraryOnly) { return }

$root = RepoRoot
if ($Clean) { Invoke-CleanArtifacts -Root $root; return }
if ($Run -or $All) { throw "-Run/-All require -Clean" }
Set-Location $root

$package = Read-JsonFile (Join-Path $root "package.json")
$nextVersion = if ($Version) { Normalize-Version $Version } else { Normalize-Version $package.version }

Update-VersionFiles -Root $root -NextVersion $nextVersion
Assert-Compatible-AppIdentity -Root $root

if ($VersionOnly) {
  Write-Host "Version files updated to $nextVersion. Packaging skipped because -VersionOnly was set." -ForegroundColor Green
  exit 0
}

Assert-WindowsHost -SkipActualBuild ([bool]$SkipBuild)
$outPath = Join-Path $root $OutputDir
New-Item -ItemType Directory -Force -Path $outPath | Out-Null
Write-BuildOutputGuide -Root $root -OutDir $outPath

Ensure-Dependencies -Root $root
Run-Checks -Root $root

if (-not $SkipBuild) {
  $pluginArgs = @((Join-Path $root "scripts/package-obsidian.mjs"), "--output-dir", $outPath)
  if ($SkipInstall) { $pluginArgs += "--skip-install" }
  if ($SkipTests) { $pluginArgs += "--skip-tests" }
  Run "node" $pluginArgs
}

Write-Host "Packaging target: windows" -ForegroundColor Yellow
Build-App -Root $root

if ($SkipBuild) {
  Write-Host "Skipping asset copy for windows because -SkipBuild was set." -ForegroundColor DarkYellow
  exit 0
}

Copy-WindowsAssets -Root $root -NextVersion $nextVersion -OutDir $outPath
Write-Host "Packaged windows assets in $outPath" -ForegroundColor Green
Write-ReleaseAssetSummary -OutDir $outPath
