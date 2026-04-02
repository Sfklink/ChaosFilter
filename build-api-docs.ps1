$ErrorActionPreference = "Stop"

function Require-Cmd($name) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    throw "Required command not found on PATH: $name"
  }
}

Require-Cmd cargo

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$siteDir = (Resolve-Path $PSScriptRoot).Path
$outDir = Join-Path $siteDir "api"

Write-Host "Building Cargo docs (no deps)..." -ForegroundColor Cyan
Push-Location $repoRoot
try {
  cargo doc --no-deps
} finally {
  Pop-Location
}

$builtDoc = Join-Path $repoRoot "target\\doc"
if (-not (Test-Path $builtDoc)) {
  throw "Expected docs at $builtDoc but it doesn't exist."
}

Write-Host "Copying docs to $outDir ..." -ForegroundColor Cyan
if (Test-Path $outDir) {
  Remove-Item -Recurse -Force $outDir
}
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Copy-Item -Recurse -Force (Join-Path $builtDoc "*") $outDir

Write-Host "Done. Open ./site/index.html and click 'Open API docs'." -ForegroundColor Green

