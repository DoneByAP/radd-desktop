$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
& (Join-Path $PSScriptRoot 'build-engine.ps1')
cargo build --manifest-path (Join-Path $projectDir 'Cargo.toml') --release --locked
if ($LASTEXITCODE -ne 0) { throw 'GUI build failed.' }
$bundleDir = Join-Path $projectDir 'dist/RADD-Desktop-Windows'
New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
Copy-Item -LiteralPath (Join-Path $projectDir 'target/release/radd-desktop.exe') -Destination $bundleDir
Copy-Item -LiteralPath (Join-Path $projectDir 'tools/radd.exe') -Destination $bundleDir
Copy-Item -LiteralPath (Join-Path $projectDir 'README.md') -Destination $bundleDir
Copy-Item -LiteralPath (Join-Path $projectDir 'LICENSE') -Destination $bundleDir
Copy-Item -LiteralPath (Join-Path $projectDir 'licenses') -Destination $bundleDir -Recurse -Force
Copy-Item -LiteralPath (Join-Path $projectDir 'samples') -Destination $bundleDir -Recurse -Force
Write-Host "Ready. Open $bundleDir\radd-desktop.exe"

