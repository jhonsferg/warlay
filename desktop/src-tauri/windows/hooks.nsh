; The installer sticks to what an installer is actually for: placing the
; files and generating an uninstaller. The one thing the uninstaller still
; needs is to stop any running warlay.exe first (it is the whole app, UI and
; network code in one process), because Windows can't delete a locked .exe.

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Deteniendo Warlay..."
  nsExec::ExecToLog 'taskkill /F /IM warlay.exe'
!macroend
