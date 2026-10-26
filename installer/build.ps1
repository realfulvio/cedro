# Compila cedro.exe e crea dist\Cedro-Setup-<versione>.exe (serve Inno Setup 6).
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
cargo build --release
# Il file con tutte le licenze (autori e testi) lo fa lo stesso Cedro dai sorgenti usati: finisce nell'installatore.
New-Item -ItemType Directory -Force dist | Out-Null
cargo run --release --example licenze -- "dist\Licenze di terze parti.txt"
if ($LASTEXITCODE -ne 0) { throw 'file delle licenze non creato' }
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 non trovato (winget install JRSoftware.InnoSetup)' }
& $iscc "/DAppVersion=$version" installer\cedro.iss
if ($LASTEXITCODE -ne 0) { throw 'ISCC fallito' }
Get-Item "dist\Cedro-Setup-$version.exe"
