# Compila cedro.exe e crea dist\Cedro-Setup-<versione>.exe (serve Inno Setup 6).
# Con -Bondeno crea l'edizione per il Comune di Bondeno: dist\Cedro-Bondeno-Setup-<versione>.exe
# (con -Arm64 anche: dist\Cedro-Bondeno-ARM64-Setup-<versione>.exe).
# Con -Arm64 crea dist\Cedro-ARM64-Setup-<versione>.exe per Windows on ARM: su un PC ARM compila in nativo,
# su un PC x64 con il target aarch64-pc-windows-msvc (servono gli strumenti ARM64 di Visual Studio e LLVM).
param([switch]$Arm64, [switch]$Bondeno)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
$native = $env:PROCESSOR_ARCHITECTURE -eq 'ARM64'
$buildDir = '..\target\release'
$features = @(if ($Bondeno) { '--features', 'bondeno' })
if ($native -and -not $Arm64) { throw 'Su un PC ARM serve -Arm64: senza, il file si chiamerebbe come quello x64.' }
if ($Arm64 -and -not $native) {
  rustup target add aarch64-pc-windows-msvc
  cargo build --release --target aarch64-pc-windows-msvc @features
  $buildDir = '..\target\aarch64-pc-windows-msvc\release'
} else {
  cargo build --release @features
}
if ($LASTEXITCODE -ne 0) { throw 'compilazione fallita' }
# Il file con tutte le licenze (autori e testi) lo fa lo stesso Cedro dai sorgenti usati: finisce nell'installatore.
New-Item -ItemType Directory -Force dist | Out-Null
cargo run --release --example licenze -- "dist\Licenze di terze parti.txt"
if ($LASTEXITCODE -ne 0) { throw 'file delle licenze non creato' }
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 non trovato (winget install JRSoftware.InnoSetup)' }
$flags = @("/DAppVersion=$version", "/DBuildDir=$buildDir")
if ($Arm64) { $flags += '/DArm64' }
if ($Bondeno) { $flags += '/DBondeno' }
& $iscc @flags installer\cedro.iss
if ($LASTEXITCODE -ne 0) { throw 'ISCC fallito' }
$prefix = 'Cedro-' + $(if ($Bondeno) { 'Bondeno-' } else { '' }) + $(if ($Arm64) { 'ARM64-' } else { '' }) + 'Setup-'
Get-Item ("dist\" + $prefix + "$version.exe")
