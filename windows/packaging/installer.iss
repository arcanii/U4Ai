; Inno Setup 6 script for U4Ai (Windows x64). Built by
; windows\scripts\release.ps1, which passes the version and stages the files
; in target\dist\stage first:
;   ISCC /DMyVer=0.1.0 /DMyShown=0.1.0.1 /DMyBuild=42 windows\packaging\installer.iss
; MyShown is the version people see, a patch release's number and all (MyVer
; when there's none); MyVer.MyBuild is the one installs and WinSparkle
; compare. Produces target\dist\U4Ai-<shown>-setup.exe, the file
; WinSparkle installs. Relative paths start from this file's folder, so
; ..\.. is the repo root.
;
; It installs for the user alone, with no admin rights, into
; %LOCALAPPDATA%\Programs\U4Ai, so WinSparkle's updates never ask for
; them either.

#ifndef MyVer
  #define MyVer "0.0.0"
#endif
#ifndef MyShown
  #define MyShown MyVer
#endif
#ifndef MyBuild
  #define MyBuild "0"
#endif
; A test build of work toward a version shows it as 0.1.0.DEV, but Windows'
; product version takes numbers alone: those there, the words in its text.
#define MyShownNumbers StringChange(MyShown, ".DEV", "")
#define MyApp "U4Ai"
#define Stage "..\..\target\dist\stage"

[Setup]
; release.ps1's -SmokeTest reads the AppId from here, to leave a real
; install alone.
AppId={{7624AA5B-C2F6-4B1B-B5CE-E5C5E61FE5B7}
AppName={#MyApp}
AppVersion={#MyVer}.{#MyBuild}
AppVerName={#MyApp} {#MyShown} (build {#MyBuild})
AppPublisher=arcanii
AppPublisherURL=https://github.com/arcanii/U4Ai
AppSupportURL=https://github.com/arcanii/U4Ai/issues
VersionInfoVersion={#MyVer}.{#MyBuild}
VersionInfoProductVersion={#MyShownNumbers}
VersionInfoProductTextVersion={#MyShown}
; Per user, no admin: {userpf} is %LOCALAPPDATA%\Programs.
PrivilegesRequired=lowest
DefaultDirName={userpf}\{#MyApp}
DisableProgramGroupPage=yes
SetupIconFile=..\..\assets\icon\u4ai.ico
UninstallDisplayIcon={app}\u4ai.ico
OutputDir=..\..\target\dist
OutputBaseFilename={#MyApp}-{#MyShown}-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
; AppMutex matches the mutex U4Ai holds while it runs, so setup and the
; uninstaller ask for it to be closed first. Restart Manager closes one that
; WinSparkle's shutdown request left running. [Run] starts it again, so
; Restart Manager doesn't (which would start it twice).
AppMutex=U4Ai.Running
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#Stage}\u4ai.exe"; DestDir: "{app}"; Flags: ignoreversion
; The updater, which U4Ai loads from beside it (windows\src\updater.rs).
Source: "{#Stage}\WinSparkle.dll"; DestDir: "{app}"; Flags: ignoreversion
; For the Start menu and Apps & features, whatever icon the exe carries.
Source: "{#Stage}\u4ai.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyApp}"; Filename: "{app}\u4ai.exe"; IconFilename: "{app}\u4ai.ico"

[Registry]
; Start at login writes this value for the user (windows\src); installing
; makes none, and uninstalling takes it away.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "U4Ai"; Flags: uninsdeletevalue dontcreatekey

[Run]
; Run by hand: a box on the last page, ticked.
Filename: "{app}\u4ai.exe"; Description: "Start {#MyApp}"; Flags: nowait postinstall skipifsilent
; Run by WinSparkle (the appcast's sparkle:installerArguments make it
; /SILENT): U4Ai quit for the update, so it starts again. /NOLAUNCH, which
; release.ps1's -SmokeTest passes, leaves it closed.
Filename: "{app}\u4ai.exe"; Flags: nowait skipifnotsilent; Check: LaunchWhenSilent

[Code]
function LaunchWhenSilent: Boolean;
var
  I: Integer;
begin
  Result := True;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/NOLAUNCH') = 0 then
      Result := False;
end;
