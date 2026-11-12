# Compila cedro.exe e crea dist\Cedro-Setup-<versione>.exe (serve Inno Setup 6).
# Con -Arm64 crea dist\Cedro-ARM64-Setup-<versione>.exe per Windows on ARM: su un PC ARM compila in nativo,
# su un PC x64 con il target aarch64-pc-windows-msvc (servono gli strumenti ARM64 di Visual Studio e LLVM).
param([switch]$Arm64)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
$native = $env:PROCESSOR_ARCHITECTURE -eq 'ARM64'
$buildDir = '..\target\release'
if ($native -and -not $Arm64) { throw 'Su un PC ARM serve -Arm64: senza, il file si chiamerebbe come quello x64.' }
if ($Arm64 -and -not $native) {
  rustup target add aarch64-pc-windows-msvc
  cargo build --release --target aarch64-pc-windows-msvc
  $buildDir = '..\target\aarch64-pc-windows-msvc\release'
} else {
  cargo build --release
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
& $iscc @flags installer\cedro.iss
if ($LASTEXITCODE -ne 0) { throw 'ISCC fallito' }
$arm = $(if ($Arm64) { 'ARM64-' } else { '' })
$setup = "dist\Cedro-${arm}Setup-$version.exe"
Get-Item $setup
