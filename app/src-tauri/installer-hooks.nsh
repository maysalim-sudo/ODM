!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\odm-host.exe" --unregister'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; An update runs the old uninstaller with /UPDATE: keep the tools and "Start ODM with
  ; Windows" then. Only a real uninstall removes them.
  ${If} $UpdateMode <> 1
    ; Remove only the video tools ODM downloaded. Downloads and the database are left alone.
    RMDir /r "$LOCALAPPDATA\ODM\tools"
    ; "Start ODM with Windows": tauri-plugin-autostart names the value after the app name
    ; set in lib.rs (.app_name("ODM")), in Run and in Task Manager's StartupApproved list.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ODM"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "ODM"
  ${EndIf}
!macroend
