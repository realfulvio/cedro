# Prova vera dei motori compilati: una voce sintetica di Windows dice una frase, ogni motore la trascrive.
# Uso: powershell -File trascrivi\prova.ps1 -Dir dist-motori
# Passa se ogni motore arriva in fondo («end») e, se richiesto, se il testo contiene la parola attesa.
param([string]$Dir = "dist", [string]$Atteso = "weather")
$ErrorActionPreference = "Stop"
$work = Join-Path $env:RUNNER_TEMP "prova-trascrivi"
New-Item -ItemType Directory -Force $work | Out-Null

$model = Join-Path $work "ggml-tiny.bin"
if (-not (Test-Path $model)) {
  Invoke-WebRequest https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin -OutFile $model
}

# Frase in inglese con la voce di sistema (il modello tiny la riconosce bene).
$wav = Join-Path $work "frase.wav"
Add-Type -AssemblyName System.Speech
$voice = New-Object System.Speech.Synthesis.SpeechSynthesizer
"Voci: " + (($voice.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }) -join ", ")
$voice.SetOutputToWaveFile($wav)
$voice.Speak("Good morning. The weather today is sunny and warm. This is a test of the transcription engine.")
$voice.Dispose()
"Audio: $((Get-Item $wav).Length) byte"

$fail = @()
foreach ($exe in Get-ChildItem $Dir -Filter "cedro-trascrivi-*.exe") {
  Write-Host "== $($exe.Name)"
  $request = (@{ file = $wav; lang = "auto" } | ConvertTo-Json -Compress)
  Write-Host "   richiesta: $request"
  $watch = [Diagnostics.Stopwatch]::StartNew()
  # Processo lanciato a mano: la richiesta arriva come byte UTF-8 puri (senza BOM né codifiche della console),
  # come la scrive Cedro; i messaggi di whisper.cpp su stderr vengono scartati.
  $psi = New-Object Diagnostics.ProcessStartInfo $exe.FullName, "--model `"$model`""
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $proc = [Diagnostics.Process]::Start($psi)
  $errTask = $proc.StandardError.ReadToEndAsync()
  $bytes = (New-Object Text.UTF8Encoding $false).GetBytes($request + "`n")
  $proc.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
  $proc.StandardInput.Close()
  $out = $proc.StandardOutput.ReadToEnd() -split "`r?`n" | Where-Object { $_ }
  $proc.WaitForExit()
  $null = $errTask.Result
  $watch.Stop()
  $out | ForEach-Object { Write-Host "   $_" }
  $events = $out | Where-Object { $_ -like '{*' } | ForEach-Object { $_ | ConvertFrom-Json }
  $text = (($events | Where-Object { $_.e -eq 'seg' }).text -join ' ')
  $ended = [bool]($events | Where-Object { $_.e -eq 'end' })
  Write-Host ("   {0:N1} s, fine: {1}, testo: {2}" -f $watch.Elapsed.TotalSeconds, $ended, $text)
  if (-not $ended -or ($Atteso -and $text -notmatch $Atteso)) { $fail += $exe.Name }
}
if ($fail.Count) { throw "Motori che non trascrivono: $($fail -join ', ')" }
Write-Host "Tutti i motori trascrivono."
