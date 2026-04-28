$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

Write-Host "Building Rust workspace (debug)..."
$env:RUST_BACKTRACE = "1"
cargo build -p acord-windows
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$exe = Join-Path $root "target\debug\acord.exe"
if (-not (Test-Path $exe)) { throw "binary not found at $exe" }

# Same icon rasterization as build.ps1 — debug builds want the icon too.
$svg = Join-Path $root "assets\Acord.svg"
$png = Join-Path (Split-Path -Parent $exe) "icon.png"
if (Test-Path $svg) {
    if (Get-Command rsvg-convert -ErrorAction SilentlyContinue) {
        rsvg-convert --width 256 --height 256 $svg -o $png
    }
}

# Foreground exec so panic output (RUST_BACKTRACE=1) lands in this terminal
# rather than vanishing on a detached console. Debug builds use the
# `console` subsystem by default so stderr is wired up automatically.
Write-Host "Launching $exe ..."
& $exe
