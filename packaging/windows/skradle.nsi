; SKradle's Windows installer (NSIS). Per user: it needs no administrator rights and installs into
; %LOCALAPPDATA%\Programs\SKradle (or where an earlier install put it). Built by packaging/windows.sh.
;   /S          silent
;   /UPDATE     start SKradle when done (the launcher's own update uses `/S /UPDATE`; it quits first, and the
;               installer waits until skradle.exe can be replaced)
; Not code-signed, so SmartScreen may ask once ("More info" > "Run anyway").
Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !error "pass -DVERSION=<version>"
!endif
!ifndef VERSION_NUM
  !error "pass -DVERSION_NUM=<a.b.c.d>"
!endif
!ifndef SRC
  !error "pass -DSRC=<folder with skradle.exe and its Qt files>"
!endif
!ifndef OUT
  !error "pass -DOUT=<installer file>"
!endif

Name "SKradle"
OutFile "${OUT}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\SKradle"
InstallDirRegKey HKCU "Software\SKradle" "InstallDir"
SetCompressor /SOLID lzma
BrandingText "SKradle ${VERSION}"

VIProductVersion "${VERSION_NUM}"
VIAddVersionKey "ProductName" "SKradle"
VIAddVersionKey "FileDescription" "SKradle installer"
VIAddVersionKey "LegalCopyright" "SKradle"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!define MUI_ICON "${SRC}\skradle.ico"
!define MUI_UNICON "${SRC}\skradle.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\skradle.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start SKradle"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "SKradle"
  ; An update starts while the old launcher is still closing: wait (up to 20 s) until its files can be replaced.
  ${For} $R0 1 40
    ClearErrors
    Delete "$INSTDIR\skradle.exe"
    ${IfNot} ${Errors}
      ${Break}
    ${EndIf}
    Sleep 500
  ${Next}
  SetOutPath "$INSTDIR"
  File /r "${SRC}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\SKradle" "InstallDir" "$INSTDIR"
  CreateShortCut "$SMPROGRAMS\SKradle.lnk" "$INSTDIR\skradle.exe" "" "$INSTDIR\skradle.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "DisplayName" "SKradle"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "DisplayIcon" "$INSTDIR\skradle.ico"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle" "NoRepair" 1
SectionEnd

Function .onInstSuccess
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/UPDATE" $R1
  ${IfNot} ${Errors}
    Exec '"$INSTDIR\skradle.exe"'
  ${EndIf}
FunctionEnd

Section "Uninstall"
  ; Only SKradle's own program folder. The player's settings, mods and game files live elsewhere and stay.
  RMDir /r "$INSTDIR"
  Delete "$SMPROGRAMS\SKradle.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SKradle"
  DeleteRegKey HKCU "Software\SKradle"
SectionEnd
