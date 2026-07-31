# Python 调用 ysc_sdk.dll

`ysc_sdk.py` 是对 `ysc_sdk.dll` 的完整 ctypes 绑定（面向对象封装），
`test_dll.py` 是覆盖全部 27 个导出接口的自动化测试。

## 环境要求

- Python 3.7+（已在 **Python 3.14 / 64 位** 验证）
- 只用标准库（ctypes），**无需 pip install 任何包**。可选 `pyserial` 仅用于对比。

## 快速开始

```bash
# 1) 确保已编译出 DLL（见上级 README），Python 64 位会自动找 x64 DLL。

# 2) 只跑离线测试（不接硬件）——测 DLL 加载、串口枚举、错误机制
python test_dll.py

# 3) 接上 YSC 设备后，连接 COM 口做完整测试（自动探测波特率）
python test_dll.py --port COM7

# 4) 指定波特率 + 额外做波特率切换往返测试
python test_dll.py --port COM7 --baud 115200 --baud-test
```

## 编程用法

```python
from ysc_sdk import YscSdk, YscError

sdk = YscSdk()                          # 自动定位 ysc_sdk.dll
print("SDK", sdk.version, sdk.supported_baudrates)
print("YSC 设备:", sdk.list_ports())

try:
    dev = sdk.connect("COM7", 0)        # 0 = 自动探测波特率
    print("连接成功，波特率", dev.baudrate)
    print("设备版本:", dev.query_version(1500))
    dev.mouse_move(100, 100, 1)         # 鼠标移动（即发即弃）
    dev.mouse_button(1, True); dev.mouse_button(1, False)  # 点一下左键
    print("手柄映射:", dev.gamepad_get_config(1500))
    dev.disconnect()
except YscError as e:
    print("出错:", e, "|", sdk.last_error)
```

## 测试覆盖（test_dll.py）

| # | 内容 | 需要硬件 |
|---|---|---|
| 1 | DLL 加载 / 版本 / 支持波特率 / 架构匹配 | 否 |
| 2 | 串口枚举（YSC 设备 + 全部 COM） | 否 |
| 3 | 错误机制（探测不存在端口 → LastError） | 否 |
| 4 | 连接（含自动探测） | 是 |
| 5 | 版本查询（cmd:132，同步等待返回） | 是 |
| 6 | 通用 send_command（同步） | 是 |
| 7 | 鼠标 / 键盘 / 上报状态（NoWait） | 是 |
| 8 | 手柄映射查询（cmd:101） | 是 |
| 9 | 原始字节发送（MAKCU 文本） | 是 |
| 10 | 波特率切换往返（`--baud-test`） | 是 |
| 11 | 断开 / 释放 | 是 |

无硬件时第 4~11 步会自动 SKIP，不影响离线测试结果。
