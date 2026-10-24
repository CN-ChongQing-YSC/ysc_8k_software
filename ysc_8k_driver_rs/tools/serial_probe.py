#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""串口层探针：在一个空闲 COM 口上验证打开/状态/切换失败/断开。

不需要真设备：显式波特率连接会成功（与 C++ 一样不做握手），
切换波特率会因无应答而失败并回推 baudrate_failed。

用法：python tools/serial_probe.py [COM口]
"""

from __future__ import annotations

import json
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402


def main():
    port = sys.argv[1] if len(sys.argv) > 1 else "COM237"
    p = Pipe()
    try:
        p.send({"type": "serial_connect", "port": port, "baud": 115200})
        print("connect", json.dumps(p.drain(0.8), ensure_ascii=False))
        p.send({"type": "get_state"})
        print("state1", json.dumps(p.drain(0.3), ensure_ascii=False))
        p.send({"type": "get_monitor"})
        print("monitor", json.dumps(p.drain(0.3), ensure_ascii=False))

        p.send({"type": "switch_baudrate", "baud": 230400})
        print("switch", json.dumps(p.drain(2.0), ensure_ascii=False))
        p.send({"type": "get_state"})
        print("state2", json.dumps(p.drain(0.3), ensure_ascii=False))

        p.send({"type": "serial_disconnect"})
        print("disconnect", json.dumps(p.drain(0.5), ensure_ascii=False))
        p.send({"type": "get_state"})
        print("state3", json.dumps(p.drain(0.3), ensure_ascii=False))

        # 自动探测失败路径（9 档 × 200ms，最长约 2s）
        p.send({"type": "serial_connect", "port": port, "baud": 0})
        print("autodetect", json.dumps(p.drain(3.0), ensure_ascii=False))
    finally:
        p.close()


if __name__ == "__main__":
    sys.exit(main())
