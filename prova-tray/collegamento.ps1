param([string]$Exe = "$env:LOCALAPPDATA\Programs\Cedro\cedro.exe", [string]$Out = "$PWD")
# Il collegamento sul desktop (e nel menu Start): con doppio clic mostra la finestra di Cedro, sia che parta da zero
# sia che Cedro sia già nella tray (istanza unica). Senza collegamento (avvio con Windows) resta solo l'icona.
# Serve un desktop vero. Esce con 1 se una verifica non riesce.
$ErrorActionPreference = 'Continue'
$log = Join-Path $Out "collegamento.log"
function L($t) { "$(Get-Date -Format HH:mm:ss) $t" | Tee-Object -FilePath $log -Append }
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices; using System.Collections.Generic;
public static class W2 {
  public delegate bool EP(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EP f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
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
# Le finestre visibili di Cedro (tutti i processi cedro.exe), tranne quella dei messaggi della tray.
function Visible() {
  $r = @()
  foreach ($p in Get-Process cedro -ErrorAction SilentlyContinue) {
    foreach ($h in [W2]::All($p.Id)) { if ([W2]::IsWindowVisible($h) -and [W2]::Cls($h) -ne 'CedroMsg') { $r += "$($p.Id):$([W2]::Cls($h))" } }
  }
  return $r
}
function CloseAll() {
  foreach ($p in Get-Process cedro -ErrorAction SilentlyContinue) {
    foreach ($h in [W2]::All($p.Id)) { if ([W2]::IsWindowVisible($h) -and [W2]::Cls($h) -ne 'CedroMsg') { [void][W2]::PostMessageW($h, 0x10, [IntPtr]0, [IntPtr]0) } }
  }
  Start-Sleep 3
}
$fail = 0
function Check($ok, $t) { if ($ok) { L "OK   $t" } else { L "FALLITO $t"; $script:fail++ } }
Remove-Item $log -ErrorAction SilentlyContinue
Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 2

$sh = New-Object -ComObject WScript.Shell
$lnks = @(
  (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Cedro.lnk'),
  (Join-Path ([Environment]::GetFolderPath('Programs')) 'Cedro.lnk')
)
foreach ($l in $lnks) {
  Check (Test-Path $l) "il collegamento esiste: $l"
  if (Test-Path $l) {
    $s = $sh.CreateShortcut($l)
    L "  destinazione $($s.TargetPath) argomenti '$($s.Arguments)'"
    Check ($s.TargetPath -eq $Exe -and $s.Arguments -eq 'apri') "il collegamento lancia cedro.exe apri: $l"
  }
}
$desk = $lnks[0]

# 1. Avvio con Windows (nessun argomento): solo l'icona, nessuna finestra.
$tray = Start-Process $Exe -PassThru; Start-Sleep 8
Check ((Visible).Count -eq 0) "senza argomenti Cedro resta nella tray, nessuna finestra (finestre: $((Visible) -join ','))"

# 2. Doppio clic sul collegamento con Cedro già nella tray: la finestra si apre, e non parte un secondo Cedro.
Start-Process $desk; Start-Sleep 8
$vis = Visible
Shot "collegamento-con-tray"
Check ($vis.Count -ge 1) "doppio clic con Cedro già nella tray: finestra visibile ($($vis -join ','))"
Check (@($vis | Where-Object { $_ -like "$($tray.Id):*" }).Count -ge 1) "la finestra è del Cedro già in esecuzione (pid $($tray.Id))"
Check (@(Get-Process cedro -ErrorAction SilentlyContinue).Count -eq 1) "un solo cedro.exe in esecuzione ($(@(Get-Process cedro -ErrorAction SilentlyContinue).Count))"

# 3. Chiusa la finestra Cedro resta nella tray; un secondo doppio clic la riapre.
CloseAll
Check ((Visible).Count -eq 0) "finestra chiusa"
Check ($null -ne (Get-Process -Id $tray.Id -ErrorAction SilentlyContinue)) "Cedro resta nella tray dopo la chiusura della finestra"
Start-Process $desk; Start-Sleep 6
Check ((Visible).Count -ge 1) "secondo doppio clic: la finestra si riapre"
# E con la finestra già aperta non se ne apre una seconda.
$prima = (Visible).Count
Start-Process $desk; Start-Sleep 5
Check ((Visible).Count -eq $prima) "con la finestra già aperta non se ne apre un'altra ($prima -> $((Visible).Count))"

# 4. Cedro non è in esecuzione: il doppio clic lo avvia e mostra la finestra.
Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep 3
Start-Process $desk; Start-Sleep 10
Check ((Visible).Count -ge 1) "doppio clic con Cedro chiuso: parte e mostra la finestra ($((Visible) -join ','))"
Check (@(Get-Process cedro -ErrorAction SilentlyContinue).Count -eq 1) "un solo cedro.exe dopo l'avvio dal collegamento"
Shot "collegamento-da-chiuso"
Get-Process cedro -ErrorAction SilentlyContinue | Stop-Process -Force
L "ESITO: $fail verifiche fallite"
if ($fail -gt 0) { exit 1 }
exit 0
