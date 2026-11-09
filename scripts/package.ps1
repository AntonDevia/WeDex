$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $cargo)) { $cargo = 'cargo' }
& $cargo build --release --locked --bin sonora
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
$versionLine = Get-Content -LiteralPath 'Cargo.toml' | Where-Object { $_ -match '^version = "([^"]+)"' } | Select-Object -First 1
if ($versionLine -notmatch '^version = "([^"]+)"') { throw 'Package version missing' }
$outputDir = Join-Path (Get-Location) ('dist\WeDex-' + $Matches[1])
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
Copy-Item -LiteralPath 'target\release\sonora.exe' -Destination (Join-Path $outputDir 'WeDex-Sonora.exe')
foreach ($name in @('README.md','LICENSE','THIRD_PARTY.md')) { Copy-Item -LiteralPath $name -Destination $outputDir }
Copy-Item -LiteralPath 'docs' -Destination $outputDir -Recurse -Force
$licenseDir = Join-Path $outputDir 'licenses'
New-Item -ItemType Directory -Force -Path $licenseDir | Out-Null
$metadataText = & $cargo metadata --format-version 1 --locked
if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed' }
$metadata = $metadataText | ConvertFrom-Json
$inventory = @('# Rust dependency licenses', '', 'Inventory includes target-specific and build dependencies.', '')
foreach ($package in ($metadata.packages | Sort-Object name,version)) {
    if ($package.name -eq 'wedex') { continue }
    $inventory += ('- ' + $package.name + ' ' + $package.version + ' — ' + $package.license)
    $upstream = Split-Path -Parent $package.manifest_path
    $files = Get-ChildItem -LiteralPath $upstream -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)' }
    $packageDir = Join-Path $licenseDir ($package.name + '-' + $package.version)
    New-Item -ItemType Directory -Force -Path $packageDir | Out-Null
    foreach ($file in $files) { Copy-Item -LiteralPath $file.FullName -Destination $packageDir }
    if ($package.license_file) { $licenseFile = Join-Path $upstream $package.license_file; if (Test-Path -LiteralPath $licenseFile) { Copy-Item -LiteralPath $licenseFile -Destination $packageDir } }
}
$inventory | Set-Content -LiteralPath (Join-Path $licenseDir 'INVENTORY.md') -Encoding UTF8
Copy-Item -LiteralPath 'src\fingerprinting\LICENSE-MIT' -Destination (Join-Path $licenseDir 'ShazamIO-Core-MIT.txt')
Copy-Item -LiteralPath 'assets\fonts\medeo\OFL.txt' -Destination (Join-Path $licenseDir 'Medeo-OFL-1.1.txt')
Copy-Item -LiteralPath 'assets\fonts\medeo\AUTHORS.txt' -Destination (Join-Path $licenseDir 'Medeo-AUTHORS.txt')
Copy-Item -LiteralPath 'assets\fontawesome\LICENSE.txt' -Destination (Join-Path $licenseDir 'FontAwesome-Free.txt')
foreach ($entry in @(
    @('assets\providers\CC0.txt','Provider-icons-CC0.txt'),
    @('assets\providers\selfhst-LICENSE.txt','Provider-Yandex-CC-BY-4.0.txt'),
    @('design\assets\LUCIDE-LICENSE','Lucide-ISC.txt')
)) {
    if (Test-Path -LiteralPath $entry[0]) { Copy-Item -LiteralPath $entry[0] -Destination (Join-Path $licenseDir $entry[1]) }
}
Get-FileHash -LiteralPath (Join-Path $outputDir 'WeDex-Sonora.exe') -Algorithm SHA256 | Format-List
Write-Output $outputDir
