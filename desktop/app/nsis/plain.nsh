; The package for a PC with OBS already. Over the one with OBS it removes the bundled OBS once
; its box is ticked, or silently with /WITHOUTOBS; without it, it stops before anything is touched.
!include "${__FILEDIR__}\common.nsh"

Var ReelRemoveObs

!macro NSIS_HOOK_PREINSTALL
  Push $0
  StrCpy $0 0
  ${If} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}"
  ${OrIf} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE_BEFORE}"
    StrCpy $0 1
  ${EndIf}
  ${If} $0 = 1
  ${AndIf} $ReelRemoveObs <> 1
    !insertmacro REEL_SAY "${REEL_NAME_WITH_OBS} is on this PC with its OBS, and this installer would remove that OBS"
    System::Call 'kernel32::AttachConsole(i -1)i.r0'
    ${If} $0 <> 0
      System::Call 'kernel32::GetStdHandle(i -11)i.r0'
      FileWrite $0 "${REEL_NAME_WITH_OBS} is on this PC with its OBS: run ${REEL_FILE_WITH_OBS}_${VERSION}_x64-setup.exe to keep that OBS, or this installer with /WITHOUTOBS to remove it.$\r$\n"
    ${EndIf}
    Abort "${REEL_NAME_WITH_OBS} is on this PC with its OBS: run ${REEL_FILE_WITH_OBS}_${VERSION}_x64-setup.exe to keep that OBS"
  ${EndIf}
  Pop $0
  !insertmacro REEL_STOP_FOR_INSTALL
  ${If} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}"
  ${OrIf} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE_BEFORE}"
    !insertmacro REEL_REMOVE_BUNDLED_OBS
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro REEL_PUT_HELPER
  !insertmacro REEL_INSTALL_DONE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro REEL_STOP_FOR_INSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro REEL_REMOVE_COPY
  ${EndIf}
  Delete "$INSTDIR\reel-elevate.exe"
  !insertmacro REEL_REMOVE_CHOSEN
  !insertmacro REEL_INSTALL_DONE
  RMDir "$INSTDIR"
!macroend
