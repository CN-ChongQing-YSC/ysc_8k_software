#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
test_dll.py — 完整测试 ysc_sdk.dll 的所有导出接口（Python ctypes 调用）
=====================================================================
覆盖：
  1) DLL 加载 / 版本 / 支持波特率 / 错误机制
  2) 串口枚举（YSC 设备 + 全部 COM）
  3) 探测波特率（如指定端口）
  4) 连接 / 版本查询 / 通用 send_command（同步等待返回）
  5) 鼠标移动 / 按键 / 上报状态 等即发即弃命令
  6) 手柄映射查询
  7) 原始字节发送
  8) 波特率切换（仅当 --baud-test 时执行，会真实切换硬件波特率）
  9) 断开 / 资源释放

用法：
  python test_dll.py                      # 仅离线测试（不连接硬件）
  python test_dll.py --port COM7          # 连接 COM7（自动探测波特率）后做完整测试
  python test_dll.py --port COM7 --baud 115200
  python test_dll.py --port COM7 --baud-test   # 额外做波特率切换往返测试
"""

from __future__ import annotations

import argparse
import ctypes
import os
import sys
import time
import traceback

# 让脚本能直接 import 同目录的 ysc_sdk
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ysc_sdk import YscSdk, YscDevice, YscError  # noqa: E402


# ---- 测试结果统计 ----
class Reporter:
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.skipped = 0

    def _line(self, tag, msg, color):
        # 简易 ANSI 着色；Windows 10+ 终端支持
        if sys.stdout.isatty():
            print(f"  {color}{tag}\033[0m {msg}")
        else:
            print(f"  [{tag}] {msg}")

    def ok(self, msg):
        self.passed += 1
        self._line("PASS", msg, "\033[32m")

    def fail(self, msg):
        self.failed += 1
        self._line("FAIL", msg, "\033[31m")

    def skip(self, msg):
        self.skipped += 1
        self._line("SKIP", msg, "\033[33m")

    def section(self, title):
        print(f"\n=== {title} ===")


def expect(cond, msg, rep: Reporter):
    (rep.ok if cond else rep.fail)(msg)
    return bool(cond)


# ---- 离线测试（不依赖硬件） ----
def test_offline(sdk: YscSdk, rep: Reporter):
    rep.section("1. DLL 基本信息")
    expect(bool(sdk.version), f"Ysc_SdkVersion() = '{sdk.version}'", rep)

    py_bits = "x64" if ctypes.sizeof(ctypes.c_void_p) == 8 else "x86"
    expect(sdk.dll_arch in ("x64", "x86"),
           f"DLL 架构匹配: dll={sdk.dll_arch}, python={py_bits} "
           f"({'OK' if sdk.dll_arch == py_bits else '架构不一致，可能加载失败！'})", rep)
    print(f"     DLL 路径: {sdk.dll_path}")

    bauds = sdk.supported_baudrates
    expect(len(bauds) == 9 and bauds[0] == 115200 and bauds[-1] == 4000000,
           f"Ysc_SupportedBaudrates() = {bauds}", rep)

    rep.section("2. 串口枚举")
    ysc_ports = sdk.list_ports()
    expect(isinstance(ysc_ports, list),
           f"Ysc_ListPorts() -> {ysc_ports if ysc_ports else '(暂无 YSC 设备)'}", rep)
    if ysc_ports:
        for p in ysc_ports:
            assert all(k in p for k in ("port", "serial", "side", "desc")), f"字段缺失: {p}"
        rep.ok(f"每个端口字段齐全(port/serial/side/desc)，共 {len(ysc_ports)} 个")
    else:
        rep.skip("未发现 YSC 设备(VID 1A86/PID FE0C)，枚举返回空数组(正常)")

    all_ports = sdk.list_all_com_ports()
    expect(isinstance(all_ports, list),
           f"Ysc_ListAllComPorts() -> {len(all_ports)} 个 COM 口: "
           f"{[p['port'] for p in all_ports]}", rep)

    rep.section("3. 错误机制（Ysc_LastError）")
    # 故意探测一个不存在的端口，触发错误字符串
    fake = "COM999"
    b = sdk.detect_baudrate(fake)
    expect(b == 0, f"探测不存在端口 {fake} 返回 0（实际 {b}）", rep)
    expect(bool(sdk.last_error),
           f"Ysc_LastError() 已填充: '{sdk.last_error}'", rep)


# ---- 在线测试（需要真实硬件） ----
def test_online(sdk: YscSdk, rep: Reporter, port: str, baud: int,
                do_baud_test: bool):
    rep.section(f"4. 连接 {port} (baud={baud if baud else '自动探测'})")
    try:
        dev = sdk.connect(port, baud)
    except YscError as e:
        rep.fail(f"连接失败: {e}")
        return
    rep.ok(f"连接成功: baudrate={dev.baudrate}")
    expect(dev.connected, "Ysc_IsConnected() == true", rep)
    expect(dev.baudrate in sdk.supported_baudrates,
           f"探测到有效波特率 {dev.baudrate}", rep)

    try:
        # 5. 版本查询（同步等待返回）
        rep.section("5. 版本查询 (cmd:132, 同步等待返回)")
        try:
            ver = dev.query_version(timeout_ms=1500)
            rep.ok(f"Ysc_QueryVersion() -> {ver}")
        except YscError as e:
            rep.fail(f"版本查询失败: {e}")

        # 通用 send_command 自测
        rep.section("6. 通用 send_command (同步)")
        try:
            resp = dev.send_command('{"cmd":132}', timeout_ms=1500)
            rep.ok(f"send_command('{{\"cmd\":132}}') -> {resp}")
        except YscError as e:
            rep.fail(f"send_command 失败: {e}")

        # 7. 即发即弃命令（移动/按键/上报）
        rep.section("7. 鼠标 / 键盘 / 上报状态 (NoWait)")
        for desc, fn in [
            ("mouse_move(50,50,1)",      lambda: dev.mouse_move(50, 50, 1)),
            ("mouse_move_tow(10,0,1)",   lambda: dev.mouse_move_tow(10, 0, 1)),
            ("mouse_button(1,True/False)", lambda: (dev.mouse_button(1, True),
                                                     time.sleep(0.05),
                                                     dev.mouse_button(1, False))),
            ("keyboard_key(0x04,True/False)", lambda: (dev.keyboard_key(0x04, True),
                                                       time.sleep(0.05),
                                                       dev.keyboard_key(0x04, False))),
            ("keyboard_release_all()",   lambda: dev.keyboard_release_all()),
            ("upload_status(False)",     lambda: dev.upload_status(False)),
        ]:
            try:
                fn()
                rep.ok(desc)
            except YscError as e:
                rep.fail(f"{desc}: {e}")
            time.sleep(0.03)

        # 8. 手柄映射查询
        rep.section("8. 手柄映射 (cmd:101)")
        try:
            cfg = dev.gamepad_get_config(timeout_ms=1500)
            rep.ok(f"gamepad_get_config() -> {cfg}")
        except YscError as e:
            rep.skip(f"手柄映射查询无响应/失败: {e}（设备可能未启用）")

        # 9. 原始字节（MAKCU 文本）
        rep.section("9. 原始字节发送 (Ysc_SendRaw)")
        try:
            dev.send_raw(b"$KE\r\n")  # MAKCU 探测命令示例
            rep.ok("send_raw(b'$KE\\r\\n')")
        except YscError as e:
            rep.fail(f"send_raw 失败: {e}")

        # 10. 波特率切换往返（可选）
        if do_baud_test:
            rep.section("10. 波特率切换往返 (cmd:133)")
            orig = dev.baudrate
            # 选一个不同于当前的、安全的目标波特率
            candidates = [b for b in sdk.supported_baudrates if b != orig]
            target = candidates[0]
            try:
                dev.switch_baudrate(target)
                rep.ok(f"切换 {orig} -> {target} 成功，当前 {dev.baudrate}")
                dev.switch_baudrate(orig)
                rep.ok(f"切回 {target} -> {orig} 成功，当前 {dev.baudrate}")
            except YscError as e:
                rep.fail(f"波特率切换失败: {e}（注意：设备可能停留在 {target}）")
        else:
            rep.section("10. 波特率切换 (跳过，加 --baud-test 启用)")
            rep.skip("默认不执行波特率切换以免影响硬件状态")

    finally:
        rep.section("11. 断开 / 释放")
        dev.disconnect()
        expect(not dev.connected, "disconnect() 后 connected == false", rep)


def main():
    ap = argparse.ArgumentParser(description="完整测试 ysc_sdk.dll")
    ap.add_argument("--dll", help="指定 ysc_sdk.dll 路径（默认自动查找）")
    ap.add_argument("--port", help="要连接的串口（如 COM7），不传则只做离线测试")
    ap.add_argument("--baud", type=int, default=0, help="连接波特率，0=自动探测(默认)")
    ap.add_argument("--baud-test", action="store_true",
                    help="执行波特率切换往返测试（会真实切换硬件波特率）")
    args = ap.parse_args()

    print("############################################################")
    print("#   YSC 8K SDK (ysc_sdk.dll) 完整接口测试  (Python ctypes) #")
    print("############################################################")

    rep = Reporter()

    try:
        sdk = YscSdk(args.dll)
    except YscError as e:
        rep.section("0. 加载 DLL")
        rep.fail(str(e))
        sys.exit(1)

    test_offline(sdk, rep)

    if args.port:
        test_online(sdk, rep, args.port, args.baud, args.baud_test)
    else:
        rep.section("4~11. 在线测试")
        rep.skip("未指定 --port，跳过硬件在线测试（可接上 YSC 设备后加 --port COMx 重跑）")

    print("\n============================================================")
    print(f"  结果汇总：  PASS={rep.passed}   FAIL={rep.failed}   SKIP={rep.skipped}")
    print("============================================================")
    sys.exit(0 if rep.failed == 0 else 1)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n(中断)")
        sys.exit(130)
    except Exception:
        traceback.print_exc()
        sys.exit(2)
