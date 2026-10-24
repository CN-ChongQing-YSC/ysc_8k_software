#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""kmboxnet 监控包端到端探针：注册监控客户端 → 注入鼠标/键盘 → 收 20B 包。"""

from __future__ import annotations

import json
import socket
import struct
import sys
import time

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402

CMD_CONNECT = 0xAF3C2828
CMD_MONITOR = 0x27388020


def hdr(cmd: int, rand: int = 0x12345678) -> bytes:
    return struct.pack("<IIII", 0x00000001, rand, 1, cmd)


def hx(s: str) -> str:
    return s.encode("utf-8").hex()


def mouse(b: int, x: int, y: int, wh: int = 0) -> str:
    inner = json.dumps({"b": b, "x": x, "y": y, "wh": wh}, separators=(",", ":"))
    return json.dumps({"code": 200, "message": "mouse", "data": inner}, separators=(",", ":"))


def kbd(keys: list[int]) -> str:
    inner = json.dumps({"mod": 2, "n": len(keys), "ov": 0, "k": keys}, separators=(",", ":"))
    return json.dumps({"code": 200, "message": "kbd", "data": inner}, separators=(",", ":"))


def main():
    p = Pipe()
    udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    udp.bind(("127.0.0.1", 5252))
    udp.settimeout(1.0)
    try:
        p.send({"type": "kmnet_start", "port": 5251})
        print("start", json.dumps(p.drain(0.4), ensure_ascii=False))

        udp.sendto(hdr(CMD_CONNECT), ("127.0.0.1", 5251))
        try:
            udp.recvfrom(64)
        except socket.timeout:
            pass

        udp.sendto(hdr(CMD_MONITOR, 5252), ("127.0.0.1", 5251))
        try:
            udp.recvfrom(64)
        except socket.timeout:
            pass

        p.send({"type": "debug_inject_slot", "test": 1, "dev": 0, "rawHex": hx(mouse(3, 10, -5, 1))})
        try:
            pkt, _ = udp.recvfrom(64)
            print("mouse_pkt", pkt.hex())
        except socket.timeout:
            print("mouse_pkt TIMEOUT")
        p.drain(0.2)

        p.send({"type": "debug_inject_slot", "test": 1, "dev": 0, "rawHex": hx(kbd([4, 5]))})
        try:
            pkt, _ = udp.recvfrom(64)
            print("kbd_pkt", pkt.hex())
        except socket.timeout:
            print("kbd_pkt TIMEOUT")
        p.drain(0.2)

        # 关闭监控：不应再收到包
        udp.sendto(hdr(CMD_MONITOR, 0), ("127.0.0.1", 5251))
        try:
            udp.recvfrom(64)
        except socket.timeout:
            pass
        p.send({"type": "debug_inject_slot", "test": 1, "dev": 0, "rawHex": hx(mouse(4, 11, -6, 0))})
        try:
            pkt, _ = udp.recvfrom(64)
            print("off_pkt UNEXPECTED", pkt.hex())
        except socket.timeout:
            print("off_pkt none")
        p.send({"type": "kmnet_stop"})
        print("stop", json.dumps(p.drain(0.6), ensure_ascii=False))
    finally:
        udp.close()
        p.close()


if __name__ == "__main__":
    sys.exit(main())
