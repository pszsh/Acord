$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "Building Rust workspace (release)..."
cargo build --release -p acord-windows
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$exe = Join-Path $root "target\release\acord.exe"
if (-not (Test-Path $exe)) { throw "binary not found at $exe" }

# Rasterize the SVG icon next to the exe so load_window_icon picks it up.
# Falls back silently if rsvg-convert isn't installed.
$svg = Join-Path $root "assets\Acord.svg"
$png = Join-Path (Split-Path -Parent $exe) "icon.png"
if (Test-Path $svg) {
    if (Get-Command rsvg-convert -ErrorAction SilentlyContinue) {
        Write-Host "Rasterizing icon..."
        rsvg-convert --width 256 --height 256 $svg -o $png
    } else {
        Write-Host "rsvg-convert not found on PATH; skipping icon rasterization"
    }
}

Write-Host "Built: $exe"
