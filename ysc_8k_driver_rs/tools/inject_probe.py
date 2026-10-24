#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""无硬件注入探针：用 debug_inject_slot(test=1) 直接喂设备数据通路。

覆盖：按数据流判角色、鼠标/键盘监控、715/711 探测、调试转发、物理护栏。
"""

from __future__ import annotations

import json
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402


def hx(s: str) -> str:
    return s.encode("utf-8").hex()


def inj(p: Pipe, dev: int, raw: str):
    p.send({"type": "debug_inject_slot", "test": 1, "dev": dev, "rawHex": hx(raw)})
    return p.drain(0.35)


def mouse(b: int, x: int, y: int, t: int, wh: int = 0) -> str:
    inner = json.dumps({"b": b, "x": x, "y": y, "wh": wh, "t": t}, separators=(",", ":"))
    return json.dumps({"code": 200, "message": "mouse", "data": inner}, separators=(",", ":"))


def kbd(t: int, keys: list[int]) -> str:
    inner = json.dumps({"mod": 2, "n": len(keys), "ov": 0, "k": keys, "t": t}, separators=(",", ":"))
    return json.dumps({"code": 200, "message": "kbd", "data": inner}, separators=(",", ":"))


def main():
    p = Pipe()
    try:
        print("mouse", json.dumps(inj(p, 0, mouse(3, 10, -5, 1000, 1)), ensure_ascii=False))
        p.send({"type": "get_monitor"})
        print("monitor1", json.dumps(p.drain(0.3), ensure_ascii=False))
        p.send({"type": "device_list"})
        print("list1", json.dumps(p.drain(0.3), ensure_ascii=False))

        print("kbd", json.dumps(inj(p, 0, kbd(1010, [4, 5])), ensure_ascii=False))
        p.send({"type": "get_kbd_state"})
        print("kbdstate", json.dumps(p.drain(0.3), ensure_ascii=False))
        p.send({"type": "device_list"})
        print("list2", json.dumps(p.drain(0.3), ensure_ascii=False))

        info = json.dumps(
            {"code": 715, "message": "ok", "data": json.dumps(
                {"vid": 13652, "pid": 65026, "product": "YSC Box"}, separators=(",", ":"))},
            separators=(",", ":"),
        )
        print("info715", json.dumps(inj(p, 0, info), ensure_ascii=False))
        # 711：CONFIG(9) + KBD IF(9) + EP(7) + MOUSE IF(9)，两个 HID 接口 → 歧义
        cfg = bytes([9, 2, 34, 0, 1, 0, 0, 0x80, 50,
                     9, 4, 0, 0, 1, 3, 1, 1, 0,
                     7, 5, 0x81, 3, 8, 0, 10,
                     9, 4, 1, 0, 1, 3, 1, 2, 0])
        cfg_json = json.dumps(
            {"code": 711, "message": "ok", "data": json.dumps(
                {"total": len(cfg), "offset": 0, "len": len(cfg), "hex": cfg.hex().upper()},
                separators=(",", ":"))},
            separators=(",", ":"),
        )
        print("cfg711", json.dumps(inj(p, 0, cfg_json), ensure_ascii=False))
        p.send({"type": "device_list"})
        print("list3", json.dumps(p.drain(0.3), ensure_ascii=False))

        p.send({"type": "human_guard_begin", "axis": "x", "duration": 3000})
        print("guard_begin", json.dumps(p.drain(0.25), ensure_ascii=False))
        seq = [(1000, 0), (1010, 5), (1020, -5), (1040, 5), (1070, -5), (1100, 5)]
        for t, x in seq:
            inj(p, 0, mouse(0, x, 0, t))
        p.send({"type": "human_guard_status"})
        print("guard_status", json.dumps(p.drain(0.35), ensure_ascii=False))
        p.send({"type": "human_guard_stop"})
        print("guard_stop", json.dumps(p.drain(0.45), ensure_ascii=False))
    finally:
        p.close()


if __name__ == "__main__":
    sys.exit(main())
