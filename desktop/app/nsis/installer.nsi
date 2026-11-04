; Tauri's installer.nsi of tauri-cli 2.12.1 (tauri-bundler 2.10.1) with reel's pages, words and colours.
; build-windows.sh refuses another tauri-cli until this is merged with its template.
Unicode true
ManifestDPIAware true
; PerMonitorV2 for Windows 10 1607 and later; older Windows reads ManifestDPIAware.
ManifestDPIAwareness PerMonitorV2

!if "{{compression}}" == "none"
  SetCompress off
!else
  ; The package with OBS compresses each file on its own, 2 % larger than solid: an update that
  ; keeps its OBS skips OBS's files without unpacking them.
  !searchparse /noerrors "{{installer_hooks}}" "with-obs.nsh" REEL_HOOKS_WITH_OBS
  !ifdef REEL_HOOKS_WITH_OBS
    SetCompressor "{{compression}}"
  !else
    SetCompressor /SOLID "{{compression}}"
  !endif
!endif

; Before any !include, to stay ahead of every plugin command.
{{#if signed_plugins_path}}
!addplugindir "{{signed_plugins_path}}"
{{/if}}

!include MUI2.nsh
!include FileFunc.nsh
!include x64.nsh
!include WordFunc.nsh
!include "utils.nsh"
!include "FileAssociation.nsh"
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "Win\RestartManager.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define HOMEPAGE "{{homepage}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define UNINSTALLERICON "{{uninstaller_icon}}"
!define UNINSTALLERHEADERIMAGE "{{uninstaller_header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define BUNDLEID "{{bundle_id}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define ADDITIONALPLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define INSTALLWEBVIEW2MODE "{{install_webview2_mode}}"
!define WEBVIEW2INSTALLERARGS "{{webview2_installer_args}}"
!define WEBVIEW2BOOTSTRAPPERPATH "{{webview2_bootstrapper_path}}"
!define WEBVIEW2INSTALLERPATH "{{webview2_installer_path}}"
!define MINIMUMWEBVIEW2VERSION "{{minimum_webview2_version}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUKEY "Software\${MANUFACTURER}"
!define MANUPRODUCTKEY "${MANUKEY}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define ESTIMATEDSIZE "{{estimated_size}}"
!define STARTMENUFOLDER "{{start_menu_folder}}"

Var PassiveMode
Var UpdateMode
Var NoShortcutMode
Var WixMode
Var OldMainBinaryName

!ifdef REEL_WITH_OBS
  !define REEL_TITLE "${REEL_NAME_WITH_OBS}"
  !define REEL_WHAT "Save what just happened in your game with one key, and share it with a link. OBS Studio comes with it, set up for you and running out of sight."
  !define REEL_NEXT "Open it to set up OBS in a few quick steps: clip length, quality and hotkey."
  !define REEL_GOES "This removes reel and the OBS that came with it, with OBS's settings. Choose what else goes."
  !define REEL_FOR_YOU "you. Windows asks for admin rights once, to add a firewall rule that keeps OBS private to this PC."
  !define REEL_FOR_LINES 2
!else
  !define REEL_TITLE "${REEL_NAME_PLAIN}"
  !define REEL_WHAT "Save what just happened in your game with one key, and share it with a link. reel works with the OBS you already have."
  !define REEL_NEXT "Open it to connect it to your OBS."
  !define REEL_GOES "This removes reel from this PC. Choose what else goes with it."
  !define REEL_FOR_YOU "you, no admin rights needed"
  !define REEL_FOR_LINES 1
!endif

Name "${PRODUCTNAME}"
SetFont "Segoe UI" 9
Caption "${REEL_TITLE}"
UninstallCaption "Remove reel"
BrandingText "${REEL_TITLE} ${VERSION}"
UninstallButtonText "Remove"
OutFile "${OUTFILE}"

; A placeholder for NSIS's directory selector; .onInit sets the real folder.
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
InstallDir "${PLACEHOLDER_INSTALL_DIR}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!addplugindir "${ADDITIONALPLUGINSPATH}"

!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel admin
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  !define MULTIUSER_EXECUTIONLEVEL Highest
  !include MultiUser.nsh
!endif

!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!else if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!endif

!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_BITMAP "${HEADERIMAGE}"
!endif

!if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_UNBITMAP "${UNINSTALLERHEADERIMAGE}"
!endif

!if "${UNINSTALLERICON}" != ""
  !define MUI_UNICON "${UNINSTALLERICON}"
!endif

!define MUI_BGCOLOR "FFFFFF"
!define MUI_TEXTCOLOR "0D0B10"

!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

!include "${REEL_NSIS_DIR}\look.nsh"
!insertmacro REEL_LOOK_FUNCTIONS ""
!insertmacro REEL_LOOK_FUNCTIONS "un."
!define MUI_CUSTOMFUNCTION_GUIINIT ReelLook
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.ReelLook

; OBS's licence, which nobody has to accept, is a line on the first page, not a page.
Var ReelPage
Var ReelInstalled
Var ReelDesktop
Var ReelDesktopBox
Var ReelOptionsLink
Var ReelOver
Var ReelThere

; The page before the install names its button: the start menu page between never shows.
Function ReelInstallButton
  GetDlgItem $0 $HWNDPARENT 1
  ${VersionCompare} "$ReelInstalled" "${VERSION}" $1
  ${If} $ReelInstalled != ""
  ${AndIf} $1 = 2
    SendMessage $0 ${WM_SETTEXT} 0 "STR:Update"
  ${Else}
    SendMessage $0 ${WM_SETTEXT} 0 "STR:$(^InstallBtn)"
  ${EndIf}
FunctionEnd

; The package on this PC by its OBS: 1 in $0 for the one with OBS.
Function ReelObsHere
  StrCpy $0 0
  ${If} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE}"
  ${OrIf} ${FileExists} "$INSTDIR\obs\bin\64bit\${REEL_OBS_EXE_BEFORE}"
    StrCpy $0 1
  ${EndIf}
FunctionEnd

!ifndef REEL_WITH_OBS
  Var ReelObsBox
  ; Over the package with OBS, Install waits for the box.
  Function ReelObsTicked
    ${NSD_GetState} $ReelObsBox $0
    GetDlgItem $1 $HWNDPARENT 1
    ${If} $0 = ${BST_CHECKED}
      EnableWindow $1 1
    ${Else}
      EnableWindow $1 0
    ${EndIf}
  FunctionEnd
!endif

!ifdef REEL_WITH_OBS
  Function ReelOpenLicence
    InitPluginsDir
    !if /FileExists "${REEL_NSIS_DIR}\..\obs\data\obs-studio\license\gplv2.txt"
      File "/oname=$PLUGINSDIR\OBS Studio's licence.txt" "${REEL_NSIS_DIR}\..\obs\data\obs-studio\license\gplv2.txt"
      ExecShell "open" "$PLUGINSDIR\OBS Studio's licence.txt"
    !else
      ExecShell "open" "https://www.gnu.org/licenses/old-licenses/gpl-2.0.html"
    !endif
  FunctionEnd
  Function ReelOpenSource
    ExecShell "open" "${REEL_OBS_SOURCE}"
  FunctionEnd
!endif

Function ReelOptions
  ShowWindow $ReelOptionsLink ${SW_HIDE}
  ShowWindow $ReelDesktopBox ${SW_SHOW}
  ${NSD_SetFocus} $ReelDesktopBox
FunctionEnd

Function ReelReady
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  ReadRegStr $ReelInstalled SHCTX "${UNINSTKEY}" "DisplayVersion"
  Call ReelObsHere
  StrCpy $ReelOver $0
  StrCpy $ReelThere "${REEL_NAME_PLAIN}"
  ${IfThen} $0 = 1 ${|} StrCpy $ReelThere "${REEL_NAME_WITH_OBS}" ${|}
  ${If} $ReelInstalled == ""
    !insertmacro MUI_HEADER_TEXT "Install ${REEL_TITLE}" "${REEL_WHAT}"
  ${ElseIf} $ReelThere != "${REEL_TITLE}"
    !insertmacro MUI_HEADER_TEXT "Switch to ${REEL_TITLE}" "${REEL_WHAT}"
  ${ElseIf} $ReelInstalled == "${VERSION}"
    !insertmacro MUI_HEADER_TEXT "Install ${REEL_TITLE} again" "${REEL_WHAT}"
  ${Else}
    ${VersionCompare} "$ReelInstalled" "${VERSION}" $0
    ${If} $0 = 1
      !insertmacro MUI_HEADER_TEXT "Replace ${REEL_TITLE}" "${REEL_WHAT}"
    ${Else}
      !insertmacro MUI_HEADER_TEXT "Update ${REEL_TITLE}" "${REEL_WHAT}"
    ${EndIf}
  ${EndIf}
  !insertmacro REEL_PAGE_BEGIN $ReelPage
  !insertmacro REEL_FOOTER ${SW_SHOW} ${SW_SHOW} ${SW_SHOW}
  !ifdef REEL_WITH_OBS
    StrCpy $ReelOver 0
  !endif
  ${If} $ReelOver = 1
    !ifndef REEL_WITH_OBS
      !insertmacro REEL_WORDS $0 0 0 504 50 ${REEL_INK} "${REEL_NAME_WITH_OBS} is on this PC, with the OBS that came with it. This installer is ${REEL_TITLE}, without OBS: it removes that OBS, its settings, and its copy for games that run as admin (Windows asks for admin rights once). reel then works with an OBS you install yourself."
      !insertmacro REEL_BOX $ReelObsBox 0 62 504 20 "Remove the OBS that came with reel, and its settings"
      ${NSD_OnClick} $ReelObsBox ReelObsTicked
      !insertmacro REEL_WORDS $0 20 84 484 32 ${REEL_QUIET} "Your clips stay where they are. To keep that OBS, cancel and run ${REEL_FILE_WITH_OBS}_${VERSION}_x64-setup.exe instead."
      StrCpy $6 116
    !endif
  ${Else}
    !insertmacro REEL_WORDS $0 0 0 92 18 ${REEL_QUIET} "Version"
    !ifdef REEL_WITH_OBS
      !insertmacro REEL_WORDS $0 100 0 404 18 ${REEL_INK} "${REEL_TITLE} ${VERSION}, with OBS Studio ${REEL_OBS_VERSION}"
    !else
      !insertmacro REEL_WORDS $0 100 0 404 18 ${REEL_INK} "${REEL_TITLE} ${VERSION}"
    !endif
    StrCpy $6 24
    ${If} $ReelInstalled != ""
      ${If} $ReelInstalled == "${VERSION}"
        StrCpy $1 "$ReelThere $ReelInstalled, the same version"
      ${Else}
        ${VersionCompare} "$ReelInstalled" "${VERSION}" $0
        ${If} $0 = 1
          StrCpy $1 "$ReelThere $ReelInstalled, a newer version"
        ${Else}
          StrCpy $1 "$ReelThere $ReelInstalled, which this updates"
        ${EndIf}
      ${EndIf}
      !insertmacro REEL_WORDS $0 0 $6 92 18 ${REEL_QUIET} "On this PC"
      !insertmacro REEL_WORDS $0 100 $6 404 18 ${REEL_INK} "$1"
      IntOp $6 $6 + 24
    ${EndIf}
    IntOp $7 ${REEL_FOR_LINES} * 18
    !insertmacro REEL_WORDS $0 0 $6 92 18 ${REEL_QUIET} "For"
    !insertmacro REEL_WORDS $0 100 $6 404 $7 ${REEL_INK} "${REEL_FOR_YOU}"
    IntOp $6 $6 + $7
    IntOp $6 $6 + 6
    !insertmacro REEL_WORDS $0 0 $6 92 18 ${REEL_QUIET} "Folder"
    !insertmacro REEL_WORDS $0 100 $6 404 18 ${REEL_INK} "$INSTDIR"
    IntOp $6 $6 + 24
    !ifdef REEL_WITH_OBS
      !insertmacro REEL_WORDS $0 0 $6 92 18 ${REEL_QUIET} "OBS Studio"
      !insertmacro REEL_WORDS_AND_LINKS 100 $6 "free software under the GNU GPL:" "its licence" ReelOpenLicence "its source code" ReelOpenSource
      IntOp $6 $6 + 24
    !endif
  ${EndIf}
  IntOp $6 $6 + 12
  !insertmacro REEL_LINK $ReelOptionsLink 0 $6 120 18 "&Options" ReelOptions
  IntOp $6 $6 - 2
  !insertmacro REEL_BOX $ReelDesktopBox 0 $6 504 20 "$(createDesktop)"
  ${If} $ReelDesktop == ""
    StrCpy $ReelDesktop 1
  ${EndIf}
  ${NSD_SetState} $ReelDesktopBox $ReelDesktop
  ShowWindow $ReelDesktopBox ${SW_HIDE}
  Call ReelInstallButton
  !ifndef REEL_WITH_OBS
    ${If} $ReelOver = 1
      Call ReelObsTicked
      ${NSD_SetFocus} $ReelObsBox
    ${EndIf}
  !endif
  nsDialogs::Show
FunctionEnd

Function ReelReadyLeave
  ${NSD_GetState} $ReelDesktopBox $ReelDesktop
  !ifndef REEL_WITH_OBS
    ${If} $ReelOver = 1
      ${NSD_GetState} $ReelObsBox $0
      ${IfThen} $0 <> ${BST_CHECKED} ${|} Abort ${|}
      StrCpy $ReelRemoveObs 1
    ${EndIf}
  !endif
FunctionEnd
Page custom ReelReady ReelReadyLeave

!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

!if "${INSTALLMODE}" == "both"
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MULTIUSER_PAGE_INSTALLMODE
!endif

Var AppStartMenuFolder
!if "${STARTMENUFOLDER}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !define MUI_STARTMENUPAGE_DEFAULTFOLDER "${STARTMENUFOLDER}"
!else
  !define MUI_PAGE_CUSTOMFUNCTION_PRE Skip
!endif
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

!define MUI_PAGE_HEADER_TEXT "Installing ${REEL_TITLE}"
!define MUI_PAGE_HEADER_SUBTEXT " "
!define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "Installed"
!define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT " "
!define MUI_INSTFILESPAGE_ABORTHEADER_TEXT "Not installed"
!define MUI_INSTFILESPAGE_ABORTHEADER_SUBTEXT " "
!define MUI_PAGE_CUSTOMFUNCTION_SHOW ReelProgressShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE ReelProgressLeave
!insertmacro MUI_PAGE_INSTFILES
!undef MUI_INSTFILESPAGE_ABORTHEADER_TEXT
!undef MUI_INSTFILESPAGE_ABORTHEADER_SUBTEXT

Function ReelProgressLeave
  IfAbort 0 reel_progress_done
    ${If} $PassiveMode = 1
    ${AndIf} $UpdateMode = 1
      Call ReelUpdateStopped
      Quit
    ${EndIf}
    Call ReelProgressStopped
  reel_progress_done:
FunctionEnd

Function ReelDone
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  !insertmacro REEL_PAGE_BEGIN $ReelPage
  ${If} $ReelInstalled != ""
  ${AndIf} $ReelInstalled != "${VERSION}"
    !insertmacro REEL_MIDDLE $ReelPage "${REEL_TITLE} is updated to ${VERSION}" "reel runs in the tray, next to the clock, with your settings as they were."
  ${Else}
    !insertmacro REEL_MIDDLE $ReelPage "${REEL_TITLE} is installed" "reel runs in the tray, next to the clock. ${REEL_NEXT}"
  ${EndIf}
  !insertmacro REEL_FOOTER ${SW_SHOW} ${SW_SHOW} ${SW_SHOW}
  !insertmacro REEL_BUTTON 1 "Open reel"
  !insertmacro REEL_BUTTON 2 "Close"
  GetDlgItem $0 $HWNDPARENT 2
  EnableWindow $0 1
  GetDlgItem $0 $HWNDPARENT 1
  ${NSD_SetFocus} $0
  nsDialogs::Show
FunctionEnd
Function ReelDoneLeave
  Call RunMainBinary
FunctionEnd
Page custom ReelDone ReelDoneLeave

Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
FunctionEnd

; The uninstaller's first page replaces Tauri's confirm page; a silent or passive uninstall keeps everything.
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
Var ReelClipsBox
Var ReelClipsSize
Var ReelClipsKept
Var ReelGone
UninstPage custom un.ReelChoices un.ReelChoicesLeave

; The clips folder as reel-stop.exe decides it: offered with its size when it is a plain folder
; of its own, else kept with why.
Var ReelClipsWhy
Function un.ReelClipsDir
  StrCpy $ReelClipsDir ""
  StrCpy $ReelClipsKept ""
  InitPluginsDir
  File "/oname=$PLUGINSDIR\reel-stop.exe" "${REEL_NSIS_DIR}\reel-stop.exe"
  nsExec::ExecToStack `"$PLUGINSDIR\reel-stop.exe" --clips "$PLUGINSDIR\reel-clips.txt" "$INSTDIR" ${REEL_PLACES}`
  Pop $0
  Pop $1
  ClearErrors
  FileOpen $R0 "$PLUGINSDIR\reel-clips.txt" r
  ${IfThen} ${Errors} ${|} Return ${|}
  FileReadUTF16LE $R0 $R1
  FileClose $R0
  ${WordFind} "$R1" "|" "+1" $R2
  ${WordFind} "$R1" "|" "+2" $R3
  ${WordFind} "$R1" "|" "+3" $R4
  ${If} $R2 == "offer"
    StrCpy $ReelClipsDir $R3
    StrCpy $ReelClipsSize $R4
  ${ElseIf} $R2 == "keep"
    StrCpy $ReelClipsKept $R3
    StrCpy $ReelClipsWhy $R4
  ${EndIf}
FunctionEnd

Function un.ReelChoices
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  !insertmacro MUI_HEADER_TEXT "Remove reel" "${REEL_GOES}"
  !insertmacro REEL_PAGE_BEGIN $ReelPage
  !insertmacro REEL_FOOTER ${SW_SHOW} ${SW_SHOW} ${SW_SHOW}
  !insertmacro REEL_BOX $DeleteAppDataCheckbox 0 0 504 20 "$(deleteAppData)"
  !insertmacro REEL_WORDS $0 20 22 484 18 ${REEL_QUIET} "Its settings, its log and the pairing with your server: next time reel starts fresh."
  Call un.ReelClipsDir
  StrCpy $ReelClipsBox ""
  ${If} $ReelClipsDir != ""
    ; a long path takes two lines
    StrCpy $1 "Also delete the clips folder $ReelClipsDir ($ReelClipsSize)"
    StrLen $2 $1
    ${If} $2 > 70
      !insertmacro REEL_BOX $ReelClipsBox 0 54 504 36 "$1"
      StrCpy $2 92
    ${Else}
      !insertmacro REEL_BOX $ReelClipsBox 0 54 504 20 "$1"
      StrCpy $2 76
    ${EndIf}
    ${NSD_OnClick} $ReelClipsBox un.ReelClipsClicked
    !insertmacro REEL_WORDS $0 20 $2 484 18 ${REEL_QUIET} "Deletes every clip in it for good. You are asked to confirm."
  ${ElseIf} $ReelClipsKept != ""
    !insertmacro REEL_WORDS $0 0 54 504 36 ${REEL_QUIET} "Your clips folder, $ReelClipsKept, stays: $ReelClipsWhy."
  ${EndIf}
  !insertmacro REEL_BUTTON 1 "$(^UninstallBtn)"
  ${NSD_SetFocus} $DeleteAppDataCheckbox
  nsDialogs::Show
FunctionEnd

Function un.ReelClipsClicked
  Pop $0
  ${NSD_GetState} $ReelClipsBox $1
  ${If} $1 = ${BST_CHECKED}
    MessageBox MB_YESNO|MB_ICONEXCLAMATION|MB_DEFBUTTON2 "Delete $ReelClipsDir and everything in it ($ReelClipsSize)?$\r$\n$\r$\nThe clips in it are gone for good; the ones already on your server stay there." IDYES reel_clips_yes
    ${NSD_Uncheck} $ReelClipsBox
    reel_clips_yes:
  ${EndIf}
FunctionEnd

Function un.ReelChoicesLeave
  ${NSD_GetState} $DeleteAppDataCheckbox $DeleteAppDataCheckboxState
  StrCpy $ReelRemoveData $DeleteAppDataCheckboxState
  StrCpy $ReelDeleteClips 0
  ${If} $ReelClipsBox != ""
    ${NSD_GetState} $ReelClipsBox $ReelDeleteClips
  ${EndIf}
  ${If} $ReelDeleteClips = 1
    StrCpy $ReelGone "Your clips folder went with it."
  ${EndIf}
FunctionEnd

!define MUI_PAGE_HEADER_TEXT "Removing reel"
!define MUI_PAGE_HEADER_SUBTEXT " "
!define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "Removed"
!define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT " "
!define MUI_INSTFILESPAGE_ABORTHEADER_TEXT "Not removed"
!define MUI_INSTFILESPAGE_ABORTHEADER_SUBTEXT " "
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ReelProgressShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ReelProgressLeave
!insertmacro MUI_UNPAGE_INSTFILES

Function un.ReelProgressLeave
  IfAbort 0 reel_progress_done
    Call un.ReelProgressStopped
  reel_progress_done:
FunctionEnd

Function un.ReelEnd
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
  !insertmacro REEL_PAGE_BEGIN $ReelPage
  !insertmacro REEL_MIDDLE $ReelPage "reel is removed" "$ReelGone"
  !insertmacro REEL_ONE_BUTTON "Close"
  nsDialogs::Show
FunctionEnd
UninstPage custom un.ReelEnd

{{#each languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}
!insertmacro MUI_RESERVEFILE_LANGDLL
{{#each language_files}}
  !include "{{this}}"
{{/each}}

Function .onInit
  !insertmacro REEL_TIMES_INIT
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  ; An app of 1.9 or before runs its update silently (/S /UPDATE /R): it gets the small window too.
  ${If} ${Silent}
  ${AndIf} $UpdateMode = 1
    ${GetOptions} $CMDLINE "/R" $0
    ${IfNot} ${Errors}
      SetSilent normal
      StrCpy $PassiveMode 1
    ${EndIf}
  ${EndIf}

  ; GetOptions takes a switch by its start: none of reel's may start as Tauri's /R or /P do
  !ifndef REEL_WITH_OBS
    ${GetOptions} $CMDLINE "/WITHOUTOBS" $ReelRemoveObs
    ${IfNot} ${Errors}
      StrCpy $ReelRemoveObs 1
    ${EndIf}
  !endif

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext

  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    !endif

    Call RestorePreviousInstallLocation
  ${EndIf}

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd

Section EarlyChecks
  !if "${ALLOWDOWNGRADES}" == "false"
  ${If} ${Silent}
    ${If} $R0 = -1
      System::Call 'kernel32::AttachConsole(i -1)i.r0'
      ${If} $0 <> 0
        System::Call 'kernel32::GetStdHandle(i -11)i.r0'
        System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)'
        FileWrite $0 "$(silentDowngrades)"
      ${EndIf}
      Abort
    ${EndIf}
  ${EndIf}
  !endif

SectionEnd

Section WebView2
  SetDetailsPrint none
  ${If} ${RunningX64}
    ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $4 == ""
    ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}

  ${If} $4 == ""
    ${If} $UpdateMode <> 1
      !if "${INSTALLWEBVIEW2MODE}" == "downloadBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        !insertmacro REEL_SAY "$(webview2Downloading)"
        NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Pop $0
        ${If} $0 == "success"
          !insertmacro REEL_SAY "$(webview2DownloadSuccess)"
        ${Else}
          !insertmacro REEL_SAY "$(webview2DownloadError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "embedBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2BOOTSTRAPPERPATH}"
        !insertmacro REEL_SAY "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "offlineInstaller"
        Delete "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"
        !insertmacro REEL_SAY "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        Goto install_webview2
      !endif

      Goto webview2_done

      install_webview2:
        !insertmacro REEL_SAY "$(installingWebview2)"
        ExecWait "$6 ${WEBVIEW2INSTALLERARGS} /install" $1
        ${If} $1 = 0
          !insertmacro REEL_SAY "$(webview2InstallSuccess)"
        ${Else}
          !insertmacro REEL_SAY "$(webview2InstallError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
      webview2_done:
    ${EndIf}
  ${Else}
    !if "${MINIMUMWEBVIEW2VERSION}" != ""
      ${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4" $R0
      ${If} $R0 = 1
        update_webview:
          !insertmacro REEL_SAY "$(installingWebview2)"
          ${If} ${RunningX64}
            ReadRegStr $R1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate" "path"
          ${Else}
            ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 == ""
            ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 != ""
            ExecWait `"$R1" /install appguid=${WEBVIEW2APPGUID}&needsadmin=true` $1
            ${If} $1 = 0
              !insertmacro REEL_SAY "$(webview2InstallSuccess)"
            ${Else}
              MessageBox MB_ICONEXCLAMATION|MB_ABORTRETRYIGNORE "$(webview2InstallError)" IDIGNORE ignore IDRETRY update_webview
              Quit
              ignore:
            ${EndIf}
          ${EndIf}
      ${EndIf}
    !endif
  ${EndIf}
SectionEnd

Section Install
  ; the details show reel's words (REEL_SAY), never paths
  SetDetailsPrint none
  SetOutPath $INSTDIR
  !insertmacro REEL_TIMES_BEGIN

  !ifmacrodef NSIS_HOOK_PREINSTALL
    !insertmacro NSIS_HOOK_PREINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro REEL_SAY "Installing ${REEL_TITLE}"

  File "${MAINBINARYSRCPATH}"

  {{#each resources_dirs}}
    CreateDirectory "$INSTDIR\\{{this}}"
  {{/each}}
  {{#each resources}}
    !insertmacro REEL_RESOURCE "{{this.[1]}}" "{{no-escape @key}}"
  {{/each}}

  {{#each binaries}}
    File /a "/oname={{this}}" "{{no-escape @key}}"
  {{/each}}

  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
       !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
    {{/each}}
  {{/each}}

  {{#each deep_link_protocols as |protocol| ~}}
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "URL Protocol" ""
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
    WriteRegStr SHCTX "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
  {{/each}}

  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

  !if "${INSTALLMODE}" == "both"
    WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
  !endif

  ReadRegStr $OldMainBinaryName SHCTX "${UNINSTKEY}" "MainBinaryName"
  ${If} $OldMainBinaryName != ""
  ${AndIf} $OldMainBinaryName != "${MAINBINARYNAME}.exe"
    Delete "$INSTDIR\$OldMainBinaryName"
  ${EndIf}

  WriteRegStr SHCTX "${UNINSTKEY}" "MainBinaryName" "${MAINBINARYNAME}.exe"

  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"

  ${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0" $0 $1 $2
  IntOp $0 $0 + ${ESTIMATEDSIZE}
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "$0"

  !if "${HOMEPAGE}" != ""
    WriteRegStr SHCTX "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"
  !endif

  ${If} $UpdateMode <> 1
    !insertmacro REEL_SAY "Adding reel to the Start menu"
  ${EndIf}
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    Call CreateOrUpdateStartMenuShortcut
  !insertmacro MUI_STARTMENU_WRITE_END

  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
  ${OrIf} $ReelDesktop = 1
    Call CreateOrUpdateDesktopShortcut
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTINSTALL
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif

  ; on to the end's page, or closed in passive mode
  SetAutoClose true
  ; NSIS's own Completed, at the end
  SetDetailsPrint both
SectionEnd

Function .onInstSuccess
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    Call ReelStartAgain
  ${EndIf}
FunctionEnd

; reel again after an update the app asked for with /R, unless its note (updating.json) says
; the update installs as the app quits.
Function ReelStartAgain
  ${GetOptions} $CMDLINE "/R" $R0
  ${IfNot} ${Errors}
    StrCpy $R1 ""
    ClearErrors
    FileOpen $R2 "$APPDATA\reel\updating.json" r
    ${IfNot} ${Errors}
      FileRead $R2 $R3
      FileClose $R2
      ${StrLoc} $R1 $R3 '"quit":true' ">"
    ${EndIf}
    ${If} $R1 == ""
      ${GetOptions} $CMDLINE "/ARGS" $R0
      !insertmacro REEL_TIME "reel starts again"
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "$R0"
    ${EndIf}
  ${EndIf}
  !insertmacro REEL_TIMES_END
FunctionEnd

; An update that stopped: why, for the app to say at its start, and the reel that was there
; started again. An app of 1.9 or before reads no note, so the installer says it.
Function ReelUpdateStopped
  FindWindow $0 "#32770" "" $HWNDPARENT
  GetDlgItem $1 $0 1006
  System::Call 'user32::GetWindowTextW(p r1, w .r2, i ${NSIS_MAX_STRLEN})'
  ${Do}
    StrCpy $3 $2 1 -1
    ${If} $3 == "$\r"
    ${OrIf} $3 == "$\n"
    ${OrIf} $3 == " "
      StrCpy $2 $2 -1
    ${Else}
      ${ExitDo}
    ${EndIf}
  ${Loop}
  CreateDirectory "$APPDATA\reel"
  FileOpen $3 "$APPDATA\reel\update-failed.txt" w
  FileWriteUTF16LE /BOM $3 "$2"
  FileClose $3
  !insertmacro REEL_TIME "stopped: $2"
  ${IfNot} ${FileExists} "$APPDATA\reel\updating.json"
    MessageBox MB_OK|MB_ICONEXCLAMATION "${REEL_TITLE} ${VERSION} did not install:$\r$\n$\r$\n$2" /SD IDOK
  ${EndIf}
  Call ReelStartAgain
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

  !insertmacro MUI_UNGETLANGUAGE

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  StrCpy $ReelGone "Your clips are still where they were."
FunctionEnd

Section Uninstall
  SetDetailsPrint none

  !ifmacrodef NSIS_HOOK_PREUNINSTALL
    !insertmacro NSIS_HOOK_PREUNINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro REEL_SAY "Removing reel"

  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  {{#each resources}}
    Delete "$INSTDIR\\{{this.[1]}}"
  {{/each}}

  {{#each binaries}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}

  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    ${If} $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
    ${EndIf}
  {{/each}}

  Delete "$INSTDIR\uninstall.exe"

  {{#each resources_ancestors}}
  RMDir /REBOOTOK "$INSTDIR\\{{this}}"
  {{/each}}
  RMDir "$INSTDIR"

  ${If} $UpdateMode <> 1
    !insertmacro DeleteAppUserModelId

    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}

    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$DESKTOP\${PRODUCTNAME}.lnk"
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}

  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}

  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"

    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"

    SetShellVarContext current
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTUNINSTALL
    !insertmacro NSIS_HOOK_POSTUNINSTALL
  !endif

  ; on to the end's page, or closed in passive mode or an update
  SetAutoClose true
  SetDetailsPrint both
SectionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd

Function Skip
  Abort
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  StrCpy $R0 0

  !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  ${If} $R0 = 1
    Return
  ${EndIf}

  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  !if "${STARTMENUFOLDER}" != ""
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
FunctionEnd

Function CreateOrUpdateDesktopShortcut
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Return
  ${EndIf}

  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
FunctionEnd
