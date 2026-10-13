; Inno Setup 6.7+; build with build.ps1. Install for the current user.
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef BinaryPath
  #define BinaryPath "..\target\release\kitlane.exe"
#endif
#ifndef ArtworkPath
  #define ArtworkPath "..\target\installer-assets"
#endif
#if VER < EncodeVer(6, 7, 0)
  #error Inno Setup 6.7 or newer is required
#endif

[Setup]
AppId={{11F04F7E-B024-45E3-88C4-E1DAD3E0DECC}
AppName=Kitlane
AppVersion={#AppVersion}
AppPublisher=Kitlane
DefaultDirName={localappdata}\Programs\Kitlane
DefaultGroupName=Kitlane
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=..\..\dist
OutputBaseFilename=Kitlane-{#AppVersion}-Setup-x64
SetupIconFile=..\..\design-system\assets\icons\app-icon.ico
UninstallDisplayIcon={app}\kitlane.ico
WizardStyle=modern dynamic windows11 includetitlebar
WizardSizePercent=120
WizardBackColor=#F4F7FA
WizardBackColorDynamicDark=#091C31
WizardImageFile={#ArtworkPath}\welcome-light.png
WizardImageFileDynamicDark={#ArtworkPath}\welcome-dark.png
WizardSmallImageFile=..\..\design-system\assets\icons\app-icon-64.png
WizardSmallImageFileDynamicDark=..\..\design-system\assets\icons\app-icon-64.png
WizardImageStretch=yes
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=yes
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
UninstallDisplayName=Kitlane

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#BinaryPath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\design-system\assets\icons\app-icon.ico"; DestDir: "{app}"; DestName: "kitlane.ico"; Flags: ignoreversion
Source: "..\..\design-system\assets\fonts\Inter-OFL.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\..\design-system\assets\fonts\Sora-OFL.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\assets\programs\NOTICE.md"; DestDir: "{app}\licenses"; DestName: "Program-Logos.md"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Kitlane"; Filename: "{app}\kitlane.exe"; IconFilename: "{app}\kitlane.ico"
Name: "{autodesktop}\Kitlane"; Filename: "{app}\kitlane.exe"; IconFilename: "{app}\kitlane.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\kitlane.exe"; Description: "Launch Kitlane and set up your workspace"; Flags: nowait postinstall skipifsilent

[Messages]
WelcomeLabel1=Your apps. A clearer workspace.
WelcomeLabel2=Welcome to Kitlane.%n%nChoose where to install and which shortcuts to create. When you open Kitlane, a short setup helps you choose appearance, name this device, and decide when to scan your apps.%n%nYour library stays on this computer until you explicitly choose to share it.
FinishedHeadingLabel=Your workspace starts here.
FinishedLabel=Kitlane is installed.%n%nLaunch Kitlane to personalize your workspace. You can pair devices later.%n%nUninstalling Kitlane keeps your local library and settings.
