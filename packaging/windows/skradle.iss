; SKradle's Windows installer (Inno Setup 6.7). Built by packaging/windows.sh, which passes everything through
; environment variables (SK_VERSION, SK_VERSION_NUM, SK_PLATFORM, SK_ARCH, SK_SOURCE, SK_OUT, SK_LICENSE).
;
; Per user: no administrator rights, installs into %LOCALAPPDATA%\Programs\SKradle. Light or dark like Windows.
; The launcher's own update runs it as `/SILENT /SUPPRESSMSGBOXES /NORESTART /UPDATE`: it closes the old launcher,
; installs over it and starts the new one. Not code-signed, so SmartScreen may ask once ("More info" > "Run anyway").

#define Version GetEnv("SK_VERSION")
#define VersionNum GetEnv("SK_VERSION_NUM")
#define Platform GetEnv("SK_PLATFORM")
#define Source GetEnv("SK_SOURCE")
#define OutDir GetEnv("SK_OUT")
#define License GetEnv("SK_LICENSE")
#if Version == "" || VersionNum == "" || Platform == "" || Source == "" || OutDir == "" || License == ""
  #error Set SK_VERSION, SK_VERSION_NUM, SK_PLATFORM, SK_SOURCE, SK_OUT and SK_LICENSE (see packaging/windows.sh)
#endif
#define Repo "https://github.com/SKonstruct/SKradle"

[Setup]
AppId={{A70AFEA6-127A-4CF7-981B-78A0178D2B74}
AppName=SKradle
AppVersion={#Version}
AppVerName=SKradle {#Version}
AppPublisher=SKonstruct
AppPublisherURL={#Repo}
AppSupportURL={#Repo}/issues
AppUpdatesURL={#Repo}/releases
DefaultDirName={autopf}\SKradle
DisableProgramGroupPage=yes
SetupMutex=SKradleSetupMutex,Global\SKradleSetupMutex
PrivilegesRequired=lowest
MinVersion=10.0
#if GetEnv("SK_ARCH") == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
LicenseFile={#License}
SetupIconFile={#Source}\skradle.ico
UninstallDisplayIcon={app}\skradle.ico
UninstallDisplayName=SKradle
OutputDir={#OutDir}
OutputBaseFilename=SKradle-{#Version}-{#Platform}-installer
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern dynamic
; The welcome page is hidden unless asked for, and it is where the wizard image first shows.
DisableWelcomePage=no
; The community artwork (credited in the image) on the welcome and finish pages and the logo on the others, as PNGs
; (Inno Setup 6.5+) in the sizes of the image areas at 100% to 250% display scaling: Setup picks the one that fits, so
; none is stretched. Drawn by render-wizard-images.py; paths are relative to this file.
WizardImageFile=wizard-202.png,wizard-269.png,wizard-336.png,wizard-430.png,wizard-534.png
WizardSmallImageFile=wizard-small-58.png,wizard-small-77.png,wizard-small-97.png,wizard-small-124.png,wizard-small-159.png
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
CloseApplications=yes
RestartApplications=no
SetupLogging=yes
VersionInfoVersion={#VersionNum}
VersionInfoCompany=SKonstruct
VersionInfoCopyright=MIT license
VersionInfoProductName=SKradle
VersionInfoDescription=SKradle installer

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "italian"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "polish"; MessagesFile: "compiler:Languages\Polish.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
english.GitHubButton=&GitHub
english.OpenGitHub=Open the SKradle page on GitHub
english.Downgrade=A newer version of SKradle (%1) is already installed. Install version %2 over it anyway?
english.Reinstall=SKradle %1 is already installed. Install it again?
english.DeleteData=Also delete SKradle's settings, history, logs and downloaded tools?%n%nYour mods and your game files are not touched.
german.GitHubButton=&GitHub
german.OpenGitHub=Die SKradle-Seite auf GitHub öffnen
german.Downgrade=Eine neuere Version von SKradle (%1) ist bereits installiert. Version %2 trotzdem darüber installieren?
german.Reinstall=SKradle %1 ist bereits installiert. Erneut installieren?
german.DeleteData=Auch die Einstellungen, den Verlauf, die Protokolle und die heruntergeladenen Werkzeuge von SKradle löschen?%n%nIhre Mods und Ihre Spieldateien bleiben unberührt.
spanish.GitHubButton=&GitHub
spanish.OpenGitHub=Abrir la página de SKradle en GitHub
spanish.Downgrade=Ya hay instalada una versión más reciente de SKradle (%1). ¿Instalar igualmente la versión %2 encima?
spanish.Reinstall=SKradle %1 ya está instalado. ¿Instalarlo de nuevo?
spanish.DeleteData=¿Eliminar también la configuración, el historial, los registros y las herramientas descargadas de SKradle?%n%nTus mods y los archivos del juego no se tocan.
french.GitHubButton=&GitHub
french.OpenGitHub=Ouvrir la page de SKradle sur GitHub
french.Downgrade=Une version plus récente de SKradle (%1) est déjà installée. Installer quand même la version %2 par-dessus ?
french.Reinstall=SKradle %1 est déjà installé. Le réinstaller ?
french.DeleteData=Supprimer aussi les paramètres, l'historique, les journaux et les outils téléchargés de SKradle ?%n%nVos mods et les fichiers du jeu ne sont pas touchés.
italian.GitHubButton=&GitHub
italian.OpenGitHub=Apri la pagina di SKradle su GitHub
italian.Downgrade=È già installata una versione più recente di SKradle (%1). Installare comunque la versione %2 sopra di essa?
italian.Reinstall=SKradle %1 è già installato. Installarlo di nuovo?
italian.DeleteData=Eliminare anche le impostazioni, la cronologia, i log e gli strumenti scaricati di SKradle?%n%nI tuoi mod e i file del gioco non vengono toccati.
japanese.GitHubButton=&GitHub
japanese.OpenGitHub=GitHub の SKradle ページを開く
japanese.Downgrade=より新しいバージョンの SKradle (%1) が既にインストールされています。それでもバージョン %2 を上書きインストールしますか？
japanese.Reinstall=SKradle %1 は既にインストールされています。もう一度インストールしますか？
japanese.DeleteData=SKradle の設定、履歴、ログ、ダウンロードしたツールも削除しますか？%n%nMod とゲームファイルには影響しません。
polish.GitHubButton=&GitHub
polish.OpenGitHub=Otwórz stronę SKradle na GitHubie
polish.Downgrade=Nowsza wersja SKradle (%1) jest już zainstalowana. Zainstalować mimo to wersję %2 na jej miejscu?
polish.Reinstall=SKradle %1 jest już zainstalowany. Zainstalować ponownie?
polish.DeleteData=Usunąć także ustawienia, historię, dzienniki i pobrane narzędzia SKradle?%n%nTwoje mody i pliki gry nie zostaną naruszone.
brazilianportuguese.GitHubButton=&GitHub
brazilianportuguese.OpenGitHub=Abrir a página do SKradle no GitHub
brazilianportuguese.Downgrade=Uma versão mais recente do SKradle (%1) já está instalada. Instalar mesmo assim a versão %2 por cima?
brazilianportuguese.Reinstall=O SKradle %1 já está instalado. Instalá-lo novamente?
brazilianportuguese.DeleteData=Excluir também as configurações, o histórico, os registros e as ferramentas baixadas do SKradle?%n%nSeus mods e os arquivos do jogo não são tocados.
russian.GitHubButton=&GitHub
russian.OpenGitHub=Открыть страницу SKradle на GitHub
russian.Downgrade=Более новая версия SKradle (%1) уже установлена. Всё равно установить версию %2 поверх неё?
russian.Reinstall=SKradle %1 уже установлен. Установить его заново?
russian.DeleteData=Удалить также настройки, историю, журналы и загруженные инструменты SKradle?%n%nВаши моды и файлы игры не затрагиваются.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
; Qt's files change between versions, and an old DLL or plugin folder left next to a newer exe can stop it from
; starting. Updating over an install first clears what the installer itself lays down: the DLLs and the folders the Qt
; deployment tool creates, never the whole install folder (it could be one the user chose with other things in it).
Type: files; Name: "{app}\*.dll"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\qml"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\generic"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\iconengines"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\imageformats"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\networkinformation"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\platforms"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\qmltooling"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\styles"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\tls"; Check: IsSKradleFolder
Type: filesandordirs; Name: "{app}\webview"; Check: IsSKradleFolder

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\SKradle"; Filename: "{app}\skradle.exe"; IconFilename: "{app}\skradle.ico"
Name: "{autodesktop}\SKradle"; Filename: "{app}\skradle.exe"; IconFilename: "{app}\skradle.ico"; Tasks: desktopicon

[Run]
; The launcher's own update has no wizard to click through: it starts SKradle again by itself.
Filename: "{app}\skradle.exe"; Flags: nowait; Check: IsUpdateRun
Filename: "{app}\skradle.exe"; Description: "{cm:LaunchProgram,SKradle}"; Flags: nowait postinstall skipifsilent
Filename: "{#Repo}"; Description: "{cm:OpenGitHub}"; Flags: shellexec postinstall skipifsilent unchecked

[Code]
const
  RepoUrl = '{#Repo}';
  UninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{A70AFEA6-127A-4CF7-981B-78A0178D2B74}_is1';

// True where SKradle is already installed (its exe is there), so the cleanup above only ever touches our own folder.
function IsSKradleFolder: Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\skradle.exe'));
end;

function IsUpdateRun: Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/UPDATE') = 0 then
      Result := True;
end;

// "1.2.3" and "1.2.3-abc1234" both become one comparable number (1.2.3).
function VersionValue(const Text: String): Int64;
var
  Rest, Part: String;
  P, I: Integer;
begin
  Result := 0;
  Rest := Text;
  P := Pos('-', Rest);
  if P > 0 then
    Rest := Copy(Rest, 1, P - 1);
  for I := 1 to 3 do
  begin
    P := Pos('.', Rest);
    if P > 0 then
    begin
      Part := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end
    else
    begin
      Part := Rest;
      Rest := '';
    end;
    Result := Result * 100000 + StrToIntDef(Part, 0);
  end;
end;

function InstalledVersion: String;
var
  Found: String;
begin
  Found := '';
  if not RegQueryStringValue(HKCU, UninstallKey, 'DisplayVersion', Found) then
    RegQueryStringValue(HKLM, UninstallKey, 'DisplayVersion', Found);
  Result := Found;
end;

// Asks before going back to an older version or installing the same one again; an update or a silent run never asks.
function InitializeSetup: Boolean;
var
  Installed: String;
begin
  Result := True;
  Installed := InstalledVersion;
  if (Installed = '') or WizardSilent then
    Exit;
  if VersionValue(Installed) > VersionValue('{#Version}') then
    Result := MsgBox(FmtMessage(CustomMessage('Downgrade'), [Installed, '{#Version}']), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES
  else if VersionValue(Installed) = VersionValue('{#Version}') then
    Result := MsgBox(FmtMessage(CustomMessage('Reinstall'), [Installed]), mbConfirmation, MB_YESNO) = IDYES;
end;

procedure GitHubClick(Sender: TObject);
var
  ErrorCode: Integer;
begin
  ShellExec('open', RepoUrl, '', '', SW_SHOWNORMAL, ewNoWait, ErrorCode);
end;

// A button next to the others, on every page, that opens the project page.
procedure InitializeWizard;
var
  Button: TNewButton;
begin
  Button := TNewButton.Create(WizardForm);
  Button.Parent := WizardForm;
  Button.Left := WizardForm.ClientWidth - (WizardForm.CancelButton.Left + WizardForm.CancelButton.Width);
  Button.Top := WizardForm.CancelButton.Top;
  Button.Width := WizardForm.CancelButton.Width;
  Button.Height := WizardForm.CancelButton.Height;
  Button.Caption := CustomMessage('GitHubButton');
  Button.OnClick := @GitHubClick;
end;

// Everything SKradle keeps of its own (settings, history, logs, downloaded tools), except the mods folder.
procedure DeleteSettingsAndData;
var
  Root, Path: String;
  Found: TFindRec;
begin
  Root := ExpandConstant('{userappdata}\SKradle');
  if FindFirst(Root + '\*', Found) then
  begin
    try
      repeat
        if (Found.Name <> '.') and (Found.Name <> '..') and (CompareText(Found.Name, 'mods') <> 0) then
        begin
          Path := Root + '\' + Found.Name;
          if (Found.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
            DelTree(Path, True, True, True)
          else
            DeleteFile(Path);
        end;
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
  DelTree(ExpandConstant('{localappdata}\SKradle'), True, True, True);
  RemoveDir(Root);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent then
    if MsgBox(CustomMessage('DeleteData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
      DeleteSettingsAndData;
end;
