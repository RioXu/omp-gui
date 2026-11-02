#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifndef InstallerAppId
  #define InstallerAppId "RioXu.OMP.GUI"
#endif

[Setup]
AppId={#InstallerAppId}
AppName=OMP GUI
AppVersion={#AppVersion}
AppPublisher=RioXu
AppPublisherURL=https://github.com/RioXu/omp-gui
AppSupportURL=https://github.com/RioXu/omp-gui/issues
DefaultDirName={localappdata}\Programs\OMP GUI
DefaultGroupName=OMP GUI
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.17763
OutputDir={#OutputDir}
OutputBaseFilename=omp-gui-{#AppVersion}-windows-x86_64-setup
SetupIconFile={#SourcePath}..\resources\omp.ico
UninstallDisplayIcon={app}\omp-gui.exe
LicenseFile={#SourcePath}..\LICENSE
VersionInfoVersion={#AppVersion}.0
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayName=OMP GUI

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\OMP GUI"; Filename: "{app}\omp-gui.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\OMP GUI"; Filename: "{app}\omp-gui.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\omp-gui.exe"; Description: "Launch OMP GUI"; Flags: nowait postinstall skipifsilent

; No user-data deletion rules: uninstall only removes files owned by this installer.
