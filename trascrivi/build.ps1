# Compila il motore di trascrizione in tutte le varianti per Windows.
# Uso:  powershell -File trascrivi\build.ps1 [-Out dist] [-SoloCpu]
# Servono Rust, CMake e Visual Studio Build Tools; bindgen vuole libclang (LIBCLANG_PATH: va bene la cartella
# clang\native di `pip install libclang`); per la variante "vulkan" anche il Vulkan SDK (VULKAN_SDK).
if (-not $env:LIBCLANG_PATH) { Write-Warning "LIBCLANG_PATH non impostata: se la compilazione dice 'Unable to find libclang', vedi l'intestazione." }
param([string]$Out = "dist", [switch]$SoloCpu)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
New-Item -ItemType Directory -Force -Path $Out | Out-Null

# Ogni variante usa istruzioni diverse; GGML_NATIVE=OFF evita che il PC di build decida al posto di chi lo usa.
$cpu = @{
  base       = @{ GGML_AVX = "OFF"; GGML_AVX2 = "OFF"; GGML_FMA = "OFF"; GGML_F16C = "OFF"; GGML_BMI2 = "OFF" }
  avx2       = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON" }
  avxvnni    = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX_VNNI = "ON" }
  avx512     = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX512 = "ON" }
  avx512vnni = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX512 = "ON"; GGML_AVX512_VNNI = "ON"; GGML_AVX512_VBMI = "ON" }
}
$variants = @($cpu.Keys)
if (-not $SoloCpu -and $env:VULKAN_SDK) { $variants += "vulkan" }

foreach ($name in $variants) {
  Write-Host "== $name"
  foreach ($k in (Get-ChildItem Env: | Where-Object { $_.Name -like "GGML_*" }).Name) { Remove-Item "Env:$k" }
  $env:GGML_NATIVE = "OFF"
  $features = @()
  if ($name -eq "vulkan") {
    # La scheda video fa il lavoro pesante: il processore resta sul minimo compatibile.
    foreach ($e in $cpu["avx2"].GetEnumerator()) { Set-Item "Env:$($e.Key)" $e.Value }
    $features = @("--features", "vulkan")
  } else {
    foreach ($e in $cpu[$name].GetEnumerator()) { Set-Item "Env:$($e.Key)" $e.Value }
  }
  $env:CARGO_TARGET_DIR = "target-$name"
  cargo build --release @features
  if ($LASTEXITCODE -ne 0) { throw "compilazione $name fallita" }
  Copy-Item "target-$name\release\cedro-trascrivi.exe" "$Out\cedro-trascrivi-$version-windows-x64-$name.exe" -Force
}
Write-Host "Pronto in $Out (versione $version)"
