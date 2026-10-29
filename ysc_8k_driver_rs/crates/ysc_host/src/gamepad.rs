//! PC 侧手柄（Windows XInput）轮询与震动。
//!
//! WebView2 不提供 Gamepad API（实测 `navigator.getGamepads()` 全为 null），
//! 所以 Tauri 测试器改为订阅本模块通过 XInput 读到的真实 Windows 手柄状态。
//! 输出形状对齐 Web Gamepad API 的 standard mapping，前端无需区分来源：
//!   axes    [LX, LY, RX, RY]（-1..1，Y 向下为正，与浏览器一致）
//!   buttons 17 项 {pressed,touched,value}
//!   id/mapping/timestamp/connected/vibrationActuator

use serde_json::Value;
use windows::Win32::UI::Input::XboxController::{
    XInputGetState, XInputSetState, XINPUT_STATE, XINPUT_VIBRATION,
};

/// XInput 最多 4 个用户槽位。
pub const MAX_PADS: u32 = 4;

fn axis(v: i16) -> f64 {
    (v as f64 / 32767.0).clamp(-1.0, 1.0)
}

fn button(buttons: u16, mask: u16, value: f64) -> Value {
    let v = if mask != 0 {
        if (buttons & mask) != 0 { 1.0 } else { 0.0 }
    } else {
        value
    };
    serde_json::json!({
        "pressed": v > 0.0,
        "touched": v > 0.0,
        "value": v,
    })
}

fn snapshot(index: u32, state: &XINPUT_STATE) -> Value {
    let g = state.Gamepad;
    let b = g.wButtons.0;
    let lt = g.bLeftTrigger as f64 / 255.0;
    let rt = g.bRightTrigger as f64 / 255.0;

    let buttons = vec![
        button(b, 0x1000, 0.0), // 0  A
        button(b, 0x2000, 0.0), // 1  B
        button(b, 0x4000, 0.0), // 2  X
        button(b, 0x8000, 0.0), // 3  Y
        button(b, 0x0100, 0.0), // 4  LB
        button(b, 0x0200, 0.0), // 5  RB
        button(b, 0, lt),       // 6  LT
        button(b, 0, rt),       // 7  RT
        button(b, 0x0020, 0.0), // 8  Back
        button(b, 0x0010, 0.0), // 9  Start
        button(b, 0x0040, 0.0), // 10 L3
        button(b, 0x0080, 0.0), // 11 R3
        button(b, 0x0001, 0.0), // 12 D-pad up
        button(b, 0x0002, 0.0), // 13 D-pad down
        button(b, 0x0004, 0.0), // 14 D-pad left
        button(b, 0x0008, 0.0), // 15 D-pad right
        button(b, 0, 0.0),      // 16 Guide (XInputGetState 不提供)
    ];

    serde_json::json!({
        "index": index,
        "id": format!("Xbox 360 Controller (XInput #{})", index + 1),
        "connected": true,
        "mapping": "standard",
        "timestamp": ysc_core::util::now_ms() as f64,
        "axes": [
            axis(g.sThumbLX),
            -axis(g.sThumbLY),
            axis(g.sThumbRX),
            -axis(g.sThumbRY),
        ],
        "buttons": buttons,
        "vibrationActuator": true,
        "hapticActuators": [true],
    })
}

/// 读 4 个 XInput 槽位，返回事件体（不含外层大括号）：
/// `"pads":[null,{...},null,null]`
pub fn poll_body() -> String {
    let mut pads: Vec<Value> = Vec::with_capacity(MAX_PADS as usize);
    for i in 0..MAX_PADS {
        let mut state = XINPUT_STATE::default();
        let rc = unsafe { XInputGetState(i, &mut state) };
        if rc == 0 {
            pads.push(snapshot(i, &state));
        } else {
            pads.push(Value::Null);
        }
    }
    format!(
        "\"pads\":{}",
        serde_json::to_string(&pads).unwrap_or_else(|_| "[]".to_string())
    )
}

/// 设置手柄震动（左右马达 0..65535）。index 为 XInput 用户槽位。
pub fn vibrate(index: u32, left: u16, right: u16) -> bool {
    if index >= MAX_PADS {
        return false;
    }
    let vib = XINPUT_VIBRATION {
        wLeftMotorSpeed: left,
        wRightMotorSpeed: right,
    };
    unsafe { XInputSetState(index, &vib) == 0 }
}
