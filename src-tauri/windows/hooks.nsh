; NSIS hooks for the Veal installer (plan-19 7.2). Wired in through
; tauri.bundle.conf.json, so only `pnpm bundle` uses them.
;
; The installer cannot replace a running veal-daemon.exe, and the template's own
; running-app check looks only at veal.exe. So before files are copied or
; removed, these stop the daemon: gracefully with `veal-daemon.exe --stop`
; (the installed exe, which matches the running daemon), then by force.

!macro VEAL_STOP_DAEMON verb
  nsis_tauri_utils::FindProcessCurrentUser "veal-daemon.exe"
  Pop $R0
  ${If} $R0 = 0
    ; An update from inside Veal already warned and stopped it (plan-19 F4):
    ; ask only when a person is running the installer themselves.
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode != 1
    ${AndIf} $UpdateMode != 1
      MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "${verb} ends every shell Veal is running in the background, including any agents." IDOK +2
      Abort
    ${EndIf}
    ${If} ${FileExists} "$INSTDIR\veal-daemon.exe"
      nsExec::Exec '"$INSTDIR\veal-daemon.exe" --stop'
      Pop $R1
    ${EndIf}
    nsis_tauri_utils::FindProcessCurrentUser "veal-daemon.exe"
    Pop $R0
    ${If} $R0 = 0
      nsis_tauri_utils::KillProcessCurrentUser "veal-daemon.exe"
      Pop $R0
      Sleep 1000
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Ask about veal.exe first (plan-19 F2): cancelling "Veal is running" must
  ; not have ended the shells already. The template's own check then finds
  ; nothing left to ask about.
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro VEAL_STOP_DAEMON "Installing"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro VEAL_STOP_DAEMON "Uninstalling"
!macroend
