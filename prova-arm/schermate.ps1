# Apre le finestre principali di Cedro e ne fa una schermata ciascuna.
# Uso: powershell -File prova-arm\schermate.ps1 -Exe <cedro.exe> -Out <cartella> [-File <immagine per la vista>]
param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Out, [string]$File = "")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
New-Item -ItemType Directory -Force $Out | Out-Null

function Snap($name) {
  $b = [Windows.Forms.SystemInformation]::VirtualScreen
  $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
  $path = Join-Path $Out "$name.png"
  $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  $black = $true
  $chk = [Drawing.Bitmap]::FromFile($path)
  foreach ($x in 0..9) { foreach ($y in 0..9) { if ($chk.GetPixel([int]($x * $chk.Width / 10), [int]($y * $chk.Height / 10)).GetBrightness() -gt 0.02) { $black = $false } } }
  $chk.Dispose()
  "{0}: {1} byte{2}" -f $name, (Get-Item $path).Length, $(if ($black) { " (NERA: nessuna sessione grafica?)" } else { "" })
}

$fail = @()
$finestre = @(
  @{ nome = "conversioni"; args = @("conversioni") },
  @{ nome = "impostazioni"; args = @("impostazioni") },
  @{ nome = "guida"; args = @("guida") },
  @{ nome = "vista"; args = @("vista") + $(if ($File) { @($File) } else { @() }) },
  @{ nome = "icona-vicino-all-orologio"; args = @() }
)
foreach ($f in $finestre) {
  $p = Start-Process $Exe -ArgumentList $f.args -PassThru
  Start-Sleep -Seconds 7
  if ($p.HasExited) { "$($f.nome): il processo è uscito subito (codice $($p.ExitCode))"; $fail += $f.nome; continue }
  "{0}: finestra «{1}»" -f $f.nome, $p.MainWindowTitle
  Snap $f.nome
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
}
if ($fail.Count) { throw "Non partono: $($fail -join ', ')" }
