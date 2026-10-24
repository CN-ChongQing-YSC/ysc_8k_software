#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""命名管道探针：向 \\\\.\\pipe\\ysc_8k_driver 发命令并收集事件。

单线程 + PeekNamedPipe，避免 Python 文件对象同句柄并发读写的内部锁死锁
（见仓库 CLAUDE.md 的说明）。用于 Rust 驱动与 C++ 参照驱动的差分测试。
"""

from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import json
import sys
import time

PIPE_NAME = r"\\.\pipe\ysc_8k_driver"
GENERIC_READ = 0x80000000
GENERIC_WRITE = 0x40000000
OPEN_EXISTING = 3

k32 = ctypes.WinDLL("kernel32", use_last_error=True)
k32.CreateFileW.argtypes = [
    wt.LPCWSTR,
    wt.DWORD,
    wt.DWORD,
    ctypes.c_void_p,
    wt.DWORD,
    wt.DWORD,
    wt.HANDLE,
]
k32.CreateFileW.restype = wt.HANDLE
k32.WriteFile.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    wt.DWORD,
    ctypes.POINTER(wt.DWORD),
    ctypes.c_void_p,
]
k32.WriteFile.restype = wt.BOOL
k32.ReadFile.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    wt.DWORD,
    ctypes.POINTER(wt.DWORD),
    ctypes.c_void_p,
]
k32.ReadFile.restype = wt.BOOL
k32.PeekNamedPipe.argtypes = [
    wt.HANDLE,
    ctypes.c_void_p,
    wt.DWORD,
    ctypes.POINTER(wt.DWORD),
    ctypes.POINTER(wt.DWORD),
    ctypes.POINTER(wt.DWORD),
]
k32.PeekNamedPipe.restype = wt.BOOL
k32.CloseHandle.argtypes = [wt.HANDLE]
k32.CloseHandle.restype = wt.BOOL

INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


class Pipe:
    def __init__(self, timeout_s: float = 6.0):
        deadline = time.time() + timeout_s
        self.h = None
        while time.time() < deadline:
            h = k32.CreateFileW(
                PIPE_NAME,
                GENERIC_READ | GENERIC_WRITE,
                0,
                None,
                OPEN_EXISTING,
                0,
                None,
            )
            if h and h != INVALID_HANDLE_VALUE:
                self.h = h
                break
            time.sleep(0.2)
        if not self.h:
            raise RuntimeError("无法连接命名管道 " + PIPE_NAME)
        self.buf = b""
        self.events = []

    def close(self):
        if self.h:
            k32.CloseHandle(self.h)
            self.h = None

    def send(self, obj):
        data = (json.dumps(obj, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
        written = wt.DWORD(0)
        ok = k32.WriteFile(self.h, data, len(data), ctypes.byref(written), None)
        if not ok:
            raise RuntimeError("WriteFile 失败: %d" % ctypes.get_last_error())

    def _read_once(self):
        avail = wt.DWORD(0)
        if not k32.PeekNamedPipe(self.h, None, 0, None, ctypes.byref(avail), None):
            return False
        if avail.value == 0:
            return True
        chunk = ctypes.create_string_buffer(65536)
        got = wt.DWORD(0)
        if not k32.ReadFile(self.h, chunk, 65536, ctypes.byref(got), None):
            return False
        if got.value:
            self.buf += chunk.raw[: got.value]
        return True

    def drain(self, seconds: float = 0.4):
        end = time.time() + seconds
        out = []
        while time.time() < end:
            if not self._read_once():
                break
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                text = line.decode("utf-8", "replace").strip()
                if not text:
                    continue
                try:
                    obj = json.loads(text)
                except Exception:
                    obj = {"_raw": text}
                out.append(obj)
                self.events.append(obj)
            time.sleep(0.005)
        return out


def run_commands(cmds, settle=0.35):
    p = Pipe()
    results = []
    try:
        for cmd, wait in cmds:
            p.send(cmd)
            results.append((cmd.get("type"), p.drain(wait if wait is not None else settle)))
        # 尾部再收一次
        results.append(("_tail", p.drain(1.0)))
    finally:
        p.close()
    return results


DEFAULT = [
    ({"type": "get_version"}, 0.2),
    ({"type": "get_state"}, 0.2),
    ({"type": "enum_ports"}, 0.2),
    ({"type": "device_list"}, 0.2),
    ({"type": "get_monitor"}, 0.2),
    ({"type": "get_kbd_state"}, 0.2),
    ({"type": "get_local_ip"}, 0.2),
    ({"type": "get_kmnet_stat"}, 0.2),
    ({"type": "check_ch343_driver"}, 0.5),
    ({"type": "towmcu_list_ports"}, 0.5),
    ({"type": "kmnet_start", "port": 5251}, 0.4),
    ({"type": "get_kmnet_stat"}, 0.2),
    ({"type": "kmnet_stop"}, 0.3),
    ({"type": "human_guard_begin", "axis": "x", "duration": 1000}, 0.3),
    ({"type": "human_guard_status"}, 0.2),
    ({"type": "human_guard_stop"}, 1.2),
    ({"type": "send_ysc", "cmd": '{"cmd":132}'}, 0.2),
    ({"type": "serial_connect", "port": "COM999", "baud": 0}, 2.5),
    ({"type": "device_attach", "dev": 0, "port": "COM999", "baud": 0}, 2.5),
    ({"type": "get_guard_config"}, 0.2),
    ({"type": "debug_status"}, 0.2),
    ({"type": "iap_enter"}, 0.2),
    ({"type": "iap_start", "path": ""}, 0.2),
    ({"type": "towmcu_query_version", "port": ""}, 0.2),
]


def main():
    cmds = DEFAULT
    if len(sys.argv) > 1:
        with open(sys.argv[1], "r", encoding="utf-8") as f:
            raw = json.load(f)
        cmds = [(c, 0.4) for c in raw]
    results = run_commands(cmds)
    for name, evs in results:
        print("##", name)
        for e in evs:
            print(json.dumps(e, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
