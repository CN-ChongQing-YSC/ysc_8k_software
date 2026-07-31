// Program.cs — C# 调用 ysc_sdk.dll 的最小示例
// 编译：csc /platform:x64 YscSdk.cs Program.cs   （64 位进程 → 用 x64 DLL）
//       csc /platform:x86 YscSdk.cs Program.cs   （32 位进程 → 用 Win32 DLL）

using System;
using Ysc;

class Program
{
    static void Main(string[] args)
    {
        var sdk = new YscSdk();
        Console.WriteLine($"YSC SDK {sdk.Version}  DLL={sdk.DllPath}");
        Console.WriteLine("支持波特率: " + string.Join(", ", sdk.SupportedBaudrates));

        var ports = sdk.ListPorts();
        Console.WriteLine($"YSC 设备 ({ports.Count}):");
        foreach (var p in ports)
            Console.WriteLine($"  {p["port"]}  side={p.GetValueOrDefault("side")}  serial={p.GetValueOrDefault("serial")}");

        if (ports.Count == 0)
        {
            Console.WriteLine("(未发现 YSC 设备，结束)");
            return;
        }

        string port = args.Length > 0 ? args[0] : ports[0]["port"];
        try
        {
            using var dev = sdk.Connect(port, 0);   // 0 = 自动探测波特率
            Console.WriteLine($"已连接 {port} @ {dev.Baudrate}");
            Console.WriteLine("版本: " + dev.QueryVersion(1500));
            dev.MouseMove(100, 100, 1);             // 演示：鼠标移动
            Console.WriteLine("手柄映射: " + dev.GamepadGetConfig(1500));
        }
        catch (YscException e)
        {
            Console.WriteLine($"出错: {e.Message}  | {sdk.LastError}");
        }
    }
}
