# Downloads a prebuilt PDFium shared library from bblanchon/pdfium-binaries and
# verifies it against a pinned SHA-256. Runs on Windows PowerShell and PowerShell 7 (pwsh).
#
# Usage: pwsh -File scripts/get-pdfium.ps1 [-Platform win-x64|mac-univ]
#
# Result: pdfium/<platform>/ with bin/pdfium.dll (Windows) or lib/libpdfium.dylib (macOS).
# To bump the version: change $Tag, download once, and update the hashes below
# (the script prints the SHA-256 of what it downloaded).
param(
    [ValidateSet("win-x64", "mac-univ")]
    [string]$Platform = "win-x64"
)

$ErrorActionPreference = "Stop"

$Tag = "chromium/8138"
$ExpectedSha256 = @{
    "win-x64"  = "8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5"
    "mac-univ" = "bbc85a8306409267bcca6d961f0b416cde1fac2423411f5c14ab02b0b3e689fd"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$archiveName = "pdfium-$Platform.tgz"
$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$Tag/$archiveName"
$destDir = Join-Path (Join-Path $repoRoot "pdfium") $Platform
$archivePath = Join-Path $destDir $archiveName

New-Item -ItemType Directory -Force -Path $destDir | Out-Null

Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing

$hash = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLower()
Write-Host "SHA-256: $hash"
if ($hash -ne $ExpectedSha256[$Platform]) {
    Remove-Item -Force $archivePath
    throw "SHA-256 mismatch for $archiveName (expected $($ExpectedSha256[$Platform]))"
}

Write-Host "Extracting to $destDir"
tar -xzf $archivePath -C $destDir
if ($LASTEXITCODE -ne 0) { throw "tar failed with exit code $LASTEXITCODE" }

Write-Host "Done. Library directory:"
if ($Platform -eq "win-x64") { Write-Host (Join-Path $destDir "bin") } else { Write-Host (Join-Path $destDir "lib") }
