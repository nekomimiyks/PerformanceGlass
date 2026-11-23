#ifndef AppVersion
  #error Specify AppVersion with /DAppVersion
#endif
#ifndef PackageDir
  #error Specify PackageDir with /DPackageDir
#endif

[Setup]
AppId={{64907C07-E982-497B-9F16-4E6C06A68F08}
AppName=Performance Glass
AppVersion={#AppVersion}
AppPublisher=nekomimiyks
AppPublisherURL=https://github.com/nekomimiyks/PerformanceGlass
AppSupportURL=https://github.com/nekomimiyks/PerformanceGlass/issues
DefaultDirName={localappdata}\Programs\PerformanceGlass
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
OutputBaseFilename=PerformanceGlass-v{#AppVersion}-Windows-x64-Setup
SetupIconFile=..\assets\icon\performance-glass.ico
UninstallDisplayIcon={app}\PerformanceGlass.exe
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
CloseApplicationsFilter=PerformanceGlass.exe,PerformanceGlass.CpuSensor.exe
RestartApplications=no
WizardStyle=modern

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "zh_CN"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"
Name: "zh_TW"; MessagesFile: "compiler:Languages\ChineseTraditional.isl"
Name: "ko"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
en.LaunchNow=Start Performance Glass now?
ja.LaunchNow=今すぐアプリを起動しますか？
zh_CN.LaunchNow=立即启动 Performance Glass 吗？
zh_TW.LaunchNow=立即啟動 Performance Glass 嗎？
ko.LaunchNow=지금 Performance Glass를 시작하시겠습니까?
de.LaunchNow=Performance Glass jetzt starten?
fr.LaunchNow=Démarrer Performance Glass maintenant ?
es.LaunchNow=¿Quieres iniciar Performance Glass ahora?
pt.LaunchNow=Deseja iniciar o Performance Glass agora?
it.LaunchNow=Avviare Performance Glass adesso?
ru.LaunchNow=Запустить Performance Glass сейчас?

[Files]
Source: "{#PackageDir}\PerformanceGlass.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\cpu-sensor\*"; DestDir: "{app}\cpu-sensor"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Performance Glass"; Filename: "{app}\PerformanceGlass.exe"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'PerformanceGlass';

function InstalledCommand: String;
begin
  Result := '"' + ExpandConstant('{app}\PerformanceGlass.exe') + '" --tray';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ExistingCommand: String;
  ResultCode: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    if RegQueryStringValue(HKCU, RunKey, RunValue, ExistingCommand) then
      RegWriteStringValue(HKCU, RunKey, RunValue, InstalledCommand);
  end;
  if (CurStep = ssDone) and not WizardSilent then
  begin
    if MsgBox(CustomMessage('LaunchNow'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    begin
      if not Exec(ExpandConstant('{app}\PerformanceGlass.exe'), '', ExpandConstant('{app}'),
        SW_SHOWNORMAL, ewNoWait, ResultCode) then
        MsgBox(SysErrorMessage(ResultCode), mbError, MB_OK);
    end;
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ExistingCommand: String;
begin
  if CurUninstallStep = usPostUninstall then
    if RegQueryStringValue(HKCU, RunKey, RunValue, ExistingCommand) then
      if CompareText(ExistingCommand, InstalledCommand) = 0 then
        RegDeleteValue(HKCU, RunKey, RunValue);
end;
