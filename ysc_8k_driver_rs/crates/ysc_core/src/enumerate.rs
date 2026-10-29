//! 串口 / 设备枚举。
//!
//!   * [`enum_com_ports`] — 注册表 `HARDWARE\DEVICEMAP\SERIALCOMM`（与 `main.cpp` 同源）。
//!   * [`enum_towmcu_ports`] — SetupAPI `GUID_DEVINTERFACE_COMPORT`，按
//!     `VID_1A86 & PID_FE0C` 过滤并读取 USB iSerialNumber（与 `towmcu_cdc.cpp` 一致）。
//!   * [`detect_ch343_driver_status`] — 枚举所有 present 设备节点，按 `VID_1A86`
//!     过滤并用 `CM_Get_DevNode_Status` 取 problem 码。

use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_Status, CM_Get_Device_IDW, CM_Get_Parent, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
    SetupDiOpenDevRegKey, CM_DEVNODE_STATUS_FLAGS, CM_PROB, CR_SUCCESS, DICS_FLAG_GLOBAL,
    DIGCF_ALLCLASSES, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, DIREG_DEV, HDEVINFO,
    SETUP_DI_REGISTRY_PROPERTY, SPDRP_DEVICEDESC, SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID,
    SP_DEVINFO_DATA,
};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_READ, REG_SZ, REG_VALUE_TYPE,
};

use crate::util::from_wide;

const TOWMCU_VID: &str = "VID_1A86";
const TOWMCU_PID: &str = "PID_FE0C";

/// {86E0D1E0-8089-11D0-9CE4-08003E301F73} — GUID_DEVINTERFACE_COMPORT。
fn comport_guid() -> GUID {
    GUID::from_values(
        0x86e0_d1e0,
        0x8089,
        0x11d0,
        [0x9c, 0xe4, 0x08, 0x00, 0x3e, 0x30, 0x1f, 0x73],
    )
}

/// 一个 CDC COM 口。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TowmcuPort {
    pub port: String,
    pub serial: String,
    pub description: String,
    /// `Left` / `Right` / `IAP` / 空。
    pub side: String,
}

/// YSC V3 USB CDC 的 iSerialNumber（IAP/APP 刻意共用同一序列号）。
/// `XFBOX-CDC` 是旧 XF Box 批次，升级/CDC 行为与 V3 相同，按 V3 兼容处理。
pub const YSC_V3_CDC_SERIAL: &str = "YSCV3-CDC";
pub const XFBOX_CDC_SERIAL: &str = "XFBOX-CDC";

/// 序列号是否属于 YSC V3 / 旧 XF Box 控制口。
///
/// 这两个 CDC 口没有真实波特率：主机侧设置任何 baud 都只影响 DCB 显示，
/// USB 数据始终按全速/高速传输。驱动据此放开波特率白名单。
pub fn is_v3_serial(serial: &str) -> bool {
    serial == YSC_V3_CDC_SERIAL || serial == XFBOX_CDC_SERIAL
}

/// USB iSerialNumber → side 标签。
///
/// `YSCV3-CDC` 是 YSC V3 控制口（IAP/APP 刻意共用同一序列号）；
/// `XFBOX-CDC` 是旧 XF Box 批次，升级流程与 V3 相同，归入 `V3` 便于前端识别。
pub fn towmcu_side_from_serial(serial: &str) -> &'static str {
    match serial {
        "TOWMCULEFT" => "Left",
        "TOWMCURIGHT" => "Right",
        "TOWMCUIAP" => "IAP",
        _ if is_v3_serial(serial) => "V3",
        _ => "",
    }
}

/// 当前 COM 口是否是 YSC V3 USB CDC（按 SetupAPI 枚举到的 USB 序列号判断）。
///
/// 只应在连接/切换等低频路径调用：每次都会走一遍 SetupAPI。
pub fn is_ysc_v3_port(port: &str) -> bool {
    enum_towmcu_ports()
        .iter()
        .any(|p| p.port.eq_ignore_ascii_case(port) && is_v3_serial(&p.serial))
}

fn com_number(name: &str) -> u32 {
    if name.len() > 3 && name[..3].eq_ignore_ascii_case("COM") {
        name[3..].parse().unwrap_or(0)
    } else {
        0
    }
}

/// 枚举注册表里的全部 COM 口（按 COM 号升序）。
pub fn enum_com_ports() -> Vec<String> {
    let mut ports = Vec::new();
    let mut hkey = HKEY::default();
    let subkey = crate::util::wide("HARDWARE\\DEVICEMAP\\SERIALCOMM");
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_READ,
            &mut hkey,
        )
    };
    if rc != ERROR_SUCCESS {
        return ports;
    }

    let mut index = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let mut data = [0u8; 512];
        let mut data_len = data.len() as u32;
        let mut ty = 0u32;
        let rc = unsafe {
            RegEnumValueW(
                hkey,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut ty),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if rc != ERROR_SUCCESS {
            break;
        }
        if ty == REG_SZ.0 && data_len >= 2 {
            let words: Vec<u16> = data[..data_len as usize]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let s = from_wide(&words);
            if !s.is_empty() {
                ports.push(s);
            }
        }
        index += 1;
    }
    unsafe {
        let _ = RegCloseKey(hkey);
    }

    ports.sort_by(|a, b| {
        let (na, nb) = (com_number(a), com_number(b));
        if na != 0 && nb != 0 {
            na.cmp(&nb)
        } else {
            a.cmp(b)
        }
    });
    ports
}

fn hw_id_matches_vid_pid(hw_id: &str) -> bool {
    hw_id.contains(TOWMCU_VID) && hw_id.contains(TOWMCU_PID)
}

/// 硬件/实例 ID 的最后一个路径节点即 USB iSerialNumber（`USB\VID_..\TOWMCULEFT`）。
/// Windows 生成的实例 ID 含 `&`，返回空让调用方继续向上找。
fn extract_serial_from_hw_id(hw_id: &str) -> String {
    let node = match hw_id.rsplit('\\').next() {
        Some(n) => n,
        None => return String::new(),
    };
    if node.is_empty() || node.contains('&') {
        return String::new();
    }
    node.to_string()
}

fn device_id(devinst: u32) -> Option<String> {
    let mut buf = [0u16; 512];
    let cr = unsafe { CM_Get_Device_IDW(devinst, &mut buf, 0) };
    if cr != CR_SUCCESS {
        return None;
    }
    Some(from_wide(&buf))
}

/// 在设备树（本节点 + 最多 5 级父节点）里找 VID/PID 命中，并取第一个带序列号的节点。
fn match_towmcu_in_tree(devinst: u32) -> (bool, String) {
    let mut best_serial = String::new();
    let mut matched = false;
    let mut cur = devinst;
    for _ in 0..6 {
        if cur == 0 {
            break;
        }
        if let Some(id) = device_id(cur) {
            if hw_id_matches_vid_pid(&id) {
                matched = true;
                let s = extract_serial_from_hw_id(&id);
                if !s.is_empty() {
                    best_serial = s;
                    break;
                }
            }
        }
        let mut parent = 0u32;
        let cr = unsafe { CM_Get_Parent(&mut parent, cur, 0) };
        if cr != CR_SUCCESS || parent == 0 {
            break;
        }
        cur = parent;
    }
    (matched, best_serial)
}

fn read_port_name(hdev: HDEVINFO, did: &SP_DEVINFO_DATA) -> String {
    let key =
        unsafe { SetupDiOpenDevRegKey(hdev, did, DICS_FLAG_GLOBAL.0, 0, DIREG_DEV, KEY_READ.0) };
    let key = match key {
        Ok(k) => k,
        Err(_) => return String::new(),
    };
    let name = crate::util::wide("PortName");
    let mut buf = [0u16; 64];
    let mut ty = REG_VALUE_TYPE::default();
    let mut sz = std::mem::size_of_val(&buf) as u32;
    let rc = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut sz),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if rc != ERROR_SUCCESS || ty != REG_SZ {
        return String::new();
    }
    from_wide(&buf)
}

fn registry_property(
    hdev: HDEVINFO,
    did: &SP_DEVINFO_DATA,
    prop: SETUP_DI_REGISTRY_PROPERTY,
) -> String {
    let mut buf = [0u8; 512];
    let ok =
        unsafe { SetupDiGetDeviceRegistryPropertyW(hdev, did, prop, None, Some(&mut buf), None) };
    if ok.is_err() {
        return String::new();
    }
    let words: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    from_wide(&words)
}

/// 枚举 present 的 towmcu CDC 口（VID 0x1A86 / PID 0xFE0C），按 COM 号升序。
pub fn enum_towmcu_ports() -> Vec<TowmcuPort> {
    let mut out = Vec::new();
    let guid = comport_guid();
    let hdev = match unsafe {
        SetupDiGetClassDevsW(
            Some(&guid),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    } {
        Ok(h) => h,
        Err(_) => return out,
    };

    let mut did = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    let mut i = 0u32;
    loop {
        if unsafe { SetupDiEnumDeviceInfo(hdev, i, &mut did) }.is_err() {
            break;
        }
        i += 1;

        let (matched, serial) = match_towmcu_in_tree(did.DevInst);
        if !matched {
            continue;
        }
        let port = read_port_name(hdev, &did);
        if port.is_empty() {
            continue;
        }

        let mut description = registry_property(hdev, &did, SPDRP_FRIENDLYNAME);
        if description.is_empty() {
            description = registry_property(hdev, &did, SPDRP_DEVICEDESC);
        }
        if description.is_empty() {
            description = serial.clone();
        }

        out.push(TowmcuPort {
            side: towmcu_side_from_serial(&serial).to_string(),
            port,
            serial,
            description,
        });
    }
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(hdev);
    }

    out.sort_by_key(|p| com_number(&p.port));
    out
}

/// 任意 COM 口（SDK `Ysc_ListAllComPorts` 用）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnyComPort {
    pub port: String,
    pub description: String,
}

/// 枚举 present 的全部 COM 口（不区分厂商），带友好名称。
pub fn enum_all_com_ports() -> Vec<AnyComPort> {
    let mut out = Vec::new();
    let guid = comport_guid();
    let hdev = match unsafe {
        SetupDiGetClassDevsW(
            Some(&guid),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    } {
        Ok(h) => h,
        Err(_) => return out,
    };

    let mut did = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    let mut i = 0u32;
    loop {
        if unsafe { SetupDiEnumDeviceInfo(hdev, i, &mut did) }.is_err() {
            break;
        }
        i += 1;

        let port = read_port_name(hdev, &did);
        if port.is_empty() {
            continue;
        }
        let mut description = registry_property(hdev, &did, SPDRP_FRIENDLYNAME);
        if description.is_empty() {
            description = registry_property(hdev, &did, SPDRP_DEVICEDESC);
        }
        out.push(AnyComPort { port, description });
    }
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(hdev);
    }
    out.sort_by_key(|p| com_number(&p.port));
    out
}

/// 一个 present 但驱动异常的 WCH/CH34x 设备。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WchDeviceProblem {
    pub pid_hex: String,
    pub friendly_name: String,
    pub problem_code: u32,
}

/// WCH/CH34x 驱动健康度。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ch343DriverStatus {
    pub any_present: bool,
    pub any_problem: bool,
    pub problems: Vec<WchDeviceProblem>,
}

fn hw_id_is_wch(hw_id: &str) -> bool {
    hw_id.contains(TOWMCU_VID)
}

fn extract_pid_hex(hw_id: &str) -> String {
    let p = match hw_id.find("PID_") {
        Some(p) => p + 4,
        None => return String::new(),
    };
    hw_id[p..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .take(4)
        .collect()
}

/// 检测 VID 1A86 设备的驱动状态（覆盖 inbox usbser 失效与 CH343 驱动缺失）。
pub fn detect_ch343_driver_status() -> Ch343DriverStatus {
    let mut st = Ch343DriverStatus::default();
    let hdev = match unsafe {
        SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
    } {
        Ok(h) => h,
        Err(_) => return st,
    };

    let mut did = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    let mut i = 0u32;
    loop {
        if unsafe { SetupDiEnumDeviceInfo(hdev, i, &mut did) }.is_err() {
            break;
        }
        i += 1;

        let mut hwbuf = [0u8; 512];
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                hdev,
                &did,
                SPDRP_HARDWAREID,
                None,
                Some(&mut hwbuf),
                None,
            )
        };
        if ok.is_err() {
            continue;
        }
        // REG_MULTI_SZ：逐条以 NUL 分隔，双 NUL 结束。
        let words: Vec<u16> = hwbuf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let mut hw_ids: Vec<String> = Vec::new();
        let mut start = 0usize;
        for (idx, &w) in words.iter().enumerate() {
            if w == 0 {
                if idx > start {
                    hw_ids.push(from_wide(&words[start..idx]));
                }
                if idx + 1 < words.len() && words[idx + 1] == 0 {
                    break;
                }
                start = idx + 1;
            }
        }
        let matched = hw_ids.iter().find(|s| hw_id_is_wch(s));
        let Some(hw) = matched else { continue };

        st.any_present = true;

        let mut status = CM_DEVNODE_STATUS_FLAGS::default();
        let mut problem = CM_PROB::default();
        let cr = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, did.DevInst, 0) };
        let problem_code = if cr != CR_SUCCESS {
            0xFFFF_FFFF
        } else {
            problem.0
        };
        if problem.0 != 0 || cr != CR_SUCCESS {
            st.any_problem = true;
            let mut name = registry_property(hdev, &did, SPDRP_FRIENDLYNAME);
            if name.is_empty() {
                name = registry_property(hdev, &did, SPDRP_DEVICEDESC);
            }
            st.problems.push(WchDeviceProblem {
                pid_hex: extract_pid_hex(hw),
                friendly_name: name,
                problem_code,
            });
        }
    }
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(hdev);
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_extraction() {
        assert_eq!(
            extract_serial_from_hw_id("USB\\VID_1A86&PID_FE0C\\TOWMCULEFT"),
            "TOWMCULEFT"
        );
        assert_eq!(
            extract_serial_from_hw_id("USB\\VID_1A86&PID_FE0C\\6&abcd1234&0"),
            ""
        );
    }

    #[test]
    fn side_mapping() {
        assert_eq!(towmcu_side_from_serial("TOWMCULEFT"), "Left");
        assert_eq!(towmcu_side_from_serial("TOWMCURIGHT"), "Right");
        assert_eq!(towmcu_side_from_serial("TOWMCUIAP"), "IAP");
        assert_eq!(towmcu_side_from_serial("YSCV3-CDC"), "V3");
        assert_eq!(towmcu_side_from_serial("XFBOX-CDC"), "V3");
        assert_eq!(towmcu_side_from_serial("X"), "");
    }

    #[test]
    fn v3_serial_detection() {
        assert!(is_v3_serial("YSCV3-CDC"));
        assert!(is_v3_serial("XFBOX-CDC"));
        assert!(!is_v3_serial("TOWMCULEFT"));
        assert!(!is_v3_serial("TOWMCURIGHT"));
        assert!(!is_v3_serial(""));
    }

    #[test]
    fn pid_extraction() {
        assert_eq!(extract_pid_hex("USB\\VID_1A86&PID_FE0C"), "FE0C");
    }
}
