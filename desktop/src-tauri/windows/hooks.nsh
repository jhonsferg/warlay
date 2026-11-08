; The agent is self-starting -- the desktop app already checks for a
; running agent on every launch and spawns one (elevated) if none is up,
; so registering a logon auto-start task here would just be a second,
; redundant way to reach the same state. The installer sticks to what an
; installer is actually for: placing the files and generating an
; uninstaller. The one thing the uninstaller still needs is to stop any
; running warlay.exe first: it is both the UI and the agent (the agent is
; embedded in it now), and Windows can't delete a locked .exe.

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Deteniendo Warlay (cliente y agente)..."
  nsExec::ExecToLog 'taskkill /F /IM warlay.exe'
!macroend
