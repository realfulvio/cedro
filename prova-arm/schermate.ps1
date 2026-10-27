# Apre le finestre principali di Cedro e ne fa una schermata ciascuna (della finestra, con PrintWindow,
# e di tutto lo schermo).
# Uso: powershell -File prova-arm\schermate.ps1 -Exe <cedro.exe> -Out <cartella> [-File <immagine per la vista>]
param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Out, [string]$File = "")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[void][W]::SetProcessDPIAware()
New-Item -ItemType Directory -Force $Out | Out-Null

function Say($t) { Write-Host $t }
function Describe($path) {
  $b = [Drawing.Bitmap]::FromFile($path)
  $colori = @{}
  foreach ($x in 0..39) { foreach ($y in 0..39) { $colori[$b.GetPixel([int]($x * ($b.Width - 1) / 39), [int]($y * ($b.Height - 1) / 39)).ToArgb()] = 1 } }
  $r = "{0}x{1}, {2} colori su 1600 punti, {3} byte" -f $b.Width, $b.Height, $colori.Count, (Get-Item $path).Length
  $b.Dispose(); $r
}

function Snap($name, $proc) {
  $b = [Windows.Forms.SystemInformation]::VirtualScreen
  $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
  $bmp.Save((Join-Path $Out "$name-schermo.png"), [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say ("  schermo: " + (Describe (Join-Path $Out "$name-schermo.png")))
  if ($proc.HasExited) { Say "  (nessuna finestra: il processo è già uscito)"; return $false }
  $proc.Refresh()
  $h = $proc.MainWindowHandle
  if ($h -eq [IntPtr]::Zero) { Say "  nessuna finestra principale"; return $false }
  $r = New-Object W+RECT
  [void][W]::GetWindowRect($h, [ref]$r)
  $w = $r.R - $r.L; $hh = $r.B - $r.T
  Say ("  finestra: {0}x{1} a ({2},{3})" -f $w, $hh, $r.L, $r.T)
  if ($w -lt 50 -or $hh -lt 50) { return $false }
  $bmp = New-Object Drawing.Bitmap $w, $hh
  $g = [Drawing.Graphics]::FromImage($bmp)
  $dc = $g.GetHdc()
  [void][W]::PrintWindow($h, $dc, 2)
  $g.ReleaseHdc($dc); $g.Dispose()
  $path = Join-Path $Out "$name.png"
  $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  Say ("  finestra: " + (Describe $path))
  return $true
}

$fail = @()
$finestre = @(
  @{ nome = "conversioni"; args = @("conversioni"); obbl = $true },
  @{ nome = "impostazioni"; args = @("impostazioni"); obbl = $true },
  @{ nome = "guida"; args = @("guida"); obbl = $true },
  # La finestra di scelta della modalità c'è solo dalle versioni che la contengono.
  @{ nome = "vista"; args = @("vista"); obbl = $false },
  @{ nome = "icona-vicino-all-orologio"; args = $null; obbl = $true }
)
foreach ($f in $finestre) {
  Say "== $($f.nome)"
  $p = if ($f.args) { Start-Process $Exe -ArgumentList $f.args -PassThru } else { Start-Process $Exe -PassThru }
  Start-Sleep -Seconds 6
  if ($p.HasExited) {
    Say "  il processo è uscito subito (codice $($p.ExitCode))"
    if ($f.obbl -and $f.args) { $fail += $f.nome }
    else { $null = Snap $f.nome $p }
    continue
  }
  $ok = Snap $f.nome $p
  if ($f.obbl -and $f.nome -ne "icona-vicino-all-orologio" -and -not $ok) { $fail += $f.nome }
  Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 1
}
if ($fail.Count) { throw "Problemi con: $($fail -join ', ')" }
