!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\odm-host.exe" --unregister'
!macroend
