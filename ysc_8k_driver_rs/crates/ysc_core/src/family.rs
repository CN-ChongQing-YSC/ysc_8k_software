//! CDC 固件族识别。
//!
//! YSC 8K 现有两类 USB-CDC 固件：
//!   * `Towmcu`  — 双 CH32V305 / towmcu 板，APP=`ysc-towmcu-L|R v1.0`，IAP=`YSC-IAP`
//!   * `YscV3`   — YSC V3 双 CH32V317 板，APP=`YSC-V3-APP <date>`，IAP=`YSC-V3-IAP`
//!
//! 旧 XF Box 批次（`XFBOX-APP/XFBOX-IAP`）与 YSC V3 的升级流程完全相同，
//! 这里作为 `YscV3` 的兼容别名，保证已烧旧身份串的板子也能被识别。
//!
//! 版本串来自 cmd132 的 `data` 字段。V3 APP 的 data 是一段嵌套 JSON 字符串
//! （`{"v":"...","raw":"YSC-V3-APP ..."}`），所以这里用 `contains` 而不是
//! `starts_with`。

/// USB-CDC 固件族。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdcFamily {
    /// towmcu 双 MCU（V2）。
    Towmcu,
    /// YSC V3 / 旧 XF Box 双 CH32V317。
    YscV3,
}

impl CdcFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            CdcFamily::Towmcu => "v2",
            CdcFamily::YscV3 => "v3",
        }
    }
}

/// 从 cmd132 版本串判断固件族；无法识别返回 `None`（V1 单芯片等）。
pub fn family_of_version(version: &str) -> Option<CdcFamily> {
    if version.contains("YSC-V3-APP")
        || version.contains("YSC-V3-IAP")
        || version.contains("XFBOX-APP")
        || version.contains("XFBOX-IAP")
    {
        Some(CdcFamily::YscV3)
    } else if version.contains("ysc-towmcu-") || version.contains("YSC-IAP") {
        Some(CdcFamily::Towmcu)
    } else {
        None
    }
}

/// 版本串是否表示设备停在 IAP。
pub fn is_iap_version(version: &str) -> bool {
    version.contains("-IAP")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ysc_v3() {
        assert_eq!(
            family_of_version("YSC-V3-APP Oct 21 2026"),
            Some(CdcFamily::YscV3)
        );
        assert_eq!(
            family_of_version(r#"{"v":"1.1.0","raw":"YSC-V3-APP Oct 21 2026"}"#),
            Some(CdcFamily::YscV3)
        );
        assert_eq!(family_of_version("YSC-V3-IAP"), Some(CdcFamily::YscV3));
    }

    #[test]
    fn detects_towmcu_v2() {
        assert_eq!(
            family_of_version("ysc-towmcu-L v1.0"),
            Some(CdcFamily::Towmcu)
        );
        assert_eq!(
            family_of_version("ysc-towmcu-R v1.0"),
            Some(CdcFamily::Towmcu)
        );
        assert_eq!(family_of_version("YSC-IAP"), Some(CdcFamily::Towmcu));
    }

    #[test]
    fn detects_legacy_xfbox_as_v3() {
        assert_eq!(
            family_of_version("XFBOX-APP Oct 13 2026"),
            Some(CdcFamily::YscV3)
        );
        assert_eq!(family_of_version("XFBOX-IAP"), Some(CdcFamily::YscV3));
    }

    #[test]
    fn unknown_v1_is_none() {
        assert_eq!(family_of_version("Jul 23 2026"), None);
        assert_eq!(family_of_version(""), None);
    }

    #[test]
    fn iap_detection() {
        assert!(is_iap_version("YSC-V3-IAP"));
        assert!(is_iap_version("YSC-IAP"));
        assert!(is_iap_version("XFBOX-IAP"));
        assert!(!is_iap_version("YSC-V3-APP Oct 21 2026"));
        assert!(!is_iap_version("ysc-towmcu-L v1.0"));
    }

    #[test]
    fn family_names() {
        assert_eq!(CdcFamily::Towmcu.as_str(), "v2");
        assert_eq!(CdcFamily::YscV3.as_str(), "v3");
    }
}
