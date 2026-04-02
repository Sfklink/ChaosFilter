$ErrorActionPreference = "Stop"

function Require-Cmd($name) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    throw "Required command not found on PATH: $name"
  }
}

Require-Cmd cargo

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$siteDir = (Resolve-Path $PSScriptRoot).Path
$builtDoc = Join-Path $repoRoot "target\\doc"
$crateDoc = Join-Path $builtDoc "chaosfilter"
$outDir = Join-Path $siteDir "chaosfilter"

Write-Host "Building Cargo docs (no deps)..." -ForegroundColor Cyan
Push-Location $repoRoot
try {
  cargo doc --no-deps -p chaosfilter
} finally {
  Pop-Location
}

if (-not (Test-Path $crateDoc)) {
  throw "Expected crate docs at $crateDoc — run `cargo doc --no-deps` from the workspace root."
}

Write-Host "Copying chaosfilter docs to $outDir ..." -ForegroundColor Cyan
if (Test-Path $outDir) {
  Remove-Item -Recurse -Force $outDir
}
Copy-Item -Recurse -Force $crateDoc $outDir

Write-Host "Done. Open ./site/index.html and click 'Open API docs'." -ForegroundColor Green
