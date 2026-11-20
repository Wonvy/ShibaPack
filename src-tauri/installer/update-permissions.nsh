LangString ShibaPackUpdatePermission 1033 "ShibaPack cannot write to its installation folder. Approve the Windows administrator prompt with the account that installed ShibaPack, then try updating again."
LangString ShibaPackUpdatePermission 2052 "ShibaPack 无法写入安装目录。请使用安装 ShibaPack 的账户确认 Windows 管理员授权，然后重试更新。"

!macro NSIS_HOOK_PREINSTALL
  !if "${INSTALLMODE}" == "currentUser"
    ${If} $UpdateMode = 1
      Push $0
      Push $1
      Push $2
      Push $3
      Push $4

      ; An elevated retry must keep the original HKCU installation identity.
      ClearErrors
      ${GetOptions} $CMDLINE "/SHIBAPACKUPDATEUSER=" $4
      ${IfNot} ${Errors}
        System::Call 'secur32::GetUserNameExW(i 2, w .r2, *i ${NSIS_MAX_STRLEN}) i.r3'
        ${If} $3 = 0
        ${OrIf} $2 != $4
          MessageBox MB_OK|MB_ICONEXCLAMATION "$(ShibaPackUpdatePermission)" /SD IDOK
          SetErrorLevel 1
          Quit
        ${EndIf}
      ${Else}
        StrCpy $4 ""
      ${EndIf}

      ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
        ; Ask for FILE_ADD_FILE on the directory without touching the running executable.
        System::Call 'kernel32::CreateFileW(w "$INSTDIR", i 2, i 7, p 0, i 3, i 0x02000000, p 0) p.r0 ?e'
        Pop $1
        ${If} $0 != -1
          System::Call 'kernel32::CloseHandle(p r0)'
        ${ElseIf} $1 = 5 ; ERROR_ACCESS_DENIED
          ${If} $4 == ""
            System::Call 'secur32::GetUserNameExW(i 2, w .r2, *i ${NSIS_MAX_STRLEN}) i.r3'
            ${If} $3 <> 0
              StrCpy $4 '/P /UPDATE /SHIBAPACKUPDATEUSER="$2"'
              ClearErrors
              ${GetOptions} $CMDLINE "/R" $3
              ${IfNot} ${Errors}
                ${GetOptions} $CMDLINE "/ARGS" $3
                StrCpy $4 "$4 /R /ARGS $3"
              ${EndIf}
              ClearErrors
              ExecShell "runas" "$EXEPATH" "$4 /D=$INSTDIR"
              ${IfNot} ${Errors}
                SetErrorLevel 0
                Quit
              ${EndIf}
            ${EndIf}
          ${EndIf}
          MessageBox MB_OK|MB_ICONEXCLAMATION "$(ShibaPackUpdatePermission)" /SD IDOK
          SetErrorLevel 1
          Quit
        ${EndIf}
      ${EndIf}

      Pop $4
      Pop $3
      Pop $2
      Pop $1
      Pop $0
      ClearErrors
    ${EndIf}
  !endif
!macroend
