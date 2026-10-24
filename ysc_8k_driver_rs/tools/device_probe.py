#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""多设备槽位探针：挂载失败/参数校验/角色/卸下（无硬件）。"""

from __future__ import annotations

import json
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402


def main():
    p = Pipe()
    try:
        p.send({"type": "device_attach", "dev": 0, "port": "", "baud": 115200})
        print("empty_port", json.dumps(p.drain(0.5), ensure_ascii=False))
        p.send({"type": "device_attach", "dev": 0, "port": "COM237", "baud": 12345})
        print("bad_baud", json.dumps(p.drain(0.5), ensure_ascii=False))
        p.send({"type": "device_attach", "dev": 0, "port": "COM999", "baud": 115200})
        print("bad_port", json.dumps(p.drain(0.8), ensure_ascii=False))
        p.send({"type": "device_attach", "dev": 0, "port": "COM237", "baud": 115200})
        print("no_fw", json.dumps(p.drain(3.0), ensure_ascii=False))
        p.send({"type": "device_list"})
        print("list", json.dumps(p.drain(0.4), ensure_ascii=False))
        p.send({"type": "device_set_baud", "dev": 0, "baud": 230400})
        print("set_baud", json.dumps(p.drain(0.5), ensure_ascii=False))
        p.send({"type": "device_set_role", "dev": 0, "role": "mouse"})
        print("set_role", json.dumps(p.drain(0.4), ensure_ascii=False))
        p.send({"type": "device_detach", "dev": 1})
        print("detach1", json.dumps(p.drain(0.4), ensure_ascii=False))
        p.send({"type": "device_detach", "dev": 0})
        print("detach0", json.dumps(p.drain(0.4), ensure_ascii=False))
    finally:
        p.close()


if __name__ == "__main__":
    sys.exit(main())
