; How Windows opens magnet links and .torrent files when the user chose ODM for them.
!define ODM_OPEN_CMD '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'

!macro NSIS_HOOK_POSTINSTALL
  ; Asked on a normal install only, and No by default. Silent, passive and update runs
  ; leave the user's earlier choice as it is.
  ${If} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
    IfSilent odm_assoc_done
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Open magnet links and .torrent files with ODM?$\r$\n$\r$\nYou can change this later in Windows Settings > Apps > Default apps." IDNO odm_assoc_done
      WriteRegStr HKCU "Software\Classes\magnet" "" "URL:Magnet link"
      WriteRegStr HKCU "Software\Classes\magnet" "URL Protocol" ""
      WriteRegStr HKCU "Software\Classes\magnet\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
      WriteRegStr HKCU "Software\Classes\magnet\shell\open\command" "" '${ODM_OPEN_CMD}'
      WriteRegStr HKCU "Software\Classes\ODM.torrent" "" "Torrent file"
      WriteRegStr HKCU "Software\Classes\ODM.torrent\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
      WriteRegStr HKCU "Software\Classes\ODM.torrent\shell\open\command" "" '${ODM_OPEN_CMD}'
      WriteRegStr HKCU "Software\Classes\.torrent" "" "ODM.torrent"
      WriteRegStr HKCU "Software\Classes\.torrent" "Content Type" "application/x-bittorrent"
      ; Tell Explorer the associations changed (SHCNE_ASSOCCHANGED).
      System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
    odm_assoc_done:
  ${EndIf}
!macroend

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
    ; Magnet links and .torrent files: only what still points to this ODM goes.
    ReadRegStr $0 HKCU "Software\Classes\magnet\shell\open\command" ""
    ${If} $0 == '${ODM_OPEN_CMD}'
      DeleteRegKey HKCU "Software\Classes\magnet"
    ${EndIf}
    ReadRegStr $0 HKCU "Software\Classes\.torrent" ""
    ${If} $0 == "ODM.torrent"
      DeleteRegKey HKCU "Software\Classes\.torrent"
    ${EndIf}
    DeleteRegKey HKCU "Software\Classes\ODM.torrent"
    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  ${EndIf}
!macroend
