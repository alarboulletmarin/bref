; Installeur Windows de Bref. Compilé par .github/workflows/release-binaries.yml :
;   BREF_VERSION=0.2.0 iscc packaging/windows/bref.iss   (produit dist/bref-windows-x86_64-setup.exe)
#define Version GetEnv("BREF_VERSION")

[Setup]
AppId={{9A5A9701-FBF2-446B-8861-E78C81C9CA55}
AppName=Bref
AppVersion={#Version}
AppPublisher=Andrea Larboullet Marin
AppPublisherURL=https://github.com/alarboulletmarin/bref
DefaultDirName={autopf}\Bref
DefaultGroupName=Bref
; Installation pour l'utilisateur seul (%LOCALAPPDATA%\Programs\Bref) : aucune demande d'administrateur.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=bref.ico
UninstallDisplayIcon={app}\bref.exe
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
OutputBaseFilename=bref-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "..\..\target\release\bref.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Bref"; Filename: "{app}\bref.exe"
Name: "{autodesktop}\Bref"; Filename: "{app}\bref.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\bref.exe"; Description: "{cm:LaunchProgram,Bref}"; Flags: nowait postinstall skipifsilent
