# Prova vera dei motori del riassunto: ogni motore riassume una riunione breve (prova.json) con Qwen3.5-4B.
# Uso: powershell -File riassumi\prova.ps1 -Dir dist-motori -Model <file .gguf>
# Passa se ogni motore arriva in fondo («end») con i quattro titoli che Cedro legge.
param([string]$Dir = "dist", [Parameter(Mandatory)][string]$Model)
$ErrorActionPreference = "Stop"
$request = [IO.File]::ReadAllText((Join-Path $PSScriptRoot "prova.json")).Trim()
$fail = @()
foreach ($exe in Get-ChildItem $Dir -Filter "cedro-riassumi-*.exe") {
  Write-Host "== $($exe.Name)"
  $watch = [Diagnostics.Stopwatch]::StartNew()
  # Come Cedro: la richiesta in byte UTF-8 puri su stdin, stderr scartato.
  $psi = New-Object Diagnostics.ProcessStartInfo $exe.FullName, "--model `"$Model`""
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = New-Object Text.UTF8Encoding $false
  $proc = [Diagnostics.Process]::Start($psi)
  $errTask = $proc.StandardError.ReadToEndAsync()
  $bytes = (New-Object Text.UTF8Encoding $false).GetBytes($request + "`n")
  $proc.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
  $proc.StandardInput.Close()
  $out = $proc.StandardOutput.ReadToEnd() -split "`r?`n" | Where-Object { $_ }
  $proc.WaitForExit()
  $null = $errTask.Result
  $watch.Stop()
  $out | Where-Object { $_ -notmatch '"e":"(text|progress)"' } | ForEach-Object { Write-Host "   $($_.Substring(0, [Math]::Min(600, $_.Length)))" }
  $end = $out | Where-Object { $_ -like '*"e":"end"*' } | Select-Object -First 1
  $text = if ($end) { ($end | ConvertFrom-Json).text } else { "" }
  Write-Host ("   {0:N1} s" -f $watch.Elapsed.TotalSeconds)
  $ok = $true
  foreach ($t in "## In breve", "## Chi ha detto cosa", "## Decisioni", "## Da fare") { if (-not $text.Contains($t)) { $ok = $false } }
  if (-not $ok) { $fail += $exe.Name }
}
if ($fail.Count) { throw "Motori che non riassumono: $($fail -join ', ')" }
Write-Host "Tutti i motori riassumono."
