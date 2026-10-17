Target amd64-unicode
ManifestDPIAware true

!define APP "Hush"
!define VERSION "1.0.0"
!define PUBLISHER "AlliSighs"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Hush"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
; WM_APP + 10, see src/win/sys.rs
!define WM_HUSH_EXIT 0x800A

!ifndef EXE
  !define EXE "../target/x86_64-pc-windows-gnu/release/Hush.exe"
!endif
!ifndef OUT
  !define OUT "../dist/HushSetup.exe"
!endif

Name "${APP}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\Hush"
RequestExecutionLevel user
SilentInstall silent
SilentUnInstall silent
SetCompressor /SOLID lzma
Icon "../assets/app.ico"
UninstallIcon "../assets/app.ico"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "© 2026 ${PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP} Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!include "FileFunc.nsh"

; Closes a running Hush (installed or not). Takes the unmute flag on the stack.
!macro CLOSE_HUSH un
Function ${un}CloseHush
  Pop $1
  FindWindow $0 "Hush.Host" ""
  IntCmp $0 0 done
  SendMessage $0 ${WM_HUSH_EXIT} $1 0 /TIMEOUT=2000
  StrCpy $2 0
  wait:
    Sleep 100
    FindWindow $0 "Hush.Host" ""
    IntCmp $0 0 done
    IntOp $2 $2 + 1
    IntCmp $2 40 0 wait 0
  nsExec::Exec 'taskkill /F /IM Hush.exe'
  Pop $0
  Sleep 400
  done:
FunctionEnd
!macroend
!insertmacro CLOSE_HUSH ""
!insertmacro CLOSE_HUSH "un."

Section
  Push 0
  Call CloseHush

  SetOutPath "$INSTDIR"
  SetOverwrite try
  StrCpy $3 0
  copy:
    ClearErrors
    File "/oname=Hush.exe" "${EXE}"
    IfErrors 0 copied
    IntOp $3 $3 + 1
    IntCmp $3 20 copied
    Sleep 250
    Goto copy
  copied:

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Hush.lnk" "$INSTDIR\Hush.exe"

  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\Hush.exe,0"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  Exec '"$INSTDIR\Hush.exe"'
SectionEnd

Section "Uninstall"
  Push 1
  Call un.CloseHush

  Delete "$INSTDIR\Hush.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Hush.lnk"

  DeleteRegValue HKCU "${RUN_KEY}" "Hush"
  DeleteRegValue HKCU "${APPROVED_KEY}" "Hush"
  DeleteRegKey HKCU "${UNINST_KEY}"
  RMDir /r "$APPDATA\Hush"
SectionEnd
