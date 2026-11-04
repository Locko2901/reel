; The package with OBS: the portable OBS in $INSTDIR\obs. obs-websocket has no address to bind
; to, so a firewall rule blocks inbound connections to its program; adding it asks for admin once.
!include "${__FILEDIR__}\common.nsh"

!define REEL_WITH_OBS
!searchparse /file "${__FILEDIR__}\..\..\obs\pinned.json" `"version": "` REEL_OBS_VERSION `"`
!searchparse /file "${__FILEDIR__}\..\..\obs\pinned.json" `"source": "` REEL_OBS_SOURCE `"`
!searchparse /file "${__FILEDIR__}\..\..\obs\pinned.json" `"sha256": "` REEL_OBS_SHA256 `"`

; obs\reel-obs.txt names the OBS the folder holds, written once all its files are in place: an
; update that brings the same OBS leaves its thousands of files alone.
!define REEL_OBS_MARK "reel-obs.txt"
!define REEL_OBS_ID "OBS Studio ${REEL_OBS_VERSION} ${REEL_OBS_SHA256}"
Var ReelKeepObs

!macro NSIS_HOOK_PREINSTALL
  !insertmacro REEL_STOP_FOR_INSTALL
  StrCpy $ReelKeepObs 0
  ${If} $UpdateMode = 1
  ${AndIf} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}"
    ClearErrors
    FileOpen $0 "$INSTDIR\obs\${REEL_OBS_MARK}" r
    ${IfNot} ${Errors}
      FileRead $0 $1
      FileClose $0
      ${If} $1 == "${REEL_OBS_ID}"
        StrCpy $ReelKeepObs 1
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ${If} $ReelKeepObs = 1
    !insertmacro REEL_SAY "OBS Studio ${REEL_OBS_VERSION} stays as it is"
  ${Else}
    !insertmacro REEL_FRESH_OBS
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $ReelKeepObs <> 1
    FileOpen $0 "$INSTDIR\obs\${REEL_OBS_MARK}" w
    FileWrite $0 "${REEL_OBS_ID}"
    FileClose $0
  ${EndIf}
  !insertmacro REEL_SAY "Keeping OBS private to this PC"
  nsExec::ExecToStack `"$SYSDIR\netsh.exe" advfirewall firewall show rule name="${REEL_OBS_RULE}"`
  Pop $0
  Pop $1
  ${If} $0 != 0
    InitPluginsDir
    FileOpen $1 "$PLUGINSDIR\reel-rules.txt" w
    FileWrite $1 `advfirewall firewall delete rule name="${REEL_OBS_RULE_BEFORE}"$\r$\n`
    FileWrite $1 `advfirewall firewall add rule name="${REEL_OBS_RULE}" dir=in action=block program="$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}" enable=yes profile=any$\r$\n`
    FileClose $1
    ExecShellWait "runas" "$SYSDIR\netsh.exe" `-f "$PLUGINSDIR\reel-rules.txt"` SW_HIDE
  ${EndIf}
  !insertmacro REEL_PUT_HELPER
  !insertmacro REEL_INSTALL_DONE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro REEL_STOP_FOR_INSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro REEL_REMOVE_BUNDLED_OBS
  ${EndIf}
  Delete "$INSTDIR\reel-elevate.exe"
  !insertmacro REEL_REMOVE_CHOSEN
  !insertmacro REEL_INSTALL_DONE
  RMDir "$INSTDIR"
!macroend
