; Installer di Cedro per Windows (x64 e, con /DArm64, ARM64). Compilazione: installer\build.ps1 [-Arm64]
; Il tema «Comune di Bondeno» è dentro Cedro (Impostazioni > Aspetto).
; Installa per l'utente corrente: non servono i diritti di amministratore.
#define AppName "Cedro"
; Per Windows on ARM: /DArm64 (e /DBuildDir=... se l'exe non sta in target\release).
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif
#ifndef AppVersion
  #define AppVersion "0.21.3"
#endif

[Setup]
AppId={{5C1CAEBF-024C-4A9F-BEC8-1FB3C9FEA4CE}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=realfulvio
AppPublisherURL=https://github.com/realfulvio/cedro
AppSupportURL=https://github.com/realfulvio/cedro/issues
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
; I nomi devono restare quelli che Cedro cerca nella release per aggiornarsi (src/update.rs).
#ifdef Arm64
  #define Prefix "Cedro-ARM64-Setup-"
#else
  #define Prefix "Cedro-Setup-"
#endif
#define IconFile "..\assets\cedro.ico"
OutputBaseFilename={#Prefix}{#AppVersion}
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\cedro.exe
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
#ifdef Arm64
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
LicenseFile=..\LICENSE
ShowLanguageDialog=no

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un collegamento sul desktop"; GroupDescription: "Collegamenti:"
Name: "autostart"; Description: "Avvia Cedro all'accesso a Windows"; GroupDescription: "Avvio:"

[Files]
Source: "{#BuildDir}\cedro.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\Licenze di terze parti.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#IconFile}"; DestDir: "{app}"; DestName: "cedro.ico"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\cedro.exe"; IconFilename: "{app}\cedro.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\cedro.exe"; IconFilename: "{app}\cedro.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Cedro"; ValueData: """{app}\cedro.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\cedro.exe"; Parameters: "vista"; Description: "Scegli come usare Cedro (Semplice o Completa)"; Flags: nowait postinstall skipifsilent; Check: ViewNotAsked
Filename: "{app}\cedro.exe"; Description: "Avvia Cedro"; Flags: nowait postinstall skipifsilent
Filename: "{app}\cedro.exe"; Parameters: "guida"; Description: "Guarda come funziona (guida animata)"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\Cedro"

[Code]
// Chiede la vista (Semplice/Completa) solo la prima volta: negli aggiornamenti la scelta c'è già.
function ViewNotAsked: Boolean;
var
  text: AnsiString;
begin
  Result := True;
  if LoadStringFromFile(ExpandConstant('{userappdata}\Cedro\config.txt'), text) then
    Result := Pos('view_asked=1', text) = 0;
end;

procedure CloseCedro;
var
  code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM cedro.exe', '', SW_HIDE, ewWaitUntilTerminated, code);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseCedro;
  Result := '';
end;

function InitializeUninstall: Boolean;
begin
  CloseCedro;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  { La voce si può accendere anche dal menu di Cedro, non solo dall'installatore. }
  if CurUninstallStep = usUninstall then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Cedro');
end;
