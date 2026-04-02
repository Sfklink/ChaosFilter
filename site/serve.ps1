$ErrorActionPreference = "Stop"

$port = 5173
Write-Host "Serving ChaosFilter site on http://localhost:$port"
Write-Host "Press Ctrl+C to stop."

if (Get-Command python -ErrorAction SilentlyContinue) {
  python -m http.server $port
  exit $LASTEXITCODE
}

if (Get-Command py -ErrorAction SilentlyContinue) {
  py -m http.server $port
  exit $LASTEXITCODE
}

throw "Python not found. Install Python or run any static file server in the ./site directory."

