$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$engineDir = Join-Path $projectDir 'vendor/radd'
$revision = '4fd4f59c9a442e6414c20ae3fd364a082e97f1ff'
foreach ($tool in @('git', 'cargo')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Install $tool first. See README.md." }
}
if (-not (Test-Path -LiteralPath $engineDir)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $projectDir 'vendor') | Out-Null
    git clone --no-checkout https://github.com/pnnl/radd.git $engineDir
    if ($LASTEXITCODE -ne 0) { throw 'Could not download RADD.' }
    git -C $engineDir checkout --detach $revision
    if ($LASTEXITCODE -ne 0) { throw 'Could not select the compatible RADD revision.' }
}
$actual = git -C $engineDir rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $actual -ne $revision) { throw "Expected RADD revision $revision in $engineDir. Move the existing vendor/radd folder aside and retry." }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'engine.Cargo.lock') -Destination (Join-Path $engineDir 'Cargo.lock')
# Rapstone's generated decoder tables can overflow rustc's default thread stack.
# Limit the override to the compiler invocation and preserve a caller's setting.
$previousRustStack = $env:RUST_MIN_STACK
try {
    if (-not $env:RUST_MIN_STACK) { $env:RUST_MIN_STACK = '67108864' }
    cargo build --manifest-path (Join-Path $engineDir 'Cargo.toml') --release --locked --bin radd --config profile.release.lto=false --config profile.release.package.rapstone.opt-level=0
    if ($LASTEXITCODE -ne 0) { throw 'RADD build failed. See the compiler error above and the prerequisites in README.md.' }
} finally {
    $env:RUST_MIN_STACK = $previousRustStack
}
$toolsDir = Join-Path $projectDir 'tools'
New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
Copy-Item -LiteralPath (Join-Path $engineDir 'target/release/radd.exe') -Destination $toolsDir
Write-Host "Engine ready: $toolsDir\radd.exe"

