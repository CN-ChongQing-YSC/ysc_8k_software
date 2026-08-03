; NSIS 自定义钩子：安装阶段弹出 WCH CH343 驱动安装程序，由用户点击安装。
;
; 说明：本产品的主设备 (VID_1A86/PID_FE0C) 由 Windows 自带 usbser.sys 枚举，
; 不需要本驱动；但用户另有其他基于 CH343 通信的设备（PID 在 CH343SER.INF
; 覆盖范围内），确实需要该驱动。此处随软件安装时一并弹出官方安装器，
; 让用户自己点「安装」，可见、可控。
;
; 时序：customInstall 在 installApplicationFiles（解压应用包 + extraResources）
; 之后执行，故 $INSTDIR\resources\ch343-driver\CH343SER.EXE 已就位。
; NSIS 安装器进程本身已是管理员权限，CH343SER.EXE 继承，安装驱动无需再次 UAC。
; ExecWait 会阻塞直到用户关闭 WCH 安装窗口；即使用户取消，也只是不装驱动，
; 不影响主程序安装。

!macro customInstall
  SetOutPath "$INSTDIR\resources\ch343-driver"
  DetailPrint "[YSC] Launching WCH CH343 driver installer — please click INSTALL."
  ; 用 runas 动词确保以管理员权限启动（装驱动需要）：安装器已提权时不重复弹 UAC，
  ; 按用户安装（未提权）时触发一次 UAC。ExecShellWait 会等待用户操作完 GUI。
  ExecShellWait "runas" "$INSTDIR\resources\ch343-driver\CH343SER.EXE" ""
  DetailPrint "[YSC] CH343 driver installer closed."
!macroend
