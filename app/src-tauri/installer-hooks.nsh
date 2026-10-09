!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\odm-host.exe" --unregister'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove only the video tools ODM downloaded. Downloads and the database are left alone.
  RMDir /r "$LOCALAPPDATA\ODM\tools"
!macroend
