# Conversioni di base dalla riga di comando, con i file di prova del repository.
# Uso: powershell -File prova-arm\conversioni.ps1 -Exe <cedro.exe>
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = "Continue"
$work = Join-Path $env:RUNNER_TEMP "conv"
Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $work | Out-Null
$fx = Resolve-Path tests\fixtures
$casi = @(
  @("scan.jpg", "png"), @("scan.jpg", "webp"), @("scan.jpg", "pdf"),
  @("circolare.pdf", "txt"), @("circolare.pdf", "docx"), @("circolare.pdf", "jpg"),
  @("nota.m4a", "mp3"), @("nota.flac", "wav"),
  @("audio.mp4", "mp3"), @("clip.mov", "mp4"), @("vp9.webm", "mp4"), @("clip.mkv", "mp4")
)
$fail = @()
foreach ($c in $casi) {
  $src = Join-Path $work $c[0]
  Copy-Item (Join-Path $fx $c[0]) $src -Force
  $before = (Get-ChildItem $work -File).Name
  $watch = [Diagnostics.Stopwatch]::StartNew()
  $out = & $Exe convert $src $c[1] 2>&1 | Out-String
  $code = $LASTEXITCODE
  $watch.Stop()
  $new = Get-ChildItem $work -File | Where-Object { $before -notcontains $_.Name -and $_.Length -gt 0 }
  $ok = ($code -eq 0) -and $new
  "{0} -> {1}: {2} ({3:N1} s) {4}" -f $c[0], $c[1], $(if ($ok) { "OK" } else { "ERRORE" }), $watch.Elapsed.TotalSeconds, (($new | ForEach-Object { "$($_.Name) $($_.Length)" }) -join ", ")
  if (-not $ok) { $out.Trim(); $fail += "$($c[0])->$($c[1])" }
}
if ($fail.Count) { throw "Conversioni fallite: $($fail -join '; ')" }
