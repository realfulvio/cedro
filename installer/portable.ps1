# Crea dist\Cedro-Portable-<versione>-x64.zip: Cedro che parte senza installazione (Windows x64).
# Serve target\release\cedro.exe e dist\Licenze di terze parti.txt (li crea installer\build.ps1).
# Dentro lo zip c'è la cartella «Cedro-Portable» con cedro.exe, il file cedro.portable (è lui a far lavorare Cedro
# in modalità portable: dati e impostazioni nella cartella «Dati» accanto a cedro.exe) e le licenze.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$exe = 'target\release\cedro.exe'
if (-not (Test-Path $exe)) { throw "manca $exe (compila prima con cargo build --release)" }
$stage = Join-Path $env:TEMP "cedro-portable-$PID"
$root = Join-Path $stage 'Cedro-Portable'
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $root | Out-Null
Copy-Item $exe $root
Copy-Item LICENSE $root
Copy-Item 'dist\Licenze di terze parti.txt' $root
New-Item -ItemType File (Join-Path $root 'cedro.portable') | Out-Null
@"
Cedro $version - versione portable (Windows x64)

Apri cedro.exe: non c'e' niente da installare.
Impostazioni, registro, modelli e programmi scaricati (trascrizione, video dal web...) stanno nella cartella
"Dati" accanto a cedro.exe: per spostare Cedro su un'altra cartella o su una chiavetta porta tutta la cartella.
Cedro si aggiorna da solo sostituendo i file di questa cartella (Dati resta com'e').
Per toglierlo basta cancellare la cartella.
"@ | Set-Content -Encoding UTF8 (Join-Path $root 'LEGGIMI.txt')
New-Item -ItemType Directory -Force dist | Out-Null
$zip = "dist\Cedro-Portable-$version-x64.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path $root -DestinationPath $zip
Remove-Item $stage -Recurse -Force
Get-Item $zip
