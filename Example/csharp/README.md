# C# / .NET 调用 ysc_sdk.dll

`YscSdk.cs` 是 P/Invoke 封装类，`Program.cs` 是示例入口。stdcall 调用约定，
位数须与 DLL 一致。

## 编译运行

```bash
# 64 位（对应 ysc_sdk_driver/bin/x64/Release/ysc_sdk.dll）
csc /platform:x64 YscSdk.cs Program.cs
YscDemo.exe            # 自动找 DLL；也可把 ysc_sdk.dll 放到 exe 同目录

# 32 位（对应 bin/Win32/Release/ysc_sdk.dll）
csc /platform:x86 YscSdk.cs Program.cs
```

> `csc.exe` 在 .NET Framework / `Microsoft.Net\Framework64\v4.0.x\` 下；
> .NET Core/5+ 用户改用 `dotnet build` 并把 YscSdk.cs 编进项目即可。

## 用法

```csharp
var sdk = new YscSdk();
using var dev = sdk.Connect("COM7", 0);   // 0=自动探测
Console.WriteLine(dev.QueryVersion(1500));
dev.MouseMove(100, 100, 1);
dev.SwitchBaudrate(921600);
```
