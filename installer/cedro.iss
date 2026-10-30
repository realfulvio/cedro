; Installer di Cedro per Windows (x64 e, con /DArm64, ARM64). Compilazione: installer\build.ps1 [-Arm64]
; Il tema «Comune di Bondeno» è dentro Cedro (Impostazioni > Aspetto). Chi aveva la vecchia edizione Bondeno si
; aggiorna con la copia «Cedro-Bondeno-Setup-<versione>.exe» di questo stesso file: se l'installatore si chiama
; così, lascia il file «da-bondeno» e Cedro, al primo avvio, accende il tema Bondeno.
; Installa per l'utente corrente: non servono i diritti di amministratore.
#define AppName "Cedro"
; Per Windows on ARM: /DArm64 (e /DBuildDir=... se l'exe non sta in target\release).
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif
#ifndef AppVersion
  #define AppVersion "0.29.0"
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
; Si sceglie la lingua (italiano o inglese); la stessa va in Cedro (vedi CurStepChanged).
ShowLanguageDialog=yes
UsePreviousLanguage=no

[Languages]
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
italian.TaskDesktop=Crea un collegamento sul desktop
italian.GroupLinks=Collegamenti:
italian.TaskAutostart=Avvia Cedro all'accesso a Windows
italian.GroupStartup=Avvio:
italian.RunView=Scegli come usare Cedro (Semplice o Completa)
italian.RunStart=Avvia Cedro
italian.RunGuide=Guarda come funziona (guida animata)
english.TaskDesktop=Create a desktop shortcut
english.GroupLinks=Shortcuts:
english.TaskAutostart=Start Cedro when Windows starts
english.GroupStartup=Startup:
english.RunView=Choose how to use Cedro (Simple or Full)
english.RunStart=Start Cedro
english.RunGuide=See how it works (animated guide)

[Tasks]
Name: "desktopicon"; Description: "{cm:TaskDesktop}"; GroupDescription: "{cm:GroupLinks}"
Name: "autostart"; Description: "{cm:TaskAutostart}"; GroupDescription: "{cm:GroupStartup}"

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
Filename: "{app}\cedro.exe"; Parameters: "vista"; Description: "{cm:RunView}"; Flags: nowait postinstall skipifsilent; Check: ViewNotAsked
Filename: "{app}\cedro.exe"; Description: "{cm:RunStart}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\cedro.exe"; Parameters: "guida"; Description: "{cm:RunGuide}"; Flags: nowait postinstall skipifsilent

; Aggiornamento silenzioso fatto da Cedro stesso (/SILENTUPDATE): Cedro si riapre da solo, senza finestre.
Filename: "{app}\cedro.exe"; Flags: nowait; Check: SilentUpdate

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

function SilentUpdate: Boolean;
var
  i: Integer;
begin
  Result := False;
  for i := 1 to ParamCount do
    if CompareText(ParamStr(i), '/SILENTUPDATE') = 0 then Result := True;
end;

procedure CloseCedro;
var
  code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM cedro.exe', '', SW_HIDE, ewWaitUntilTerminated, code);
end;

// Chi ha la vecchia edizione Comune di Bondeno si aggiorna con la copia «Cedro-Bondeno-…»: in quel caso
// il tema Bondeno resta acceso (Cedro lo sceglie da solo al primo avvio, trovando questo file).
procedure LeaveBondenoNote;
begin
  if Pos('cedro-bondeno', LowerCase(ExtractFileName(ExpandConstant('{srcexe}')))) = 1 then
  begin
    ForceDirectories(ExpandConstant('{userappdata}\Cedro'));
    SaveStringToFile(ExpandConstant('{userappdata}\Cedro\da-bondeno'), '', False);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseCedro;
  LeaveBondenoNote;
  Result := '';
end;

// La lingua scelta nell'installatore diventa quella di Cedro, se non ne ha già una nelle Impostazioni.
procedure CurStepChanged(CurStep: TSetupStep);
var
  path: String;
  text: AnsiString;
  code: String;
begin
  if CurStep <> ssPostInstall then Exit;
  if ActiveLanguage = 'english' then code := 'en' else code := 'it';
  path := ExpandConstant('{userappdata}\Cedro\config.txt');
  text := '';
  LoadStringFromFile(path, text);
  if Pos('language=', text) = 0 then
  begin
    ForceDirectories(ExpandConstant('{userappdata}\Cedro'));
    if (Length(text) > 0) and (text[Length(text)] <> #10) then text := text + #13#10;
    SaveStringToFile(path, 'language=' + code + #13#10, True);
  end;
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
