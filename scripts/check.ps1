$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $cargo)) { $cargo = 'cargo' }
& $cargo fmt -- --check
if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed' }
& $cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
& $cargo test --locked
if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
