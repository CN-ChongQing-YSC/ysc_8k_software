#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""kmboxnet UDP 探针：发一组命令，收 ACK，再从管道读 kmnet_stat。

用于 Rust 驱动与 C++ 参照驱动的差分对照。用法：
  python tools/kmnet_probe.py
前提：驱动已启动、`kmnet_start` 已在 5251 上监听（本脚本自己发 start）。
"""

from __future__ import annotations

import json
import socket
import struct
import sys
import time

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402

CMD_CONNECT = 0xAF3C2828
CMD_MOUSE_MOVE = 0xAEDE7345
CMD_MOUSE_LEFT = 0x9823AE8D
CMD_MOUSE_MIDDLE = 0x97A3AE8D
CMD_MOUSE_RIGHT = 0x238D8212
CMD_MOUSE_WHEEL = 0xFFEEAD38
CMD_KEYBOARD_ALL = 0x123C2C2F
CMD_MONITOR = 0x27388020
CMD_MOUSE_AUTOMOVE = 0xAEDE7346
CMD_BAZER_MOVE = 0xA238455A
CMD_REBOOT = 0xAA8855AA
CMD_MASK_MOUSE = 0x23234343
CMD_UNMASK_ALL = 0x23344343

HDR = struct.Struct("<IIII")


def hdr(cmd, mac=1, rand=0, idx=1):
    return HDR.pack(mac, rand, idx, cmd)


def mouse_payload(button=0, x=0, y=0, wheel=0):
    # soft_mouse_t: 4*u32 + 10*int32 = 56
    return struct.pack("<Iii i", button, x, y, wheel) + b"\x00" * 40


def kb_payload(ctrl=0, keys=()):
    arr = list(keys) + [0] * (10 - len(keys))
    return struct.pack("<BB", ctrl, 0) + bytes(arr)


def send_recv(sock, data, timeout=1.0):
    sock.sendto(data, ("127.0.0.1", 5251))
    sock.settimeout(timeout)
    try:
        ack, _ = sock.recvfrom(2048)
        return list(HDR.unpack(ack[:16]))
    except socket.timeout:
        return None


def main():
    pipe = Pipe()
    try:
        pipe.send({"type": "kmnet_start", "port": 5251})
        evs = pipe.drain(0.5)
        print("start", json.dumps(evs, ensure_ascii=False))

        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind(("127.0.0.1", 0))
        cases = [
            ("connect", hdr(CMD_CONNECT, rand=0, idx=3)),
            ("move", hdr(CMD_MOUSE_MOVE, rand=0, idx=4) + mouse_payload(0, 10, -5, 0)),
            ("automove", hdr(CMD_MOUSE_AUTOMOVE, rand=160, idx=5) + mouse_payload(0, 3, 4, 0)),
            ("beizer", hdr(CMD_BAZER_MOVE, rand=160, idx=6) + mouse_payload(0, 2, 2, 0)),
            ("left", hdr(CMD_MOUSE_LEFT, idx=7) + mouse_payload(1, 0, 0, 0)),
            ("right", hdr(CMD_MOUSE_RIGHT, idx=8) + mouse_payload(2, 0, 0, 0)),
            ("middle", hdr(CMD_MOUSE_MIDDLE, idx=9) + mouse_payload(4, 0, 0, 0)),
            ("wheel", hdr(CMD_MOUSE_WHEEL, idx=10) + mouse_payload(0, 0, 0, 3)),
            ("all", hdr(CMD_MOUSE_WHEEL, idx=11) + mouse_payload(3, 5, -2, -1)),
            ("kbd", hdr(CMD_KEYBOARD_ALL, idx=12) + kb_payload(0x02, (0x04, 0x05))),
            ("mask", hdr(CMD_MASK_MOUSE, rand=0x1F, idx=13)),
            ("unmask", hdr(CMD_UNMASK_ALL, rand=0, idx=14)),
            ("monitor_on", hdr(CMD_MONITOR, rand=5252, idx=15)),
            ("monitor_off", hdr(CMD_MONITOR, rand=0, idx=16)),
            ("reboot", hdr(CMD_REBOOT, idx=17)),
            ("unsupported", hdr(0xDEADBEEF, idx=18)),
        ]
        for name, pkt in cases:
            print("ack", name, send_recv(sock, pkt))

        time.sleep(0.2)
        pipe.send({"type": "get_kmnet_stat"})
        stat = pipe.drain(0.5)
        print("stat", json.dumps(stat, ensure_ascii=False, sort_keys=True))

        pipe.send({"type": "kmnet_stop"})
        print("stop", json.dumps(pipe.drain(0.7), ensure_ascii=False, sort_keys=True))
        sock.close()
    finally:
        pipe.close()


if __name__ == "__main__":
    sys.exit(main())
