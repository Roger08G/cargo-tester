#ifndef MyAppVersion
  #error MyAppVersion must be provided to ISCC
#endif

#ifndef MySourceExe
  #error MySourceExe must be provided to ISCC
#endif

#ifndef MyRepoRoot
  #error MyRepoRoot must be provided to ISCC
#endif

#ifndef MyOutputDir
  #error MyOutputDir must be provided to ISCC
#endif

[Setup]
AppId={{65E35D25-4A26-4809-822A-3425F9C67CCB}
AppName=Cargo Tester
AppVersion={#MyAppVersion}
AppPublisher=Roger Gómez Martínez
AppPublisherURL=https://github.com/Roger08G/cargo-tester
AppSupportURL=https://github.com/Roger08G/cargo-tester/issues
AppUpdatesURL=https://github.com/Roger08G/cargo-tester/releases/latest
DefaultDirName={localappdata}\Programs\Cargo Tester
DefaultGroupName=Cargo Tester
DisableProgramGroupPage=yes
OutputDir={#MyOutputDir}
OutputBaseFilename=cargo-tester-v{#MyAppVersion}-windows-x86_64-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
VersionInfoVersion={#MyAppVersion}
VersionInfoCompany=Roger Gómez Martínez
VersionInfoDescription=Cargo Tester installer
VersionInfoProductName=Cargo Tester
VersionInfoProductVersion={#MyAppVersion}
SetupLogging=yes

[Files]
Source: "{#MySourceExe}"; DestDir: "{code:GetCargoBin}"; DestName: "cargo-tester.exe"; Flags: ignoreversion
Source: "{#MyRepoRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MyRepoRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Code]
function GetCargoBin(Param: String): String;
var
  CargoHome: String;
begin
  CargoHome := GetEnv('CARGO_HOME');
  if CargoHome = '' then
    CargoHome := AddBackslash(GetEnv('USERPROFILE')) + '.cargo';

  Result := AddBackslash(CargoHome) + 'bin';
end;
