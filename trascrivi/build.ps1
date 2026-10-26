# Compila il motore di trascrizione in tutte le varianti per Windows (x64, oppure ARM64 se lo lanci su un PC ARM).
# Uso:  powershell -File trascrivi\build.ps1 [-Out dist] [-SoloCpu | -SoloVulkan] [-Target C:\ctv]
#
# Servono:
#  - Rust e Visual Studio Build Tools (con CMake e Ninja, già inclusi);
#  - su ARM64 anche LLVM/clang (ggml non compila con il compilatore di Microsoft per ARM);
#  - libclang per bindgen: LIBCLANG_PATH deve puntare a una cartella con libclang.dll
#    (va bene quella di `pip install libclang`: ...\site-packages\clang\native);
#  - per la variante "vulkan" il Vulkan SDK (VULKAN_SDK), da installare come amministratore.
# La cartella di compilazione deve essere CORTA (-Target): i sotto-progetti di CMake superano
# i 260 caratteri di Windows se stanno dentro cartelle lunghe.
param([string]$Out = "dist", [switch]$SoloCpu, [switch]$SoloVulkan, [string]$Target = "C:\ctv")
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
if (-not $env:LIBCLANG_PATH) { throw "LIBCLANG_PATH non impostata: vedi l'intestazione dello script." }
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path

# Ambiente di Visual Studio (compilatore per i sotto-progetti di CMake) e generatore Ninja.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
if (-not $vs) { throw "Visual Studio Build Tools non trovati." }
$arm = $env:PROCESSOR_ARCHITECTURE -eq "ARM64"
$arch = if ($arm) { "arm64" } else { "x64" }
$vcvars = Join-Path $vs ("VC\Auxiliary\Build\" + $(if ($arm) { "vcvarsarm64.bat" } else { "vcvars64.bat" }))
cmd /c "`"$vcvars`" >nul && set" | ForEach-Object { if ($_ -match '^([^=]+)=(.*)$') { Set-Item "Env:$($Matches[1])" $Matches[2] } }
$env:CMAKE_GENERATOR = "Ninja"

# Ogni variante usa istruzioni diverse; GGML_NATIVE=OFF evita che il PC di build decida al posto di chi lo usa.
$cpu = [ordered]@{
  base       = @{ GGML_AVX = "OFF"; GGML_AVX2 = "OFF"; GGML_FMA = "OFF"; GGML_F16C = "OFF"; GGML_BMI2 = "OFF" }
  avx2       = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON" }
  avxvnni    = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX_VNNI = "ON" }
  avx512     = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX512 = "ON" }
  avx512vnni = @{ GGML_AVX = "ON";  GGML_AVX2 = "ON";  GGML_FMA = "ON";  GGML_F16C = "ON";  GGML_BMI2 = "ON"; GGML_AVX512 = "ON"; GGML_AVX512_VNNI = "ON"; GGML_AVX512_VBMI = "ON" }
}
if ($arm) {
  # GGML_CPU_ARM_ARCH è il -march di clang: ogni variante abilita istruzioni in più.
  $cpu = [ordered]@{
    base    = @{ GGML_CPU_ARM_ARCH = "armv8.1-a" }
    dotprod = @{ GGML_CPU_ARM_ARCH = "armv8.2-a+dotprod+fp16" }
    i8mm    = @{ GGML_CPU_ARM_ARCH = "armv8.2-a+dotprod+fp16+i8mm" }
  }
  $env:CMAKE_C_COMPILER = "clang"
  $env:CMAKE_CXX_COMPILER = "clang++"
}
$variants = @($cpu.Keys)
if (-not $SoloCpu -and $env:VULKAN_SDK) { $variants += "vulkan" }
if ($SoloVulkan) {
  if (-not $env:VULKAN_SDK) { throw "VULKAN_SDK non impostata." }
  $variants = @("vulkan")
}

foreach ($name in $variants) {
  Write-Host "== $name"
  foreach ($k in (Get-ChildItem Env: | Where-Object { $_.Name -like "GGML_*" }).Name) { Remove-Item "Env:$k" }
  $env:GGML_NATIVE = "OFF"
  $features = @()
  if ($name -eq "vulkan") {
    # La scheda video fa il lavoro pesante: il processore resta sul minimo compatibile.
    $gpuCpu = if ($arm) { $cpu["dotprod"] } else { $cpu["avx2"] }
    foreach ($e in $gpuCpu.GetEnumerator()) { Set-Item "Env:$($e.Key)" $e.Value }
    $features = @("--features", "vulkan")
    $env:PATH = "$env:VULKAN_SDK\Bin;$env:PATH"
  } else {
    foreach ($e in $cpu[$name].GetEnumerator()) { Set-Item "Env:$($e.Key)" $e.Value }
  }
  $env:CARGO_TARGET_DIR = Join-Path $Target $name
  cargo build --release @features
  if ($LASTEXITCODE -ne 0) { throw "compilazione $name fallita" }
  Copy-Item (Join-Path $env:CARGO_TARGET_DIR "release\cedro-trascrivi.exe") "$Out\cedro-trascrivi-$version-windows-$arch-$name.exe" -Force
}
Write-Host "Pronto in $Out (versione $version)"
