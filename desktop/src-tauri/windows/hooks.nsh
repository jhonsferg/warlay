; The daemon is self-starting -- the desktop app's ensure_daemon_running
; (see desktop/src-tauri/src/commands/daemon.rs) already checks for a
; running wnetd.exe on every launch and spawns one (elevated) if none is
; up, so registering a logon auto-start task here would just be a second,
; redundant way to reach the same state. The installer sticks to what an
; installer is actually for: placing the files and generating an
; uninstaller. The one thing the uninstaller still needs is to stop any
; running wnetd.exe first, since Windows can't delete a locked .exe.

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Deteniendo el daemon de Warlay..."
  nsExec::ExecToLog 'taskkill /F /IM wnetd.exe'
!macroend
