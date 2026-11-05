param([string]$Exe = "$env:LOCALAPPDATA\Programs\Cedro\cedro.exe", [string]$Out = "$PWD")
# Tray di Windows: la ruota con Maiusc+trascinamento prima e dopo la pulizia della memoria a finestre chiuse.
# L'input è simulato nella sessione utente (serve un desktop vero). Esce con 1 se la ruota non si apre.
$ErrorActionPreference = 'Continue'
$log = Join-Path $Out "ruota.log"
function L($t) { "$(Get-Date -Format HH:mm:ss) $t" | Tee-Object -FilePath $log -Append }
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class W {
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EP f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr e);
  public static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassNameW(h, s, 256); return s.ToString(); }
  public static List<IntPtr> All(uint pid) { var r = new List<IntPtr>(); EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid) r.Add(h); return true; }, IntPtr.Zero); return r; }
}
"@
function Shot($name) {
  try {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $Out "$name.png")); $g.Dispose(); $bmp.Dispose()
  } catch { L "schermata $name non riuscita: $_" }
}
function Mem($t) {
  $p = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  if (!$p) { L "$t : CEDRO CHIUSO"; return }
  L ("{0,-28} WS {1,6} MB  Priv {2,6} MB  Thr {3}" -f $t, [math]::Round($p.WorkingSet64/1MB,1), [math]::Round($p.PrivateMemorySize64/1MB,1), $p.Threads.Count)
}
function Wheel() { @([W]::All($proc.Id) | Where-Object { [W]::Cls($_) -eq 'CedroWheel' -and [W]::IsWindowVisible($_) }).Count }
function Gesture($n) {
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
  $x = [int]($b.Width * 0.5); $y = [int]($b.Height * 0.45)
  [void][W]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 300
  [W]::keybd_event(0x10, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 150      # Maiusc giù
  [W]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 100   # sinistro giù
  for ($i = 1; $i -le 12; $i++) { [W]::mouse_event(0x0001, 5, 3, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 40 }
  Start-Sleep -Milliseconds 800
  $open = Wheel
  Shot "ruota-$n"
  [W]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 100   # sinistro su
  [W]::keybd_event(0x10, 0, 2, [IntPtr]::Zero); Start-Sleep -Milliseconds 300        # Maiusc su
  $still = Wheel
  [W]::keybd_event(0x1B, 0, 0, [IntPtr]::Zero); [W]::keybd_event(0x1B, 0, 2, [IntPtr]::Zero); Start-Sleep -Milliseconds 800  # Esc
  L "ruota $n : aperta trascinando = $open, dopo il rilascio = $still, dopo Esc = $(Wheel)"
  return $open
}
Remove-Item $log -ErrorAction SilentlyContinue
Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 2
$proc = Start-Process $Exe -PassThru; Start-Sleep 8
$msg = [IntPtr](@([W]::All($proc.Id) | Where-Object { [W]::Cls($_) -eq 'CedroMsg' })[0])
L "cedro pid $($proc.Id) finestra messaggi $msg"
Mem "avvio"
$ok = @()
$ok += Gesture "1-prima-della-pulizia"
Start-Sleep 5; Mem "dopo ruota 1 (+5 s)"
foreach ($cmd in 7, 5) {
  [void][W]::PostMessageW($msg, 0x111, [IntPtr]$cmd, [IntPtr]0); Start-Sleep 4
  Mem "finestra $cmd aperta"
  if ($cmd -eq 7) { Shot "conversioni" }
  foreach ($h in [W]::All($proc.Id) | Where-Object { [W]::IsWindowVisible($_) -and [W]::Cls($_) -ne 'CedroMsg' }) { [void][W]::PostMessageW($h, 0x10, [IntPtr]0, [IntPtr]0) }
  Start-Sleep 6; Mem "finestra $cmd chiusa (+6 s)"
}
$ok += Gesture "2-dopo-la-pulizia"
Start-Sleep 5; Mem "dopo ruota 2 (+5 s)"
$ok += Gesture "3-di-nuovo"
Start-Sleep 5; Mem "fine"
L "ESITO ruota aperta: $($ok -join ',')"
Get-Process -Id $proc.Id -ErrorAction SilentlyContinue | Stop-Process -Force
if (@($ok | Where-Object { $_ -lt 1 }).Count -gt 0) { exit 1 }
exit 0
