; text_killer Windows installer (NSIS 3).
; Build:  makensis -DVERSION=1.2.0 -DSRCDIR=<folder with text_killer.exe> -DOUTFILE=<setup.exe> installer.nsi
;
; Installs for the current user (no administrator rights needed) into
; %LOCALAPPDATA%\Programs\text_killer, adds that folder to the user's PATH,
; creates Start-menu shortcuts and registers an uninstaller in
; Settings > Apps.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!ifndef VERSION
  !error "pass -DVERSION=x.y.z"
!endif
!ifndef SRCDIR
  !define SRCDIR "."
!endif
!ifndef OUTFILE
  !define OUTFILE "text_killer-${VERSION}-windows-x86_64-setup.exe"
!endif

!define APP "text_killer"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT License"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "StrFunc.nsh"
${StrStr}
${UnStrRep}

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "This will install ${APP} ${VERSION}, a terminal text editor with an IRC-style interface.$\r$\n$\r$\nIt is installed for your user account only and added to your PATH, so you can run it from Windows Terminal or PowerShell by typing:$\r$\n$\r$\n    text_killer myfile.txt"
!define MUI_FINISHPAGE_TEXT "${APP} is installed.$\r$\n$\r$\nOpen a new Windows Terminal or PowerShell window and type:$\r$\n$\r$\n    text_killer myfile.txt$\r$\n$\r$\nPress F1 inside the editor for help."
!define MUI_FINISHPAGE_SHOWREADME "$INSTDIR\MANUAL.txt"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Open the manual"
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SRCDIR}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; NSIS strings are limited to NSIS_MAX_STRLEN characters. To avoid ever
; truncating the user's PATH, it is only edited when comfortably shorter.
!define PATH_LIMIT 900

Section "text_killer" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${SRCDIR}\text_killer.exe"
  File "${SRCDIR}\MANUAL.txt"
  File "${SRCDIR}\LICENSE.txt"
  File "${SRCDIR}\THIRD-PARTY-LICENSES.txt"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; --- Add to the user's PATH -------------------------------------------
  ClearErrors
  ReadRegStr $0 HKCU "Environment" "Path"
  ${If} ${Errors}
    StrCpy $0 ""
  ${EndIf}
  StrLen $2 $0
  ${If} $2 > ${PATH_LIMIT}
    DetailPrint "PATH is long; not modified. Add $INSTDIR to PATH manually."
    MessageBox MB_ICONINFORMATION|MB_OK "Your PATH is too long to update safely.$\r$\nTo run text_killer from any terminal, add this folder to PATH:$\r$\n$\r$\n$INSTDIR"
  ${Else}
    ${StrStr} $1 ";$0;" ";$INSTDIR;"
    ${If} $1 == ""
      ${If} $0 == ""
        StrCpy $0 "$INSTDIR"
      ${Else}
        StrCpy $3 $0 1 -1
        ${If} $3 == ";"
          StrCpy $0 "$0$INSTDIR"
        ${Else}
          StrCpy $0 "$0;$INSTDIR"
        ${EndIf}
      ${EndIf}
      WriteRegExpandStr HKCU "Environment" "Path" $0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
      DetailPrint "Added $INSTDIR to PATH"
    ${EndIf}
  ${EndIf}

  ; --- Shortcuts ---------------------------------------------------------
  CreateDirectory "$SMPROGRAMS\${APP}"
  CreateShortCut "$SMPROGRAMS\${APP}\${APP}.lnk" "$INSTDIR\text_killer.exe"
  CreateShortCut "$SMPROGRAMS\${APP}\${APP} manual.lnk" "$INSTDIR\MANUAL.txt"
  CreateShortCut "$SMPROGRAMS\${APP}\Uninstall ${APP}.lnk" "$INSTDIR\uninstall.exe"

  ; --- Settings > Apps entry --------------------------------------------
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "text_killer"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\text_killer.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" 2048
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\text_killer.exe"
  Delete "$INSTDIR\MANUAL.txt"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD-PARTY-LICENSES.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP}\${APP}.lnk"
  Delete "$SMPROGRAMS\${APP}\${APP} manual.lnk"
  Delete "$SMPROGRAMS\${APP}\Uninstall ${APP}.lnk"
  RMDir "$SMPROGRAMS\${APP}"

  ; Remove our folder from PATH (only the exact entry we added).
  ClearErrors
  ReadRegStr $0 HKCU "Environment" "Path"
  ${IfNot} ${Errors}
    StrLen $2 $0
    ${If} $2 <= ${PATH_LIMIT}
      StrCpy $0 ";$0;"
      ${UnStrRep} $0 $0 ";$INSTDIR;" ";"
      ; Trim the sentinel semicolons added above.
      StrCpy $0 $0 "" 1
      StrCpy $0 $0 -1
      WriteRegExpandStr HKCU "Environment" "Path" $0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    ${EndIf}
  ${EndIf}

  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
