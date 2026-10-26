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

# Senza BOM: il motore legge una riga JSON pura (Cedro gli scrive così).
$OutputEncoding = New-Object System.Text.UTF8Encoding $false
$fail = @()
foreach ($exe in Get-ChildItem $Dir -Filter "cedro-trascrivi-*.exe") {
  Write-Host "== $($exe.Name)"
  $request = (@{ file = $wav; lang = "auto" } | ConvertTo-Json -Compress)
  $watch = [Diagnostics.Stopwatch]::StartNew()
  # whisper.cpp scrive i suoi messaggi su stderr: con «Stop» PowerShell 5 li tratterebbe come errori.
  $ErrorActionPreference = "Continue"
  $out = $request | & $exe.FullName --model $model 2>$null
  $ErrorActionPreference = "Stop"
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
