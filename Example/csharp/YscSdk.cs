// YscSdk.cs — C# (P/Invoke) 调用 ysc_sdk.dll 的封装
// ----------------------------------------------------------------
// 把 ysc_sdk.dll 的 C ABI 包装成面向对象的 C# 类。C# / .NET 程序无需重复
// 实现串口驱动，直接调用本类即可：列出串口 / 连接 / 切换波特率 / 发命令。
//
// 用法：
//   using Ysc;
//   var sdk = new YscSdk();                 // 自动找 ysc_sdk.dll（x64/x86）
//   Console.WriteLine(sdk.Version);
//   var ports = sdk.ListPorts();
//   using var dev = sdk.Connect("COM7", 0);  // 0=自动探测
//   Console.WriteLine(dev.QueryVersion(1500));
//   dev.MouseMove(100, 100, 1);
//
// 编译（命令行，任选其一）：
//   csc /platform:x64 YscSdk.cs Program.cs   // 注意：程序位数要与 dll 一致
// ----------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

namespace Ysc
{
    public class YscException : Exception
    {
        public YscException(string msg) : base(msg) { }
    }

    public sealed class YscDevice : IDisposable
    {
        private IntPtr _handle;
        private readonly YscSdk _sdk;
        public string Port { get; }
        public uint Baudrate { get; private set; }

        internal YscDevice(YscSdk sdk, IntPtr h, string port, uint baud)
        { _sdk = sdk; _handle = h; Port = port; Baudrate = baud; }

        public bool IsConnected => _handle != IntPtr.Zero && Native.Ysc_IsConnected(_handle);

        // —— 通用 ——
        public string SendCommand(string json, int timeoutMs = 1000)
        {
            if (_handle == IntPtr.Zero) throw new YscException("device closed");
            var sb = new StringBuilder(8192);
            int n = Native.Ysc_SendCommand(_handle, json, timeoutMs, sb, sb.Capacity);
            if (n >= 0) return sb.ToString();
            if (n == -2) throw new YscException($"timeout ({timeoutMs} ms): {_sdk.LastError}");
            throw new YscException($"send_command failed: {_sdk.LastError}");
        }

        public void SendCommandNoWait(string json)
        {
            if (_handle == IntPtr.Zero) throw new YscException("device closed");
            if (!Native.Ysc_SendCommandNoWait(_handle, json))
                throw new YscException($"send_command_no_wait failed: {_sdk.LastError}");
        }

        public void SendRaw(byte[] data)
        {
            if (_handle == IntPtr.Zero) throw new YscException("device closed");
            if (!Native.Ysc_SendRaw(_handle, data, data.Length))
                throw new YscException($"send_raw failed: {_sdk.LastError}");
        }

        public string QueryVersion(int timeoutMs = 1000) => SendCommand("{\"cmd\":132}", timeoutMs);

        // —— 便捷命令 ——
        public void MouseMove(int x, int y, int steps = 1)
            => SendCommandNoWait($"{{\"cmd\":30,\"x\":{x},\"y\":{y},\"c\":{steps}}}");
        public void MouseMoveTow(int x, int y, int steps = 1)
            => SendCommandNoWait($"{{\"cmd\":31,\"x\":{x},\"y\":{y},\"c\":{steps}}}");
        public void MouseButton(byte buttonMask, bool pressed)
            => SendCommandNoWait($"{{\"cmd\":33,\"b\":{buttonMask},\"s\":{pressed?1:0}}}");
        public void KeyboardKey(byte keycode, bool down)
            => SendCommandNoWait($"{{\"cmd\":45,\"kc\":{keycode},\"down\":{down?1:0}}}");
        public void KeyboardReleaseAll() => SendCommandNoWait("{\"cmd\":46}");
        public void UploadStatus(bool enable)
            => SendCommandNoWait($"{{\"cmd\":34,\"status\":{enable?1:0}}}");
        public void JumpIAP() => SendCommandNoWait("{\"cmd\":50}");
        public string GamepadGetConfig(int timeoutMs = 1500)
            => SendCommand("{\"cmd\":101}", timeoutMs);
        public void GamepadSetConfig(string configJson)
            => SendCommandNoWait("{\"cmd\":100,\"data\":" + configJson + "}");
        public void GamepadEnable(bool on)
            => SendCommandNoWait($"{{\"cmd\":102,\"on\":{on?1:0}}}");
        public void GamepadReset() => SendCommandNoWait("{\"cmd\":104}");

        public void SwitchBaudrate(uint newBaud)
        {
            if (!Native.Ysc_SwitchBaudrate(_handle, newBaud))
                throw new YscException($"switch_baudrate({newBaud}) failed: {_sdk.LastError}");
            Baudrate = newBaud;
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero) { Native.Ysc_Disconnect(_handle); _handle = IntPtr.Zero; }
        }
    }

    public sealed class YscSdk
    {
        public string DllPath { get; }
        public string Version => PtrToUtf8(Native.Ysc_SdkVersion());
        public string LastError => PtrToUtf8(Native.Ysc_LastError());

        public uint[] SupportedBaudrates
        {
            get
            {
                int cnt = 0;
                IntPtr p = Native.Ysc_SupportedBaudrates(ref cnt);
                var arr = new uint[cnt];
                for (int i = 0; i < cnt; i++) arr[i] = (uint)Marshal.ReadInt32(p, i * 4);
                return arr;
            }
        }

        public YscSdk(string dllPath = null)
        {
            DllPath = dllPath ?? FindDll();
            Native.Load(DllPath);   // 触发 P/Invoke 绑定到指定路径
        }

        // —— 枚举 ——
        public List<Dictionary<string, string>> ListPorts() => ParseList(Native.Ysc_ListPorts);
        public List<Dictionary<string, string>> ListAllComPorts() => ParseList(Native.Ysc_ListAllComPorts);

        private List<Dictionary<string, string>> ParseList(Func<StringBuilder, int, int> fn)
        {
            int size = 8192;
            for (int i = 0; i < 4; i++)
            {
                var sb = new StringBuilder(size);
                int need = fn(sb, size);
                if (need < 0) throw new YscException($"list failed: {LastError}");
                if (need < size - 1) return ParseJsonList(sb.ToString());
                size = need + 16;
            }
            throw new YscException("list buffer kept overflowing");
        }

        // —— 连接 ——
        public uint DetectBaudrate(string port) => Native.Ysc_DetectBaudrate(port);

        public YscDevice Connect(string port, uint baud = 0)
        {
            var err = new StringBuilder(512);
            IntPtr h = Native.Ysc_Connect(port, baud, err, 512);
            if (h == IntPtr.Zero)
                throw new YscException($"connect({port},{baud}) failed: {err}");
            return new YscDevice(this, h, port, Native.Ysc_GetBaudrate(h));
        }

        // —— 工具 ——
        private static string PtrToUtf8(IntPtr p)
            => p == IntPtr.Zero ? "" : Marshal.PtrToStringAnsi(p);

        // 极简 JSON 数组解析（仅解析本 DLL 返回的 [{"port":"...","desc":"..."}]）
        private static List<Dictionary<string, string>> ParseJsonList(string json)
        {
            var result = new List<Dictionary<string, string>>();
            if (string.IsNullOrWhiteSpace(json)) return result;
            // 用 System.Text.Json 更稳；这里手写极简解析以避免额外依赖。
            int i = 0;
            while (i < json.Length)
            {
                int objStart = json.IndexOf('{', i);
                if (objStart < 0) break;
                int objEnd = json.IndexOf('}', objStart);
                if (objEnd < 0) break;
                string obj = json.Substring(objStart, objEnd - objStart + 1);
                var dict = new Dictionary<string, string>();
                int p = 1;
                while (p < obj.Length)
                {
                    int k1 = obj.IndexOf('"', p);
                    if (k1 < 0) break;
                    int k2 = obj.IndexOf('"', k1 + 1);
                    int c1 = obj.IndexOf(':', k2 + 1);
                    int v1 = obj.IndexOf('"', c1 + 1);
                    if (v1 < 0) break; // 值不是字符串，跳过
                    int v2 = obj.IndexOf('"', v1 + 1);
                    if (v2 < 0) break;
                    dict[obj.Substring(k1 + 1, k2 - k1 - 1)] = obj.Substring(v1 + 1, v2 - v1 - 1);
                    p = v2 + 1;
                }
                result.Add(dict);
                i = objEnd + 1;
            }
            return result;
        }

        private static string FindDll()
        {
            string here = AppDomain.CurrentDomain.BaseDirectory;
            string root = System.IO.Path.GetFullPath(System.IO.Path.Combine(here, "..", ".."));
            string arch = Environment.Is64BitProcess ? "x64" : "Win32";
            string[] cands = {
                System.IO.Path.Combine(root, "ysc_8k_driver", "bin", arch, "Release", "ysc_sdk.dll"),
                System.IO.Path.Combine(here, "ysc_sdk.dll"),
            };
            foreach (var c in cands) if (System.IO.File.Exists(c)) return c;
            throw new YscException("找不到 ysc_sdk.dll，请先编译或用 new YscSdk(路径) 指定");
        }
    }

    // —— 原生 P/Invoke 声明（stdcall）——
    internal static class Native
    {
        private static IntPtr _dll = IntPtr.Zero;

        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Ansi)]
        private static extern IntPtr LoadLibrary(string lpLibFileName);

        public static void Load(string path)
        {
            IntPtr h = LoadLibrary(path);
            if (h == IntPtr.Zero)
                throw new YscException($"LoadLibrary({path}) 失败，错误码={Marshal.GetLastWin32Error()}");
            _dll = h; // 让 DLL 留在进程里，后续 P/Invoke("ysc_sdk.dll") 即可命中
        }

        const string D = "ysc_sdk.dll";

        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern IntPtr Ysc_SdkVersion();
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern IntPtr Ysc_LastError();
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern IntPtr Ysc_SupportedBaudrates(ref int count);

        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern int Ysc_ListPorts(StringBuilder outBuf, int bufSize);
        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern int Ysc_ListAllComPorts(StringBuilder outBuf, int bufSize);

        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern IntPtr Ysc_Connect(string portName, uint baudRate,
                                                StringBuilder errBuf, int errBufSize);
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern void Ysc_Disconnect(IntPtr dev);
        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern uint Ysc_DetectBaudrate(string portName);
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern int Ysc_IsConnected(IntPtr dev);
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern uint Ysc_GetBaudrate(IntPtr dev);

        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern int Ysc_SwitchBaudrate(IntPtr dev, uint newBaud);
        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern int Ysc_SendCommand(IntPtr dev, string json, int timeoutMs,
                                                 StringBuilder outBuf, int bufSize);
        [DllImport(D, CallingConvention = CallingConvention.StdCall, CharSet = CharSet.Ansi)]
        public static extern int Ysc_SendCommandNoWait(IntPtr dev, string json);
        [DllImport(D, CallingConvention = CallingConvention.StdCall)]
        public static extern int Ysc_SendRaw(IntPtr dev, byte[] data, int len);
    }
}
