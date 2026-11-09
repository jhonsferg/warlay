; The installer sticks to what an installer is actually for: placing the
; files and generating an uninstaller. The one thing the uninstaller still
; needs is to stop any running warlay.exe first (it is the whole app, UI and
; network code in one process), because Windows can't delete a locked .exe.

; Texts of the welcome and finish pages (translations live in installer/*.nsh).
; hooks.nsh is included before the pages are declared, so these defines apply.
!define MUI_WELCOMEPAGE_TITLE "$(welcomeTitle)"
!define MUI_WELCOMEPAGE_TEXT "$(welcomeText)"
!define MUI_FINISHPAGE_TITLE "$(finishTitle)"
!define MUI_FINISHPAGE_TEXT "$(finishText)"
!define MUI_ABORTWARNING

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Deteniendo Warlay..."
  nsExec::ExecToLog 'taskkill /F /IM warlay.exe'
!macroend
