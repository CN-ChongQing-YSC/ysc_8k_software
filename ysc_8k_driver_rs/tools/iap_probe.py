#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""IAP v1/v2 错误路径探针（无硬件）。"""

from __future__ import annotations

import json
import os
import sys
import tempfile

sys.path.insert(0, __file__.rsplit("\\", 1)[0])
from pipe_probe import Pipe  # noqa: E402


def main():
    fw = tempfile.NamedTemporaryFile(delete=False, suffix=".bin")
    fw.write(b"\xA5" * 120)
    fw.close()
    missing = os.path.join(tempfile.gettempdir(), "ysc_no_such_fw_12345.bin")
    try:
        p = Pipe()
        try:
            p.send({"type": "iap_start", "path": missing})
            print("v1_missing", json.dumps(p.drain(0.6), ensure_ascii=False))
            p.send({"type": "iap_start", "path": fw.name})
            print("v1_noport", json.dumps(p.drain(1.0), ensure_ascii=False))
            p.send({"type": "iap_start_mem", "data": "not-base64!!", "size": 12})
            print("v1_badb64", json.dumps(p.drain(0.6), ensure_ascii=False))

            p.send({"type": "towmcu_start", "port": "COM237", "path": missing})
            print("v2_missing", json.dumps(p.drain(0.8), ensure_ascii=False))
            p.send({"type": "towmcu_start_mem", "port": "COM237", "data": "not-base64!!"})
            print("v2_badb64", json.dumps(p.drain(0.8), ensure_ascii=False))
            p.send({"type": "towmcu_query_version", "port": "COM999"})
            print("v2_ver_badport", json.dumps(p.drain(0.8), ensure_ascii=False))
            p.send({"type": "towmcu_enter_iap", "port": "COM999"})
            print("v2_enter_badport", json.dumps(p.drain(0.8), ensure_ascii=False))
            p.send({"type": "towmcu_query_version", "port": "COM237"})
            print("v2_ver_idle", json.dumps(p.drain(2.0), ensure_ascii=False))
        finally:
            p.close()
    finally:
        try:
            os.unlink(fw.name)
        except OSError:
            pass


if __name__ == "__main__":
    sys.exit(main())
