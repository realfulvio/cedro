; Installer di Cedro per Windows (x64 e, con /DArm64, ARM64). Compilazione: installer\build.ps1 [-Arm64]
; Il tema «Comune di Bondeno» è dentro Cedro (Impostazioni > Aspetto). Chi aveva la vecchia edizione Bondeno si
; aggiorna con la copia «Cedro-Bondeno-Setup-<versione>.exe» di questo stesso file: se l'installatore si chiama
; così, lascia il file «da-bondeno» e Cedro, al primo avvio, accende il tema Bondeno.
; Installa per l'utente corrente: non servono i diritti di amministratore. Per tutti gli utenti solo da riga di comando
; (/ALLUSERS, per chi distribuisce Cedro in rete): mai la scelta nella procedura, che portava a due copie installate.
#define AppName "Cedro"
; Per Windows on ARM: /DArm64 (e /DBuildDir=... se l'exe non sta in target\release).
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif
#ifndef AppVersion
  #define AppVersion "0.37.0"
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
PrivilegesRequiredOverridesAllowed=commandline
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
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\cedro.exe"; IconFilename: "{app}\cedro.ico"; Tasks: desktopicon; Check: DesktopIconWanted

[InstallDelete]
; Tolta la spunta del collegamento sul desktop in una nuova installazione a mano: il collegamento vecchio va via.
Type: files; Name: "{autodesktop}\{#AppName}.lnk"; Tasks: not desktopicon; Check: NotSilent

; L'avvio con Windows (HKCU\...\Run) lo scrive il codice qui sotto: si accende e si spegne anche dal menu di Cedro,
; e gli aggiornamenti non devono riaccenderlo.

[Run]
Filename: "{app}\cedro.exe"; Parameters: "vista"; Description: "{cm:RunView}"; Flags: nowait postinstall skipifsilent; Check: ViewNotAsked
Filename: "{app}\cedro.exe"; Description: "{cm:RunStart}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\cedro.exe"; Parameters: "guida"; Description: "{cm:RunGuide}"; Flags: nowait postinstall skipifsilent

; Aggiornamento silenzioso fatto da Cedro stesso (/SILENTUPDATE): Cedro si riapre da solo, senza finestre.
Filename: "{app}\cedro.exe"; Flags: nowait runasoriginaluser; Check: SilentUpdate

[UninstallRun]
; Cedro toglie le sue voci per il doppio clic sui file firmati (.p7m, .p7s, .m7m, .tsd, .tsr, .p7e).
Filename: "{app}\cedro.exe"; Parameters: "associa-no"; Flags: runhidden waituntilterminated; RunOnceId: "CedroAssociaNo"

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\Cedro"

[Code]
const
  UNINSTALL_KEY = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{5C1CAEBF-024C-4A9F-BEC8-1FB3C9FEA4CE}_is1';
  RUN_KEY = 'Software\Microsoft\Windows\CurrentVersion\Run';

var
  // Cedro era già installato nello stesso modo (per l'utente o per tutti): è un aggiornamento.
  WasInstalled: Boolean;
  TasksFromPc: Boolean;

// Le installazioni per tutti gli utenti stanno nella vista a 64 bit del registro.
function MachineRoot: Integer;
begin
  if IsWin64 then Result := HKLM64 else Result := HKLM;
end;

function InstalledForUser: Boolean;
begin
  Result := RegKeyExists(HKCU, UNINSTALL_KEY);
end;

function InstalledForAll: Boolean;
begin
  Result := RegKeyExists(MachineRoot, UNINSTALL_KEY);
end;

function HasParam(name: String): Boolean;
var
  i: Integer;
begin
  Result := False;
  for i := 1 to ParamCount do
    if CompareText(ParamStr(i), name) = 0 then Result := True;
end;

function NotSilent: Boolean;
begin
  Result := not WizardSilent;
end;

function SilentUpdate: Boolean;
begin
  Result := HasParam('/SILENTUPDATE');
end;

// Chiede la vista (Semplice/Completa) solo la prima volta: negli aggiornamenti la scelta c'è già.
function ViewNotAsked: Boolean;
var
  text: AnsiString;
begin
  Result := True;
  if LoadStringFromFile(ExpandConstant('{userappdata}\Cedro\config.txt'), text) then
    Result := Pos('view_asked=1', text) = 0;
end;

// Negli aggiornamenti silenziosi il collegamento sul desktop si rinnova solo se c'è ancora: chi l'ha
// cancellato non se lo vede ricomparire a ogni versione.
function DesktopIconWanted: Boolean;
begin
  Result := True;
  if WizardSilent and WasInstalled then
    Result := FileExists(ExpandConstant('{autodesktop}\{#AppName}.lnk'));
end;

// Le Cedro fino alla 0.26.2, detto «sì» all'aggiornamento, lanciano questo installatore senza parametri
// dalla cartella temporanea (%TEMP%\Cedro-Setup-<v>.exe): invece della procedura guidata si rilancia da
// solo in silenzio, come fanno le Cedro nuove, e alla fine resta solo la notifica di Cedro aggiornato.
// Chi lo apre a mano (da Download, da un'altra cartella) o installa la prima volta vede la procedura di sempre.
function GetLongPathName(ShortPath: String; LongPath: String; Size: Cardinal): Cardinal;
  external 'GetLongPathNameW@kernel32.dll stdcall';

// %TEMP% spesso è scritto corto (C:\Users\ACCORS~1\...): si confrontano i nomi lunghi.
function LongDir(path: String): String;
var
  buffer: String;
  size: Cardinal;
begin
  Result := RemoveBackslashUnlessRoot(path);
  SetLength(buffer, 1024);
  size := GetLongPathName(Result, buffer, 1024);
  if (size > 0) and (size < 1024) then
    Result := RemoveBackslashUnlessRoot(Copy(buffer, 1, size));
end;

function LaunchedByOldCedro: Boolean;
var
  dir: String;
begin
  Result := False;
  if WizardSilent then Exit;
  if not (InstalledForUser or InstalledForAll) then Exit;
  // Cedro-Setup-…, Cedro-ARM64-Setup-…, Cedro-Bondeno-Setup-…: i nomi che le Cedro scaricano.
  if Pos('cedro-', LowerCase(ExtractFileName(ExpandConstant('{srcexe}')))) <> 1 then Exit;
  dir := LongDir(ExtractFileDir(ExpandConstant('{srcexe}')));
  // {tmp} è una sottocartella della cartella temporanea dell'utente: la stessa in cui scarica Cedro.
  Result := (CompareText(dir, LongDir(ExtractFileDir(ExpandConstant('{tmp}')))) = 0)
    or (CompareText(dir, LongDir(GetEnv('TMP'))) = 0) or (CompareText(dir, LongDir(GetEnv('TEMP'))) = 0);
  if Result then Log('Lanciato da una Cedro vecchia da ' + dir + ': si rilancia in silenzio');
end;

// Lanciare direttamente il file in uso dà «accesso negato»: lo fa partire cmd, nascosto.
function Relaunch(params: String): Boolean;
var
  code: Integer;
begin
  Result := Exec(ExpandConstant('{cmd}'), '/C start "" "' + ExpandConstant('{srcexe}') + '" ' + params, '', SW_HIDE, ewNoWait, code);
  if not Result then Log('Non si rilancia: ' + SysErrorMessage(code));
end;

// I parametri con cui è stato lanciato, tranne quelli interni di Inno e la scelta utente/tutti.
function PassedParams: String;
var
  i: Integer;
  p: String;
begin
  Result := '';
  for i := 1 to ParamCount do
  begin
    p := ParamStr(i);
    if (Pos('/SL5', UpperCase(p)) = 1) or (Pos('/SPAWNWND', UpperCase(p)) = 1) or (Pos('/NOTIFYWND', UpperCase(p)) = 1)
      or (CompareText(p, '/ALLUSERS') = 0) or (CompareText(p, '/CURRENTUSER') = 0) then
      continue;
    Result := Result + ' "' + p + '"';
  end;
end;

function InitializeSetup: Boolean;
begin
  Result := True;
  if LaunchedByOldCedro then
  begin
    CreateDir(ExpandConstant('{userappdata}\Cedro'));
    SaveStringToFile(ExpandConstant('{userappdata}\Cedro\updated.txt'), '{#AppVersion}', False);
    if Relaunch('/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /SILENTUPDATE') then
    begin
      Result := False;
      Exit;
    end;
    DeleteFile(ExpandConstant('{userappdata}\Cedro\updated.txt'));
  end;
  // Cedro c'è già, ma installato per tutti gli utenti (in Programmi): si aggiorna quella copia, che chiede i
  // permessi di amministratore, invece di metterne una seconda per l'utente. Chi lo vuole proprio per sé
  // lo dice con /CURRENTUSER.
  if (not IsAdminInstallMode) and (not InstalledForUser) and InstalledForAll and (not HasParam('/CURRENTUSER')) then
  begin
    Log('Cedro è installato per tutti gli utenti: si aggiorna quella copia');
    Result := False;
    if not Relaunch(PassedParams + ' /ALLUSERS') then
      SuppressibleMsgBox('Cedro è installato per tutti gli utenti: per aggiornarlo servono i permessi di amministratore.',
        mbError, MB_OK, IDOK);
    Exit;
  end;
  if IsAdminInstallMode then WasInstalled := InstalledForAll else WasInstalled := InstalledForUser;
end;

procedure CloseCedro;
var
  code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM cedro.exe', '', SW_HIDE, ewWaitUntilTerminated, code);
end;

// Nella procedura di una nuova installazione le spunte partono da com'è il PC adesso (l'avvio con Windows si
// cambia anche dal menu di Cedro, il collegamento si può cancellare a mano), non da com'erano la prima volta.
procedure CurPageChanged(CurPageID: Integer);
var
  value: String;
begin
  if (CurPageID <> wpSelectTasks) or TasksFromPc or not WasInstalled then Exit;
  TasksFromPc := True;
  if RegQueryStringValue(HKCU, RUN_KEY, 'Cedro', value) then WizardSelectTasks('autostart')
  else WizardSelectTasks('!autostart');
  if FileExists(ExpandConstant('{autodesktop}\{#AppName}.lnk')) then WizardSelectTasks('desktopicon')
  else WizardSelectTasks('!desktopicon');
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

// Installando per tutti gli utenti, la copia che l'utente aveva solo per sé va via: una sola Cedro.
// Si toglie a mano (cartella, collegamenti, voce in Programmi) perché il suo disinstallatore cancellerebbe
// anche le impostazioni dell'utente.
procedure RemoveUserCopy;
var
  dir: String;
begin
  if not (IsAdminInstallMode and InstalledForUser) then Exit;
  if RegQueryStringValue(HKCU, UNINSTALL_KEY, 'InstallLocation', dir) then
  begin
    dir := RemoveBackslashUnlessRoot(dir);
    if (CompareText(ExtractFileName(dir), 'Cedro') = 0) and (CompareText(dir, ExpandConstant('{app}')) <> 0) then
    begin
      Log('Tolgo la copia per l''utente in ' + dir);
      DelTree(dir, True, True, True);
    end;
  end;
  DeleteFile(ExpandConstant('{userprograms}\{#AppName}.lnk'));
  if FileExists(ExpandConstant('{userdesktop}\{#AppName}.lnk')) then
  begin
    DeleteFile(ExpandConstant('{userdesktop}\{#AppName}.lnk'));
    // Il collegamento c'era: resta, per tutti.
    if not WizardIsTaskSelected('desktopicon') then
      CreateShellLink(ExpandConstant('{commondesktop}\{#AppName}.lnk'), '', ExpandConstant('{app}\cedro.exe'), '', '',
        ExpandConstant('{app}\cedro.ico'), 0, SW_SHOWNORMAL);
  end;
  RegDeleteKeyIncludingSubkeys(HKCU, UNINSTALL_KEY);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  CloseCedro;
  LeaveBondenoNote;
  Result := '';
end;

function QuotedExe: String;
begin
  Result := '"' + ExpandConstant('{app}\cedro.exe') + '"';
end;

// Avvio con Windows. Prima installazione o procedura a mano: come la spunta. Aggiornamento silenzioso: com'era
// (acceso o spento dal menu di Cedro), solo con il percorso giusto se quello scritto non esiste più.
procedure ApplyAutostart;
var
  value: String;
begin
  if WizardSilent and WasInstalled then
  begin
    if RegQueryStringValue(HKCU, RUN_KEY, 'Cedro', value) and not FileExists(RemoveQuotes(value)) then
      RegWriteStringValue(HKCU, RUN_KEY, 'Cedro', QuotedExe);
    Exit;
  end;
  if WizardIsTaskSelected('autostart') then
    RegWriteStringValue(HKCU, RUN_KEY, 'Cedro', QuotedExe)
  else if WasInstalled then
    RegDeleteValue(HKCU, RUN_KEY, 'Cedro');
end;

// Installando per l'utente con una Cedro vecchia anche per tutti gli utenti (le due copie che faceva la scelta
// nella procedura): nella procedura a mano si propone di togliere quella in Programmi.
procedure OfferRemoveAllUsersCopy;
var
  uninstaller, config, backup: String;
  code: Integer;
begin
  if WizardSilent or IsAdminInstallMode or not InstalledForAll then Exit;
  if not RegQueryStringValue(MachineRoot, UNINSTALL_KEY, 'UninstallString', uninstaller) then Exit;
  uninstaller := RemoveQuotes(uninstaller);
  if not FileExists(uninstaller) then Exit;
  if MsgBox('C''è anche una seconda copia di Cedro, installata per tutti gli utenti in ' + ExtractFileDir(uninstaller) + '.'
    + #13#10#13#10 + 'Vuoi toglierla, così ne resta una sola? Servono i permessi di amministratore.',
    mbConfirmation, MB_YESNO) <> IDYES then Exit;
  // Il disinstallatore vecchio cancella le impostazioni (%APPDATA%\Cedro): si tengono da parte.
  config := ExpandConstant('{userappdata}\Cedro\config.txt');
  backup := ExpandConstant('{tmp}\config.txt');
  if FileExists(config) then FileCopy(config, backup, False);
  if not ShellExec('runas', uninstaller, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_SHOW, ewWaitUntilTerminated, code) then
    Log('Copia per tutti gli utenti non tolta: ' + SysErrorMessage(code));
  if FileExists(backup) and not FileExists(config) then
  begin
    ForceDirectories(ExtractFileDir(config));
    FileCopy(backup, config, False);
  end;
end;

// La lingua scelta nell'installatore diventa quella di Cedro, se non ne ha già una nelle Impostazioni.
procedure SaveLanguage;
var
  path: String;
  text: AnsiString;
  code: String;
begin
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

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then RemoveUserCopy;
  if CurStep <> ssPostInstall then Exit;
  // Prima la copia doppia (il suo disinstallatore toglie anche l'avvio con Windows), poi l'avvio.
  OfferRemoveAllUsersCopy;
  ApplyAutostart;
  SaveLanguage;
end;

function InitializeUninstall: Boolean;
begin
  CloseCedro;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  value, data: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    // L'avvio con Windows va via solo se porta a questa copia di Cedro.
    if RegQueryStringValue(HKCU, RUN_KEY, 'Cedro', value)
      and (CompareText(RemoveQuotes(value), ExpandConstant('{app}\cedro.exe')) = 0) then
      RegDeleteValue(HKCU, RUN_KEY, 'Cedro');
  end;
  if CurUninstallStep = usPostUninstall then
  begin
    // Quello che resta nella cartella di Cedro (file lasciati da Cedro stesso): via anche quello.
    if CompareText(ExtractFileName(RemoveBackslashUnlessRoot(ExpandConstant('{app}'))), 'Cedro') = 0 then
      DelTree(ExpandConstant('{app}'), True, True, True);
    // Modelli e motori scaricati (anche alcuni GB) e trascrizioni da rivedere: si chiede, nel silenzioso restano.
    data := ExpandConstant('{localappdata}\Cedro');
    if (not UninstallSilent) and DirExists(data) then
      if MsgBox('Vuoi cancellare anche i modelli scaricati per la trascrizione e il Riassunto AI e le trascrizioni salvate per la revisione?'
        + #13#10#13#10 + data + #13#10#13#10 + 'Se reinstalli Cedro, i modelli si riscaricano da soli al primo uso.',
        mbConfirmation, MB_YESNO) = IDYES then
        DelTree(data, True, True, True);
  end;
end;
