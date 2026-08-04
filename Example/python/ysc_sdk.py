"""
ysc_sdk.py — YSC 8K 驱动 SDK 的 Python(ctypes) 封装
=====================================================================
本模块是对 ysc_sdk.dll 的完整 Python 绑定。易语言 / C# / Delphi 等语言
直接调用同一个 DLL，Python 这里用 ctypes 加载，提供面向对象的封装。

核心能力（全部由 DLL 实现，Python 不重复开发驱动逻辑）：
  - 列出串口（YSC 设备 / 全部 COM）
  - 连接串口（指定波特率 或 0=自动探测）
  - 切换波特率
  - 发送任意 YSC 协议命令（同步等待返回）或原始字节
  - 鼠标 / 键盘 / 上报状态 / 跳转 IAP / 手柄映射 等便捷封装

用法：
    from ysc_sdk import YscSdk, YscDevice
    sdk = YscSdk()                       # 自动定位 ysc_sdk.dll
    print(sdk.version, sdk.supported_baudrates)
    print(sdk.list_ports())              # 列出 YSC 设备
    dev = sdk.connect("COM7", 0)         # 0 = 自动探测波特率
    print(dev.baudrate, dev.version(timeout_ms=1000))
    dev.mouse_move(100, 100, 1)
    dev.disconnect()
"""

from __future__ import annotations

import ctypes
import json
import os
import platform
import struct
import sys
from ctypes import (
    CDLL, WinDLL, c_char_p, c_int, c_uint8, c_uint32, c_void_p, POINTER, byref, create_string_buffer,
)
from typing import List, Optional, Dict, Any


class YscError(RuntimeError):
    """SDK 调用错误（携带 DLL 内 Ysc_LastError 的描述）。"""


def _python_is_64bit() -> bool:
    return struct.calcsize("P") * 8 == 64


def _default_dll_search_dirs() -> List[str]:
    """返回候选 DLL 目录（按优先级）。"""
    here = os.path.dirname(os.path.abspath(__file__))
    # 相对于 Example/python/ 回到仓库根，再进 ysc_8k_driver/bin
    repo_root = os.path.abspath(os.path.join(here, "..", ".."))
    arch_dir = "x64" if _python_is_64bit() else "Win32"
    built = os.path.join(repo_root, "ysc_8k_driver", "bin", arch_dir, "Release", "ysc_sdk.dll")
    candidates = [
        built,
        os.path.join(here, "ysc_sdk.dll"),                       # 脚本同目录
        os.path.join(here, arch_dir, "ysc_sdk.dll"),
        os.path.join(os.getcwd(), "ysc_sdk.dll"),
    ]
    return candidates


class YscDevice:
    """一个已连接的设备句柄（对应 DLL 里的 YscDevice*）。"""

    def __init__(self, sdk: "YscSdk", handle: int, port: str, baud: int):
        self._sdk = sdk
        self._handle = c_void_p(handle)
        self.port = port
        self.baudrate = baud

    # ---- 生命周期 ----
    def disconnect(self) -> None:
        if self._handle:
            self._sdk._dll.Ysc_Disconnect(self._handle)
            self._handle = None

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        self.disconnect()

    @property
    def connected(self) -> bool:
        return bool(self._handle) and bool(self._sdk._dll.Ysc_IsConnected(self._handle))

    # ---- 通用发送 ----
    def send_command(self, json_str: str, timeout_ms: int = 1000) -> str:
        """发送一帧 JSON 命令，同步等待下一条带帧响应，返回响应负载字符串。

        超时无响应抛 YscError。
        """
        if not self._handle:
            raise YscError("device closed")
        out = create_string_buffer(8192)
        n = self._sdk._dll.Ysc_SendCommand(
            self._handle, json_str.encode("utf-8"), c_int(timeout_ms), out, c_int(len(out))
        )
        if n >= 0:
            return out.value.decode("utf-8", errors="replace")
        if n == -2:
            raise YscError(f"timeout ({timeout_ms} ms) waiting for response: {self._sdk.last_error}")
        raise YscError(f"send_command failed: {self._sdk.last_error}")

    def send_command_no_wait(self, json_str: str) -> None:
        """发送一帧 JSON 命令但不等待返回（鼠标移动等即发即弃命令）。"""
        if not self._handle:
            raise YscError("device closed")
        ok = self._sdk._dll.Ysc_SendCommandNoWait(self._handle, json_str.encode("utf-8"))
        if not ok:
            raise YscError(f"send_command_no_wait failed: {self._sdk.last_error}")

    def send_raw(self, data: bytes) -> None:
        """发送原始字节（不加帧），用于 MAKCU 文本协议。"""
        if not self._handle:
            raise YscError("device closed")
        buf = (c_uint8 * len(data)).from_buffer_copy(data)
        ok = self._sdk._dll.Ysc_SendRaw(self._handle, buf, c_int(len(data)))
        if not ok:
            raise YscError(f"send_raw failed: {self._sdk.last_error}")

    # ---- 查询 ----
    def query_version(self, timeout_ms: int = 1000) -> str:
        return self.send_command('{"cmd":132}', timeout_ms)

    # ---- 便捷命令（移动/按键类无可靠返回 -> no_wait） ----
    def mouse_move(self, x: int, y: int, steps: int = 1) -> None:
        self.send_command_no_wait(f'{{"cmd":30,"x":{int(x)},"y":{int(y)},"c":{int(steps)}}}')

    def mouse_move_tow(self, x: int, y: int, steps: int = 1) -> None:
        self.send_command_no_wait(f'{{"cmd":31,"x":{int(x)},"y":{int(y)},"c":{int(steps)}}}')

    def mouse_button(self, button_mask: int, pressed: bool) -> None:
        self.send_command_no_wait(f'{{"cmd":33,"b":{int(button_mask) & 0xFF},"s":{1 if pressed else 0}}}')

    def keyboard_key(self, keycode: int, down: bool) -> None:
        """注入一次按键。keycode 为 HID 键码 0x04-0xE7（修饰键 0xE0-0xE7）。"""
        self.send_command_no_wait(f'{{"cmd":45,"kc":{int(keycode) & 0xFF},"down":{1 if down else 0}}}')

    def keyboard_release_all(self) -> None:
        self.send_command_no_wait('{"cmd":46}')

    def keyboard_type_string(self, s: str) -> None:
        """逐字打出一段混合大小写 ASCII 字符串 (cmd:47)。固件读取 PC 下发的
        CapsLock 状态(HID LED 报告)，对每个字母按 目标大小写 XOR CapsLock 决定
        是否自动按 Shift，符号/数字不受 CapsLock 影响。s 需为 1..128 字节。"""
        if not s:
            raise YscError("empty string")
        if len(s.encode("utf-8", errors="ignore")) > 128:
            raise YscError("string too long (max 128 bytes)")
        # json.dumps 产出已转义的带引号 JSON 字符串，直接拼到 "s": 字段
        self.send_command_no_wait('{"cmd":47,"s":' + json.dumps(s) + '}')

    def upload_status(self, enable: bool) -> None:
        self.send_command_no_wait(f'{{"cmd":34,"status":{1 if enable else 0}}}')

    def jump_iap(self) -> None:
        self.send_command_no_wait('{"cmd":50}')

    # ---- 手柄映射 ----
    def gamepad_get_config(self, timeout_ms: int = 1000) -> str:
        return self.send_command('{"cmd":101}', timeout_ms)

    def gamepad_set_config(self, config_obj: Any) -> None:
        """config_obj 为 dict 或 JSON 字符串，会被包进 {"cmd":100,"data":<...>}。"""
        cfg = json.dumps(config_obj) if not isinstance(config_obj, str) else config_obj
        # {"cmd":100,"data":<cfg>} —— 直接拼到 data 字段
        self.send_command_no_wait('{"cmd":100,"data":' + cfg + '}')

    def gamepad_enable(self, on: bool) -> None:
        self.send_command_no_wait(f'{{"cmd":102,"on":{1 if on else 0}}}')

    def gamepad_reset(self) -> None:
        self.send_command_no_wait('{"cmd":104}')

    # ---- 波特率切换 ----
    def switch_baudrate(self, new_baud: int) -> None:
        ok = self._sdk._dll.Ysc_SwitchBaudrate(self._handle, c_uint32(new_baud))
        if not ok:
            raise YscError(f"switch_baudrate({new_baud}) failed: {self._sdk.last_error}")
        self.baudrate = new_baud


class YscSdk:
    """SDK 单例：加载 DLL 并提供静态/工厂方法。"""

    def __init__(self, dll_path: Optional[str] = None):
        path = dll_path or os.environ.get("YSC_SDK_DLL")
        tried: List[str] = []
        if not path:
            for c in _default_dll_search_dirs():
                tried.append(c)
                if os.path.isfile(c):
                    path = c
                    break
        if not path or not os.path.isfile(path):
            raise YscError(
                "找不到 ysc_sdk.dll。请先编译（见 ysc_8k_driver/ysc_sdk.sln），"
                "或通过参数 / 环境变量 YSC_SDK_DLL 指定路径。尝试过:\n  " +
                "\n  ".join(tried or ["(无候选)"])
            )
        self.dll_path = os.path.abspath(path)
        # DLL 为 stdcall 调用约定（易语言/VB6/Delphi 默认）。
        #   x64：WinDLL 与 CDLL 等价（只有一种 x64 约定）；
        #   x86：必须用 WinDLL（stdcall），用 CDLL(cdecl) 会栈错乱。
        self._dll = WinDLL(self.dll_path)
        self._configure_prototypes()
        self.dll_arch = self._detect_dll_arch()

    # ---- 原型声明 ----
    def _configure_prototypes(self) -> None:
        d = self._dll

        def f(name, restype, argtypes):
            fn = getattr(d, name)
            fn.restype = restype
            fn.argtypes = argtypes
            return fn

        self._Ysc_SdkVersion        = f("Ysc_SdkVersion", c_char_p, [])
        self._Ysc_LastError         = f("Ysc_LastError", c_char_p, [])
        self._Ysc_SupportedBaudrates = f("Ysc_SupportedBaudrates",
                                         POINTER(c_uint32), [POINTER(c_int)])
        self._Ysc_ListPorts         = f("Ysc_ListPorts", c_int, [c_char_p, c_int])
        self._Ysc_ListAllComPorts   = f("Ysc_ListAllComPorts", c_int, [c_char_p, c_int])
        self._Ysc_Connect           = f("Ysc_Connect", c_void_p,
                                        [c_char_p, c_uint32, c_char_p, c_int])
        self._Ysc_Disconnect        = f("Ysc_Disconnect", None, [c_void_p])
        self._Ysc_DetectBaudrate    = f("Ysc_DetectBaudrate", c_uint32, [c_char_p])
        self._Ysc_IsConnected       = f("Ysc_IsConnected", c_int, [c_void_p])
        self._Ysc_GetBaudrate       = f("Ysc_GetBaudrate", c_uint32, [c_void_p])
        self._Ysc_GetPortName       = f("Ysc_GetPortName", c_int, [c_void_p, c_char_p, c_int])

    def _detect_dll_arch(self) -> str:
        # 简易判断：通过是否存在 IMAGE_FILE_MACHINE_AMD64
        with open(self.dll_path, "rb") as fp:
            fp.seek(0x3C)
            pe_off = int.from_bytes(fp.read(4), "little")
            fp.seek(pe_off + 4)
            machine = int.from_bytes(fp.read(2), "little")
        return "x64" if machine == 0x8664 else ("x86" if machine == 0x014C else f"machine={machine:#x}")

    # ---- 信息 ----
    @property
    def version(self) -> str:
        return self._Ysc_SdkVersion().decode("utf-8")

    @property
    def last_error(self) -> str:
        return self._Ysc_LastError().decode("utf-8", errors="replace")

    @property
    def supported_baudrates(self) -> List[int]:
        cnt = c_int(0)
        ptr = self._Ysc_SupportedBaudrates(byref(cnt))
        return [ptr[i] for i in range(cnt.value)]

    # ---- 串口枚举 ----
    def _call_json_list(self, fn, *args) -> List[Dict[str, Any]]:
        # 两段式缓冲：先 8K，若不够按返回值扩容重试
        size = 8192
        for _ in range(4):
            buf = create_string_buffer(size)
            need = fn(*args, buf, c_int(size))
            if need < 0:
                raise YscError(f"list failed: {self.last_error}")
            if need < size - 1:
                txt = buf.value.decode("utf-8", errors="replace")
                return json.loads(txt) if txt else []
            size = need + 16  # 截断了，扩容重试
        raise YscError("list buffer kept overflowing")

    def list_ports(self) -> List[Dict[str, Any]]:
        """列出 YSC towmcu CDC 设备（VID 1A86/PID FE0C）。"""
        return self._call_json_list(self._Ysc_ListPorts)

    def list_all_com_ports(self) -> List[Dict[str, Any]]:
        """列出本机全部 COM 串口。"""
        return self._call_json_list(self._Ysc_ListAllComPorts)

    # ---- 连接 ----
    def detect_baudrate(self, port: str) -> int:
        b = self._Ysc_DetectBaudrate(port.encode("utf-8"))
        return int(b)

    def connect(self, port: str, baud: int = 0, timeout_detect: bool = True) -> YscDevice:
        """连接串口。baud=0 自动探测。失败抛 YscError。"""
        err = create_string_buffer(512)
        h = self._Ysc_Connect(port.encode("utf-8"), c_uint32(baud), err, c_int(len(err)))
        if not h:
            msg = err.value.decode("utf-8", errors="replace") or self.last_error
            raise YscError(f"connect({port}, baud={baud}) failed: {msg}")
        actual_baud = int(self._Ysc_GetBaudrate(h))
        return YscDevice(self, h, port, actual_baud)
