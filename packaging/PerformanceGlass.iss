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
en.CloseForUpdateFailed=Performance Glass is still running. Exit it from its menu and run Setup again.
ja.CloseForUpdateFailed=Performance Glass を終了できませんでした。アプリのメニューから終了して、もう一度セットアップしてください。
zh_CN.CloseForUpdateFailed=Performance Glass 仍在运行。请通过应用菜单退出后重新安装。
zh_TW.CloseForUpdateFailed=Performance Glass 仍在執行。請從應用程式選單結束後重新安裝。
ko.CloseForUpdateFailed=Performance Glass가 실행 중입니다. 앱 메뉴에서 종료한 뒤 설치를 다시 실행하세요.
de.CloseForUpdateFailed=Performance Glass läuft noch. Beenden Sie die App über ihr Menü und starten Sie das Setup erneut.
fr.CloseForUpdateFailed=Performance Glass est encore en cours d’exécution. Quittez l’application depuis son menu, puis relancez l’installation.
es.CloseForUpdateFailed=Performance Glass sigue en ejecución. Sal de la aplicación desde su menú y vuelve a ejecutar el instalador.
pt.CloseForUpdateFailed=O Performance Glass ainda está em execução. Feche o aplicativo pelo menu e execute o instalador novamente.
it.CloseForUpdateFailed=Performance Glass è ancora in esecuzione. Chiudi l’app dal suo menu e avvia nuovamente il programma di installazione.
ru.CloseForUpdateFailed=Performance Glass всё ещё запущен. Выйдите из приложения через его меню и снова запустите установщик.

[Files]
Source: "{#PackageDir}\PerformanceGlass.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageDir}\cpu-sensor\*"; DestDir: "{app}\cpu-sensor"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Performance Glass"; Filename: "{app}\PerformanceGlass.exe"
Name: "{userdesktop}\Performance Glass"; Filename: "{app}\PerformanceGlass.exe"; WorkingDir: "{app}"

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'PerformanceGlass';
  PerformanceGlassWindow = 'PerformanceGlass.Native.Window';
  WindowCommand = $0111;
  ExitApp = 304;
  ProcessQueryAndWait = $00101000; { QUERY_LIMITED_INFORMATION | SYNCHRONIZE }
  ProcessExited = 0;

function GetWindowThreadProcessId(Window: HWND; var ProcessId: Cardinal): Cardinal;
  external 'GetWindowThreadProcessId@user32.dll stdcall';
function OpenProcess(Access: Cardinal; InheritHandle: BOOL; ProcessId: Cardinal): THandle;
  external 'OpenProcess@kernel32.dll stdcall';
function QueryFullProcessImageName(Process: THandle; Flags: Cardinal; Name: String; var Size: Cardinal): BOOL;
  external 'QueryFullProcessImageNameW@kernel32.dll stdcall';
function WaitForSingleObject(Handle: THandle; Timeout: Cardinal): Cardinal;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function CloseHandle(Handle: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall';

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Target, ProcessPath: String;
  Window: HWND;
  Handle: THandle;
  ProcessId, VerifiedProcessId, Size: Cardinal;
begin
  Result := '';
  Target := ExpandConstant('{app}\PerformanceGlass.exe');
  if not FileExists(Target) then
    Exit;

  Window := FindWindowByClassName(PerformanceGlassWindow);
  if (Window = 0) or (GetWindowThreadProcessId(Window, ProcessId) = 0) then
    Exit;
  Handle := OpenProcess(ProcessQueryAndWait, False, ProcessId);
  if Handle = 0 then
    Exit;
  try
    Size := 32768;
    SetLength(ProcessPath, Size);
    if not QueryFullProcessImageName(Handle, 0, ProcessPath, Size) then
      Exit;
    SetLength(ProcessPath, Size);
    if CompareText(ProcessPath, Target) <> 0 then
      Exit;
    if WaitForSingleObject(Handle, 0) = ProcessExited then
      Exit;
    if (GetWindowThreadProcessId(Window, VerifiedProcessId) <> 0) and
      (VerifiedProcessId = ProcessId) then
    begin
      { Use the existing Exit command so that older tray-mode builds also save and exit. }
      Log('Requesting a normal exit of the installed Performance Glass.');
      PostMessage(Window, WindowCommand, ExitApp, 0);
    end;
    if WaitForSingleObject(Handle, 15000) <> ProcessExited then
      Result := CustomMessage('CloseForUpdateFailed')
    else
      Log('Installed Performance Glass exited before replacement.');
  finally
    CloseHandle(Handle);
  end;
end;

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
