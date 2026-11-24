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
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
CloseApplications=yes
RestartApplications=no
SetupLogging=yes
VersionInfoVersion={#VersionNum}
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

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

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
