; Shared by both installers' hooks. OBS's obs64.exe runs under a name of reel's; the firewall
; rule names the program, so a rule for the old name is told apart without reading netsh.
!define REEL_OBS_EXE "reel-capture.exe"
!define REEL_OBS_EXE_BEFORE "obs64.exe"
!define REEL_OBS_RULE "reel: the bundled OBS (reel-capture.exe) takes no inbound connections"
!define REEL_OBS_RULE_BEFORE "reel: the bundled OBS takes no inbound connections"
; This folder, fixed here: in a macro ${__FILEDIR__} is the folder of the file it is used in.
!define REEL_NSIS_DIR "${__FILEDIR__}"
!searchparse /file "${__FILEDIR__}\..\..\packages.json" `"with_obs_name": "` REEL_NAME_WITH_OBS `"`
!searchparse /file "${__FILEDIR__}\..\..\packages.json" `"with_obs_file": "` REEL_FILE_WITH_OBS `"`
!searchparse /file "${__FILEDIR__}\..\..\packages.json" `"plain_name": "` REEL_NAME_PLAIN `"`

; NSIS's own detail lines (paths) are off for the whole section; reel's are said with this,
; which also puts them above the bar.
!macro REEL_SAY TEXT
  SetDetailsPrint both
  DetailPrint "${TEXT}"
  SetDetailsPrint none
  !insertmacro REEL_TIME "${TEXT}"
!macroend

; Each step with its time since the installer's start, in logs\installer.log, to read beside
; the app's own log.
Var ReelTimes
Var ReelT0
Var ReelReadyAt
Var ReelBorn
!macro REEL_TIMES_INIT
  System::Call 'kernel32::GetTickCount() i .r0'
  StrCpy $ReelT0 $0
  System::Call '*(l, l, l, l) p .r1'
  System::Call '*(l, l, l, l) p .r6'
  System::Call 'kernel32::GetCurrentProcess() p .r2'
  System::Call 'kernel32::GetProcessTimes(p r2, p r1, p r6, p r6, p r6)'
  System::Free $6
  System::Call '*(&i2, &i2, &i2, &i2, &i2, &i2, &i2, &i2) p .r3'
  System::Call 'kernel32::FileTimeToSystemTime(p r1, p r3)'
  System::Call '*$3(&i2 .r4, &i2 .r5, &i2, &i2 .r6, &i2 .r7, &i2 .r8, &i2 .r9, &i2 .r0)'
  IntFmt $5 "%02d" $5
  IntFmt $6 "%02d" $6
  IntFmt $7 "%02d" $7
  IntFmt $8 "%02d" $8
  IntFmt $9 "%02d" $9
  IntFmt $0 "%03d" $0
  StrCpy $ReelBorn "$4-$5-$6T$7:$8:$9.$0Z"
  System::Call '*$1(l .r4)'
  System::Call 'kernel32::GetSystemTimeAsFileTime(p r1)'
  System::Call '*$1(l .r5)'
  System::Int64Op $5 - $4
  Pop $5
  System::Int64Op $5 / 10000
  Pop $ReelReadyAt
  System::Free $1
  System::Free $3
!macroend
!macro REEL_SECONDS OUT MS
  IntOp $R8 ${MS} / 1000
  IntOp $R9 ${MS} % 1000
  IntOp $R9 $R9 / 10
  IntFmt $R9 "%02d" $R9
  StrCpy ${OUT} "$R8.$R9 s"
!macroend
!macro REEL_TIMES_BEGIN
  CreateDirectory "$INSTDIR\logs"
  FileOpen $ReelTimes "$INSTDIR\logs\installer.log" w
  FileWrite $ReelTimes "reel ${VERSION}'s installer, started $ReelBorn, with $CMDLINE$\r$\n"
  !insertmacro REEL_SECONDS $R7 $ReelReadyAt
  FileWrite $ReelTimes "$R7: ready, its own check done$\r$\n"
!macroend
!macro REEL_TIME TEXT
  ${If} $ReelTimes != ""
    Push $R7
    Push $R8
    Push $R9
    System::Call 'kernel32::GetTickCount() i .R7'
    IntOp $R7 $R7 - $ReelT0
    IntOp $R7 $R7 + $ReelReadyAt
    !insertmacro REEL_SECONDS $R7 $R7
    FileWrite $ReelTimes "$R7: ${TEXT}$\r$\n"
    Pop $R9
    Pop $R8
    Pop $R7
  ${EndIf}
!macroend
!macro REEL_TIMES_END
  ${If} $ReelTimes != ""
    !insertmacro REEL_TIME "the installer ends"
    FileClose $ReelTimes
    StrCpy $ReelTimes ""
  ${EndIf}
!macroend

; Before anything in the folder is touched (Tauri's check for a running app comes too late):
; reel-stop.exe ends the app and whatever runs from obs\, and waits until OBS's files are free.
!define REEL_MARK "reel-installing"
!macro REEL_STOP_FOR_INSTALL
  Push $R9
  InitPluginsDir
  File "/oname=$PLUGINSDIR\reel-stop.exe" "${REEL_NSIS_DIR}\reel-stop.exe"
  StrCpy $R9 "again"
  ${DoWhile} $R9 == "again"
    !insertmacro REEL_SAY "Closing reel"
    nsExec::ExecToStack `"$PLUGINSDIR\reel-stop.exe" "$INSTDIR"`
    Pop $0
    Pop $1
    ${If} $0 == 0
      StrCpy $R9 "done"
    ${ElseIf} ${Silent}
      !insertmacro REEL_SAY "$1"
      Abort "$1"
    ${Else}
      !insertmacro REEL_SAY "$1"
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "reel could not stop what runs from its folder:$\n$\n$1$\nClose it, then Retry." IDRETRY +2
      Abort "$1"
    ${EndIf}
  ${Loop}
  Pop $R9
!macroend

; One of Tauri's resources; an update that keeps its OBS skips OBS's files.
!macro REEL_RESOURCE TO FROM
  !ifdef REEL_WITH_OBS
    StrCpy $R9 "${TO}" 4
    ${If} $R9 != "obs\"
    ${OrIf} $ReelKeepObs <> 1
      File /a "/oname=${TO}" "${FROM}"
    ${EndIf}
  !else
    File /a "/oname=${TO}" "${FROM}"
  !endif
!macroend

; The mark that kept the app from starting OBS goes.
!macro REEL_INSTALL_DONE
  Delete "$INSTDIR\${REEL_MARK}"
!macroend

; reel-elevate.exe, the one program of reel's that runs as administrator: it makes and removes
; the copy of OBS in Program Files for games that run as administrator.
!macro REEL_PUT_HELPER
  SetOutPath "$INSTDIR"
  File "${REEL_NSIS_DIR}\reel-elevate.exe"
!macroend

; Only an administrator removes the copy in Program Files, so Windows asks, and only when it is
; there. The copy's own helper does it, else the one beside the app.
!macro REEL_REMOVE_COPY
  ${If} ${FileExists} "$PROGRAMFILES64\reel\obs\bin\64bit\${REEL_OBS_EXE}"
  ${OrIf} ${FileExists} "$PROGRAMFILES64\reel\obs\bin\64bit\${REEL_OBS_EXE_BEFORE}"
    Push $R8
    StrCpy $R8 "$INSTDIR\reel-elevate.exe"
    ${If} ${FileExists} "$PROGRAMFILES64\reel\reel-elevate.exe"
      StrCpy $R8 "$PROGRAMFILES64\reel\reel-elevate.exe"
    ${EndIf}
    ${If} ${FileExists} "$R8"
      ExecShellWait "runas" "$R8" `remove "$INSTDIR"` SW_HIDE
    ${Else}
      !insertmacro REEL_SAY "The copy of OBS for games that run as administrator stays: its helper is gone"
    ${EndIf}
    Pop $R8
  ${EndIf}
!macroend

; On an uninstall or a switch to the package without OBS, with the copy in Program Files; an
; update keeps them.
!macro REEL_REMOVE_BUNDLED_OBS
  !insertmacro REEL_STOP_FOR_INSTALL
  !insertmacro REEL_SAY "Removing the OBS that came with reel"
  !insertmacro REEL_REMOVE_COPY
  RMDir /r "$INSTDIR\obs"
  nsExec::ExecToStack `"$SYSDIR\netsh.exe" advfirewall firewall show rule name="${REEL_OBS_RULE}"`
  Pop $0
  Pop $1
  nsExec::ExecToStack `"$SYSDIR\netsh.exe" advfirewall firewall show rule name="${REEL_OBS_RULE_BEFORE}"`
  Pop $2
  Pop $1
  ${If} $0 == 0
  ${OrIf} $2 == 0
    InitPluginsDir
    FileOpen $1 "$PLUGINSDIR\reel-rules.txt" w
    FileWrite $1 `advfirewall firewall delete rule name="${REEL_OBS_RULE}"$\r$\n`
    FileWrite $1 `advfirewall firewall delete rule name="${REEL_OBS_RULE_BEFORE}"$\r$\n`
    FileClose $1
    ExecShellWait "runas" "$SYSDIR\netsh.exe" `-f "$PLUGINSDIR\reel-rules.txt"` SW_HIDE
  ${EndIf}
!macroend

; Every file of the OBS there before goes but obs\config (OBS's settings): a plugin left from
; another version breaks OBS's start.
!macro REEL_FRESH_OBS
  ${If} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}"
  ${OrIf} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE_BEFORE}"
    !insertmacro REEL_SAY "Making room for the new OBS"
  ${EndIf}
  Push $R0
  Push $R1
  FindFirst $R0 $R1 "$INSTDIR\obs\*"
  ${DoWhile} $R1 != ""
    ${If} $R1 != "."
    ${AndIf} $R1 != ".."
    ${AndIf} $R1 != "config"
      RMDir /r "$INSTDIR\obs\$R1"
      Delete "$INSTDIR\obs\$R1"
    ${EndIf}
    FindNext $R0 $R1
  ${Loop}
  FindClose $R0
  Pop $R1
  Pop $R0
!macroend

; The uninstaller page's choices, never on an update; a silent or passive uninstall keeps both.
; reel-stop.exe checks the clips folder again before it deletes it, never one of REEL_PLACES.
!define REEL_PLACES `"$PROFILE" "$WINDIR" "$PROGRAMFILES" "$PROGRAMFILES64" "$COMMONFILES" "$COMMONFILES64" "$LOCALAPPDATA" "$APPDATA" "$APPDATA\reel" "$VIDEOS" "$DOCUMENTS" "$DESKTOP" "$PICTURES" "$MUSIC"`
Var ReelRemoveData
Var ReelDeleteClips
Var ReelClipsDir
!macro REEL_REMOVE_CHOSEN
  ${If} $UpdateMode <> 1
    ${If} $ReelDeleteClips = 1
    ${AndIf} $ReelClipsDir != ""
      !insertmacro REEL_SAY "Deleting your clips folder"
      nsExec::ExecToStack `"$PLUGINSDIR\reel-stop.exe" --delete-clips "$ReelClipsDir" "$INSTDIR" ${REEL_PLACES}`
      Pop $0
      Pop $1
      ${If} $0 == 0
        !insertmacro REEL_SAY "Your clips folder is deleted"
      ${Else}
        !insertmacro REEL_SAY "$1"
      ${EndIf}
    ${EndIf}
    ${If} $ReelRemoveData = 1
      !insertmacro REEL_SAY "Removing reel's settings, logs and pairing"
      nsExec::ExecToStack `"$PLUGINSDIR\reel-stop.exe" --forget`
      Pop $0
      Pop $1
      RMDir /r "$APPDATA\reel"
      RMDir /r "$INSTDIR\logs"
      RMDir /r "$INSTDIR\cues"
      Delete "$INSTDIR\ledger.json"
    ${EndIf}
  ${EndIf}
!macroend
