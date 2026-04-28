$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root

$buildDir = Join-Path $root "build\bin"
New-Item -ItemType Directory -Force -Path $buildDir | Out-Null

Write-Host "Building Rust architectures..."
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc

cargo build --release -p acord-windows --target x86_64-pc-windows-msvc
cargo build --release -p acord-windows --target aarch64-pc-windows-msvc

$exeX64 = Join-Path $root "target\x86_64-pc-windows-msvc\release\acord.exe"
$exeArm64 = Join-Path $root "target\aarch64-pc-windows-msvc\release\acord.exe"

Copy-Item $exeX64 -Destination (Join-Path $buildDir "acord_x64.exe")
Copy-Item $exeArm64 -Destination (Join-Path $buildDir "acord_arm64.exe")

$svg = Join-Path $root "assets\Acord.svg"
$png = Join-Path $buildDir "icon.png"

if (Test-Path $svg) {
    if (Get-Command rsvg-convert -ErrorAction SilentlyContinue) {
        Write-Host "Rasterizing icon..."
        rsvg-convert --width 256 --height 256 $svg -o $png
    } else {
        Write-Host "rsvg-convert not found; skipping icon rasterization"
    }
}

Write-Host "Built binaries to: $buildDir"
