# Downloads a prebuilt PDFium shared library from bblanchon/pdfium-binaries.
# Usage: powershell -File scripts/get-pdfium.ps1 [-Platform win-x64] [-Tag chromium/8138]
param(
    [string]$Platform = "win-x64",
    [string]$Tag = "chromium/8138"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$archiveName = "pdfium-$Platform.tgz"
$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$Tag/$archiveName"
$destDir = Join-Path $repoRoot "pdfium\$Platform"
$archivePath = Join-Path $destDir $archiveName

New-Item -ItemType Directory -Force -Path $destDir | Out-Null

Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing

$hash = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLower()
$size = (Get-Item $archivePath).Length
Write-Host "SHA-256: $hash"
Write-Host "Size:    $size bytes"

Write-Host "Extracting to $destDir"
tar -xzf $archivePath -C $destDir
if ($LASTEXITCODE -ne 0) { throw "tar failed with exit code $LASTEXITCODE" }

Write-Host "Done. Contents:"
Get-ChildItem $destDir | Select-Object Name, Length | Format-Table -AutoSize
