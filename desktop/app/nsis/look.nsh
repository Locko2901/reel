; The installers' light window, and the small window of an update (passive mode). Sizes are
; pixels at 100 % of Windows' scaling, multiplied by the window's scale.

!define REEL_INK "0D0B10"
!define REEL_QUIET "5C5763"
!define REEL_LINK "AB1F9B"
!define REEL_PAGE "FFFFFF"
!define REEL_BAND "F6F5F8"
!define REEL_RULE "E6E4EA"
!define REEL_BAR 0xBD54DC
!define REEL_TRACK 0xEFE9EB

!define REEL_W 576
!define REEL_H 440
!define REEL_M 36
!define REEL_FOOT 64
!define REEL_BODY_Y 176
!define REEL_SMALL_W 440
!define REEL_SMALL_H 166
!define REEL_SMALL_M 28
!define REEL_TITLE_Y 82
!define REEL_LEAD_Y 118

Var ReelDpi
Var ReelSmall
Var ReelTitleFont
Var ReelLinkFont
Var ReelArt

!macro REEL_PX OUT VALUE
  IntOp ${OUT} ${VALUE} * $ReelDpi
  IntOp ${OUT} ${OUT} / 96
!macroend

; A control to x, y, width and height at 100 %.
!macro REEL_PLACE HWND X Y W H
  !insertmacro REEL_PX $R5 ${X}
  !insertmacro REEL_PX $R6 ${Y}
  !insertmacro REEL_PX $R7 ${W}
  !insertmacro REEL_PX $R8 ${H}
  System::Call 'user32::SetWindowPos(p ${HWND}, p 0, i R5, i R6, i R7, i R8, i 0x14)'
!macroend

!macro REEL_ITEM OUT ID
  GetDlgItem ${OUT} $HWNDPARENT ${ID}
!macroend

!macro REEL_SHOW ID SHOW
  GetDlgItem $R9 $HWNDPARENT ${ID}
  ShowWindow $R9 ${SHOW}
!macroend

; The footer's band and its rule, and which buttons it holds: Back never shows.
!macro REEL_FOOTER SHOW NEXT CANCEL
  !insertmacro REEL_SHOW 1034 ${SHOW}
  !insertmacro REEL_SHOW 1256 ${SHOW}
  !insertmacro REEL_SHOW 3 ${SW_HIDE}
  !insertmacro REEL_SHOW 1 ${NEXT}
  !insertmacro REEL_SHOW 2 ${CANCEL}
!macroend

; The page's one button where Cancel stands, at the right.
!macro REEL_ONE_BUTTON TEXT
  !insertmacro REEL_FOOTER ${SW_SHOW} ${SW_SHOW} ${SW_HIDE}
  !insertmacro REEL_BUTTON 1 "${TEXT}"
  GetDlgItem $R4 $HWNDPARENT 1
  IntOp $R3 ${REEL_H} - ${REEL_FOOT}
  IntOp $R3 $R3 + 17
  IntOp $R2 ${REEL_W} - ${REEL_M}
  IntOp $R2 $R2 - 100
  !insertmacro REEL_PLACE $R4 $R2 $R3 100 30
!macroend

!macro REEL_BUTTON ID TEXT
  GetDlgItem $R9 $HWNDPARENT ${ID}
  SendMessage $R9 ${WM_SETTEXT} 0 "STR:${TEXT}"
!macroend

; A line of words at 100 % pixels, in reel's ink or its quiet grey.
!macro REEL_WORDS OUT X Y W H COLOR TEXT
  !insertmacro REEL_PX $R1 ${X}
  !insertmacro REEL_PX $R2 ${Y}
  !insertmacro REEL_PX $R3 ${W}
  !insertmacro REEL_PX $R4 ${H}
  ${NSD_CreateLabel} $R1 $R2 $R3 $R4 "${TEXT}"
  Pop ${OUT}
  SetCtlColors ${OUT} ${COLOR} ${REEL_PAGE}
!macroend

!macro REEL_LINK OUT X Y W H TEXT ONCLICK
  !insertmacro REEL_PX $R1 ${X}
  !insertmacro REEL_PX $R2 ${Y}
  !insertmacro REEL_PX $R3 ${W}
  !insertmacro REEL_PX $R4 ${H}
  ${NSD_CreateLink} $R1 $R2 $R3 $R4 "${TEXT}"
  Pop ${OUT}
  SetCtlColors ${OUT} ${REEL_LINK} transparent
  SendMessage ${OUT} ${WM_SETFONT} $ReelLinkFont 1
  ${NSD_OnClick} ${OUT} ${ONCLICK}
!macroend

; The width in pixels of words in a font, for words and links set on one line.
!macro REEL_TEXT_WIDTH OUT FONT TEXT
  System::Call 'user32::GetDC(p 0) p .R9'
  System::Call 'gdi32::SelectObject(p R9, p ${FONT}) p .R8'
  System::Call '*(i, i) p .R7'
  StrLen $R6 "${TEXT}"
  System::Call 'gdi32::GetTextExtentPoint32W(p R9, w "${TEXT}", i R6, p R7)'
  System::Call '*$R7(i .R6, i)'
  System::Free $R7
  System::Call 'gdi32::SelectObject(p R9, p R8)'
  System::Call 'user32::ReleaseDC(p 0, p R9)'
  StrCpy ${OUT} $R6
!macroend

; Words, then links, on one line from x at 100 % pixels: each as wide as it is.
!macro REEL_WORDS_AND_LINKS X Y WORDS LINK1 ONCLICK1 LINK2 ONCLICK2
  !insertmacro REEL_PX $R1 ${X}
  !insertmacro REEL_PX $R2 ${Y}
  !insertmacro REEL_PX $R4 18
  !insertmacro REEL_PX $R0 8
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $R3
  !insertmacro REEL_TEXT_WIDTH $R3 $R3 "${WORDS}"
  IntOp $R3 $R3 + 2
  ${NSD_CreateLabel} $R1 $R2 $R3 $R4 "${WORDS}"
  Pop $R5
  SetCtlColors $R5 ${REEL_INK} ${REEL_PAGE}
  IntOp $R1 $R1 + $R3
  IntOp $R1 $R1 + $R0
  !insertmacro REEL_TEXT_WIDTH $R3 $ReelLinkFont "${LINK1}"
  IntOp $R3 $R3 + 2
  ${NSD_CreateLink} $R1 $R2 $R3 $R4 "${LINK1}"
  Pop $R5
  SetCtlColors $R5 ${REEL_LINK} transparent
  SendMessage $R5 ${WM_SETFONT} $ReelLinkFont 1
  ${NSD_OnClick} $R5 ${ONCLICK1}
  IntOp $R1 $R1 + $R3
  IntOp $R1 $R1 + $R0
  !insertmacro REEL_TEXT_WIDTH $R3 $ReelLinkFont "${LINK2}"
  IntOp $R3 $R3 + 2
  ${NSD_CreateLink} $R1 $R2 $R3 $R4 "${LINK2}"
  Pop $R5
  SetCtlColors $R5 ${REEL_LINK} transparent
  SendMessage $R5 ${WM_SETFONT} $ReelLinkFont 1
  ${NSD_OnClick} $R5 ${ONCLICK2}
!macroend

!macro REEL_BOX OUT X Y W H TEXT
  !insertmacro REEL_PX $R1 ${X}
  !insertmacro REEL_PX $R2 ${Y}
  !insertmacro REEL_PX $R3 ${W}
  !insertmacro REEL_PX $R4 ${H}
  ${NSD_CreateCheckbox} $R1 $R2 $R3 $R4 "${TEXT}"
  Pop ${OUT}
  SetCtlColors ${OUT} ${REEL_INK} ${REEL_PAGE}
!macroend

; A page's body in the place the window keeps for it, on the window's white.
!macro REEL_PAGE_BEGIN OUT
  !insertmacro REEL_SHOW 1037 ${SW_SHOW}
  !insertmacro REEL_SHOW 1038 ${SW_SHOW}
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $R8
  SendMessage $R9 ${WM_SETFONT} $R8 1
  IntOp $R3 ${REEL_W} - ${REEL_M}
  IntOp $R3 $R3 - ${REEL_M}
  !insertmacro REEL_PLACE $R9 ${REEL_M} ${REEL_LEAD_Y} $R3 40
  GetDlgItem $R9 $HWNDPARENT 1037
  !insertmacro REEL_PLACE $R9 ${REEL_M} ${REEL_TITLE_Y} $R3 32
  nsDialogs::Create 1018
  Pop ${OUT}
  SetCtlColors ${OUT} "" ${REEL_PAGE}
!macroend

; A title and its line alone, in the middle between the wordmark and the band. MUI's own two
; stay hidden, as the page would paint over them.
!macro REEL_MIDDLE PAGE TITLE LEAD
  !insertmacro REEL_SHOW 1037 ${SW_HIDE}
  !insertmacro REEL_SHOW 1038 ${SW_HIDE}
  IntOp $R3 ${REEL_W} - ${REEL_M}
  IntOp $R3 $R3 - ${REEL_M}
  IntOp $R2 ${REEL_H} - ${REEL_FOOT}
  IntOp $R2 $R2 - 62
  !insertmacro REEL_PLACE ${PAGE} ${REEL_M} 62 $R3 $R2
  IntOp $R2 $R2 - 76
  IntOp $R2 $R2 / 2
  IntOp $R2 $R2 - 12
  StrCpy $R0 $R2
  !insertmacro REEL_WORDS $R2 0 $R0 504 32 ${REEL_INK} "${TITLE}"
  SendMessage $R2 ${WM_SETFONT} $ReelTitleFont 1
  IntOp $R0 $R0 + 36
  !insertmacro REEL_WORDS $R2 0 $R0 504 40 ${REEL_INK} "${LEAD}"
!macroend

; The art drawn nearest the window's scale, at its own size.
!macro REEL_ART_AT SCALE
  File "/oname=$PLUGINSDIR\reel-wordmark.bmp" "${REEL_NSIS_DIR}\wordmark${SCALE}.bmp"
  File "/oname=$PLUGINSDIR\reel-wordmark-small.bmp" "${REEL_NSIS_DIR}\wordmark-small${SCALE}.bmp"
!macroend

!macro REEL_LOOK_FUNCTIONS UN

  Function ${UN}ReelLook
    InitPluginsDir
    System::Call 'user32::GetDpiForWindow(p $HWNDPARENT) i .r0'
    ${If} $0 == "error"
    ${OrIf} $0 = 0
      System::Call 'user32::GetDC(p 0) p .r1'
      System::Call 'gdi32::GetDeviceCaps(p r1, i 90) i .r0'
      System::Call 'user32::ReleaseDC(p 0, p r1)'
    ${EndIf}
    StrCpy $ReelDpi $0
    ${If} $0 >= 180
      !insertmacro REEL_ART_AT "-200"
    ${ElseIf} $0 >= 156
      !insertmacro REEL_ART_AT "-175"
    ${ElseIf} $0 >= 132
      !insertmacro REEL_ART_AT "-150"
    ${ElseIf} $0 >= 108
      !insertmacro REEL_ART_AT "-125"
    ${Else}
      !insertmacro REEL_ART_AT ""
    ${EndIf}
    StrCpy $ReelSmall $PassiveMode
    ${If} $ReelSmall = 1
      StrCpy $1 ${REEL_SMALL_W}
      StrCpy $2 ${REEL_SMALL_H}
    ${Else}
      StrCpy $1 ${REEL_W}
      StrCpy $2 ${REEL_H}
    ${EndIf}
    ; the window at that size, its frame kept, in the middle of its screen's work area
    !insertmacro REEL_PX $1 $1
    !insertmacro REEL_PX $2 $2
    System::Call '*(i, i, i, i) p .r3'
    System::Call 'user32::GetWindowRect(p $HWNDPARENT, p r3)'
    System::Call '*$3(i .r4, i .r5, i .r6, i .r7)'
    IntOp $8 $6 - $4
    IntOp $9 $7 - $5
    System::Call 'user32::GetClientRect(p $HWNDPARENT, p r3)'
    System::Call '*$3(i, i, i .r6, i .r7)'
    System::Free $3
    IntOp $8 $8 - $6
    IntOp $9 $9 - $7
    IntOp $1 $1 + $8
    IntOp $2 $2 + $9
    System::Call '*(i 40, i, i, i, i, i, i, i, i, i) p .r3'
    System::Call 'user32::MonitorFromWindow(p $HWNDPARENT, i 2) p .r4'
    System::Call 'user32::GetMonitorInfoW(p r4, p r3)'
    System::Call '*$3(i, i, i, i, i, i .r4, i .r5, i .r6, i .r7)'
    System::Free $3
    IntOp $6 $6 - $4
    IntOp $6 $6 - $1
    IntOp $6 $6 / 2
    IntOp $6 $6 + $4
    IntOp $7 $7 - $5
    IntOp $7 $7 - $2
    IntOp $7 $7 / 2
    IntOp $7 $7 + $5
    System::Call 'user32::SetWindowPos(p $HWNDPARENT, p 0, i r6, i r7, i r1, i r2, i 0x14)'

    SetCtlColors $HWNDPARENT "" ${REEL_PAGE}
    ; MUI's header, lines, icon and branding go; its title and line stay as the page's.
    ${ForEach} $0 1028 1046 + 1
      ${If} $0 = 1028
      ${OrIf} $0 = 1035
      ${OrIf} $0 = 1036
      ${OrIf} $0 = 1039
      ${OrIf} $0 = 1044
      ${OrIf} $0 = 1045
      ${OrIf} $0 = 1046
        !insertmacro REEL_SHOW $0 ${SW_HIDE}
      ${EndIf}
    ${Next}
    CreateFont $ReelLinkFont "Segoe UI" 9 400 /UNDERLINE
    ${If} $ReelSmall = 1
      CreateFont $ReelTitleFont "Segoe UI Semibold" 12 600
      StrCpy $ReelArt "$PLUGINSDIR\reel-wordmark-small.bmp"
      IntOp $3 ${REEL_SMALL_M} - 2
      IntOp $4 ${REEL_SMALL_M} - 4
    ${Else}
      CreateFont $ReelTitleFont "Segoe UI Semibold" 15 600
      StrCpy $ReelArt "$PLUGINSDIR\reel-wordmark.bmp"
      IntOp $3 ${REEL_M} - 2
      StrCpy $4 30
    ${EndIf}
    !insertmacro REEL_PX $3 $3
    !insertmacro REEL_PX $4 $4
    System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w "", i 0x5000000E, i r3, i r4, i 0, i 0, p $HWNDPARENT, p 1300, p 0, p 0) p .r5'
    System::Call 'user32::LoadImageW(p 0, w "$ReelArt", i 0, i 0, i 0, i 0x10) p .r6'
    SendMessage $5 0x172 0 $6

    !insertmacro REEL_ITEM $0 1037
    SendMessage $0 ${WM_SETFONT} $ReelTitleFont 1
    SetCtlColors $0 ${REEL_INK} ${REEL_PAGE}
    !insertmacro REEL_ITEM $1 1038
    SetCtlColors $1 ${REEL_INK} ${REEL_PAGE}
    !insertmacro REEL_ITEM $2 1018
    ${If} $ReelSmall = 1
      IntOp $3 ${REEL_SMALL_W} - ${REEL_SMALL_M}
      IntOp $3 $3 - ${REEL_SMALL_M}
      IntOp $6 ${REEL_SMALL_M} + 32
      !insertmacro REEL_PLACE $0 ${REEL_SMALL_M} $6 $3 26
      ShowWindow $1 ${SW_HIDE}
      IntOp $6 $6 + 34
      IntOp $7 ${REEL_SMALL_H} - $6
      !insertmacro REEL_PLACE $2 ${REEL_SMALL_M} $6 $3 $7
      !insertmacro REEL_FOOTER ${SW_HIDE} ${SW_HIDE} ${SW_HIDE}
    ${Else}
      IntOp $3 ${REEL_W} - ${REEL_M}
      IntOp $3 $3 - ${REEL_M}
      !insertmacro REEL_PLACE $0 ${REEL_M} ${REEL_TITLE_Y} $3 32
      !insertmacro REEL_PLACE $1 ${REEL_M} ${REEL_LEAD_Y} $3 40
      IntOp $4 ${REEL_H} - ${REEL_FOOT}
      IntOp $5 $4 - ${REEL_BODY_Y}
      IntOp $5 $5 - 12
      !insertmacro REEL_PLACE $2 ${REEL_M} ${REEL_BODY_Y} $3 $5
      ; the band under the buttons, with a rule on top
      !insertmacro REEL_ITEM $5 1034
      SetCtlColors $5 "" ${REEL_BAND}
      SendMessage $5 ${WM_SETTEXT} 0 "STR:"
      !insertmacro REEL_PLACE $5 0 $4 ${REEL_W} ${REEL_FOOT}
      System::Call 'user32::GetWindowLongW(p r5, i -16) i .r6'
      IntOp $6 $6 | 0x04000000
      System::Call 'user32::SetWindowLongW(p r5, i -16, i r6)'
      System::Call 'user32::SetWindowPos(p r5, p 1, i 0, i 0, i 0, i 0, i 0x13)'
      !insertmacro REEL_ITEM $5 1256
      SetCtlColors $5 "" ${REEL_RULE}
      SendMessage $5 ${WM_SETTEXT} 0 "STR:"
      !insertmacro REEL_PX $6 $4
      !insertmacro REEL_PX $7 ${REEL_W}
      !insertmacro REEL_PX $8 1
      ${IfThen} $8 < 1 ${|} StrCpy $8 1 ${|}
      System::Call 'user32::SetWindowPos(p r5, p 0, i 0, i r6, i r7, i r8, i 0x14)'
      ; the buttons, right in the band: the page's action, then Cancel
      IntOp $6 $4 + 17
      IntOp $7 ${REEL_W} - ${REEL_M}
      IntOp $7 $7 - 100
      !insertmacro REEL_ITEM $5 2
      !insertmacro REEL_PLACE $5 $7 $6 100 30
      IntOp $7 $7 - 108
      !insertmacro REEL_ITEM $5 1
      !insertmacro REEL_PLACE $5 $7 $6 100 30
      ; Back is never used, and NSIS shows it again on some pages: no size at all
      !insertmacro REEL_ITEM $5 3
      ShowWindow $5 ${SW_HIDE}
      System::Call 'user32::SetWindowPos(p r5, p 0, i 0, i 0, i 0, i 0, i 0x14)'
    ${EndIf}
  FunctionEnd

  ; The install's page: the bar and its one line under the title, the details only when it
  ; stopped; no buttons while it runs.
  Function ${UN}ReelProgressShow
    StrCpy $6 8
    !if "${UN}" == ""
      ${If} $UpdateMode = 1
        !insertmacro REEL_ITEM $0 1037
        SendMessage $0 ${WM_SETTEXT} 0 "STR:Updating ${REEL_TITLE} to ${VERSION}"
      ${EndIf}
    !endif
    FindWindow $0 "#32770" "" $HWNDPARENT
    SetCtlColors $0 "" ${REEL_PAGE}
    ${If} $ReelSmall = 1
      IntOp $3 ${REEL_SMALL_W} - ${REEL_SMALL_M}
      IntOp $3 $3 - ${REEL_SMALL_M}
      IntOp $5 ${REEL_SMALL_M} + 58
      !insertmacro REEL_PLACE $0 ${REEL_SMALL_M} $5 $3 60
    ${Else}
      ; the title, the bar and its line inside the page, in the middle under the wordmark
      IntOp $3 ${REEL_W} - ${REEL_M}
      IntOp $3 $3 - ${REEL_M}
      IntOp $4 ${REEL_H} - 62
      !insertmacro REEL_PLACE $0 ${REEL_M} 62 $3 $4
      IntOp $6 $4 - 80
      IntOp $6 $6 / 2
      IntOp $6 $6 - 16
      !insertmacro REEL_ITEM $1 1037
      System::Call 'user32::GetWindowTextW(p r1, w .r7, i 256)'
      ShowWindow $1 ${SW_HIDE}
      !insertmacro REEL_ITEM $1 1038
      ShowWindow $1 ${SW_HIDE}
      System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w r7, i 0x50000000, i 0, i 0, i 0, i 0, p r0, p 1301, p 0, p 0) p .r1'
      SendMessage $1 ${WM_SETFONT} $ReelTitleFont 1
      SetCtlColors $1 ${REEL_INK} ${REEL_PAGE}
      !insertmacro REEL_PLACE $1 0 $6 $3 32
      IntOp $6 $6 + 44
    ${EndIf}
    GetDlgItem $1 $0 1004
    System::Call 'uxtheme::SetWindowTheme(p r1, w " ", w " ")'
    System::Call 'user32::GetWindowLongW(p r1, i -20) i .r2'
    IntOp $2 $2 & 0xFFFDFDFF
    System::Call 'user32::SetWindowLongW(p r1, i -20, i r2)'
    System::Call 'user32::GetWindowLongW(p r1, i -16) i .r2'
    IntOp $2 $2 & 0xFF7FFFFF
    IntOp $2 $2 | 1
    System::Call 'user32::SetWindowLongW(p r1, i -16, i r2)'
    System::Call 'user32::SendMessageW(p r1, i 0x0409, p 0, p ${REEL_BAR})'
    System::Call 'user32::SendMessageW(p r1, i 0x2001, p 0, p ${REEL_TRACK})'
    !insertmacro REEL_PLACE $1 0 $6 $3 6
    System::Call 'user32::SetWindowPos(p r1, p 0, i 0, i 0, i 0, i 0, i 0x37)'
    GetDlgItem $1 $0 1006
    SetCtlColors $1 ${REEL_QUIET} ${REEL_PAGE}
    IntOp $6 $6 + 16
    !insertmacro REEL_PLACE $1 0 $6 $3 20
    GetDlgItem $1 $0 1027
    ShowWindow $1 ${SW_HIDE}
    GetDlgItem $1 $0 1016
    ShowWindow $1 ${SW_HIDE}
    !insertmacro REEL_FOOTER ${SW_HIDE} ${SW_HIDE} ${SW_HIDE}
  FunctionEnd

  ; When it stopped: the details under the bar, their last line what stopped it, and Close.
  Function ${UN}ReelProgressStopped
    FindWindow $0 "#32770" "" $HWNDPARENT
    ${If} $ReelSmall = 1
      Return
    ${EndIf}
    IntOp $3 ${REEL_W} - ${REEL_M}
    IntOp $3 $3 - ${REEL_M}
    GetDlgItem $1 $0 1301
    ShowWindow $1 ${SW_HIDE}
    !insertmacro REEL_ITEM $1 1037
    !insertmacro REEL_PLACE $1 ${REEL_M} ${REEL_TITLE_Y} $3 32
    ShowWindow $1 ${SW_SHOW}
    IntOp $4 ${REEL_H} - ${REEL_FOOT}
    IntOp $4 $4 - 136
    !insertmacro REEL_PLACE $0 ${REEL_M} 128 $3 $4
    GetDlgItem $1 $0 1004
    !insertmacro REEL_PLACE $1 0 8 $3 6
    GetDlgItem $1 $0 1006
    ShowWindow $1 ${SW_HIDE}
    IntOp $4 ${REEL_H} - ${REEL_FOOT}
    IntOp $4 $4 - 128
    IntOp $4 $4 - 40
    GetDlgItem $1 $0 1016
    !insertmacro REEL_PLACE $1 0 26 $3 $4
    ShowWindow $1 ${SW_SHOW}
    !insertmacro REEL_FOOTER ${SW_SHOW} ${SW_HIDE} ${SW_SHOW}
    !insertmacro REEL_ITEM $1 1
    System::Call 'user32::SetWindowPos(p r1, p 0, i 0, i 0, i 0, i 0, i 0x14)'
    !insertmacro REEL_BUTTON 2 "Close"
  FunctionEnd
!macroend
