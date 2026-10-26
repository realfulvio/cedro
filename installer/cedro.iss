; Installer di Cedro per Windows. Compilazione: installer\build.ps1
; Installa per l'utente corrente: non servono i diritti di amministratore.
#define AppName "Cedro"
#ifndef AppVersion
  #define AppVersion "0.13.1"
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
OutputBaseFilename=Cedro-Setup-{#AppVersion}
SetupIconFile=..\assets\cedro.ico
UninstallDisplayIcon={app}\cedro.exe
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\LICENSE
ShowLanguageDialog=no

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un collegamento sul desktop"; GroupDescription: "Collegamenti:"
Name: "autostart"; Description: "Avvia Cedro all'accesso a Windows"; GroupDescription: "Avvio:"

[Files]
Source: "..\target\release\cedro.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\cedro.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\cedro.exe"; IconFilename: "{app}\cedro.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\cedro.exe"; IconFilename: "{app}\cedro.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Cedro"; ValueData: """{app}\cedro.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\cedro.exe"; Description: "Avvia Cedro"; Flags: nowait postinstall skipifsilent
Filename: "{app}\cedro.exe"; Parameters: "guida"; Description: "Guarda come funziona (guida animata)"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\Cedro"

[Code]
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
