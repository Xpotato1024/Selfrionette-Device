//! Descriptor hints only. No port is opened, and no USB label proves device identity.
use selfrionette_serial::PortSummary;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortKey {
    pub name: String,
    pub kind: String,
    pub serial_number: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PortView {
    #[serde(flatten)]
    pub target: PortKey,
    pub category: &'static str,
    pub reason: &'static str,
}
impl From<PortSummary> for PortView {
    fn from(p: PortSummary) -> Self {
        let (category, reason) = classify(p.kind.as_str(), p.vid, p.pid);
        Self {
            target: PortKey {
                name: p.name,
                kind: p.kind.as_str().into(),
                serial_number: p.serial_number,
                product: p.product,
                manufacturer: p.manufacturer,
                vid: p.vid,
                pid: p.pid,
            },
            category,
            reason,
        }
    }
}
pub fn validate_candidate_topology(ports: &[PortView], target: &PortKey) -> Result<(), String> {
    if ports.len() > 128
        || ports
            .iter()
            .filter(|p| p.target.name == target.name)
            .count()
            != 1
    {
        return Err("接続先の列挙結果が曖昧です。".into());
    }
    let matches = if let Some(serial) = &target.serial_number {
        ports
            .iter()
            .filter(|p| p.target.serial_number.as_ref() == Some(serial))
            .count()
    } else {
        ports
            .iter()
            .filter(|p| {
                let t = &p.target;
                t.kind == target.kind
                    && t.serial_number.is_none()
                    && t.product == target.product
                    && t.manufacturer == target.manufacturer
                    && t.vid == target.vid
                    && t.pid == target.pid
            })
            .count()
    };
    if matches != 1 {
        return Err(
            "USB情報が同じ接続先が複数あります。対象を一台にして再確認してください。".into(),
        );
    }
    Ok(())
}
// Manufacturer board definitions, not an application authentication allowlist.
// SparkFun: Arduino_Boards/main/sparkfun/avr/boards.txt (Pro Micro 8/16MHz)
// Arduino: ArduinoCore-avr/master/boards.txt (Leonardo / Micro)
pub fn classify(kind: &str, vid: Option<u16>, pid: Option<u16>) -> (&'static str, &'static str) {
    if kind != "usb" {
        return if kind == "unknown" {
            ("unknown", "USB情報を取得できません")
        } else {
            ("other", "USB接続の対応基板ではありません")
        };
    }
    match (vid, pid) {
        (Some(0x1b4f), Some(0x9204 | 0x9206)) => {
            ("candidate", "Pro Microの通常動作VID/PID・個体未確認")
        }
        (Some(0x2341 | 0x2a03), Some(0x8036 | 0x8037)) => {
            ("candidate", "ATmega32U4基板のVID/PID・用途未確認")
        }
        (Some(0x2341), Some(0x8237)) => ("candidate", "Arduino MicroのVID/PID・用途未確認"),
        (Some(0x1b4f), Some(0x9203 | 0x9205))
        | (Some(0x2341 | 0x2a03), Some(0x0036 | 0x0037))
        | (Some(0x2341), Some(0x0237)) => {
            ("bootloader", "書込み待機用ポート・入力監視には使用不可")
        }
        (None, _) | (_, None) => ("unknown", "VID/PIDを取得できません"),
        _ => ("other", "対応候補に登録されていないUSB機器"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classification_is_not_a_com_number_or_product_name_guess() {
        assert_eq!(classify("usb", Some(0x1b4f), Some(0x9206)).0, "candidate");
        assert_eq!(classify("usb", Some(0x2341), Some(0x8036)).0, "candidate");
        assert_eq!(classify("pci", None, None).0, "other");
        assert_eq!(classify("unknown", None, None).0, "unknown");
        assert_eq!(classify("usb", Some(0x1a86), Some(0x7523)).0, "other");
        assert_eq!(classify("usb", Some(0x1b4f), Some(0x9205)).0, "bootloader");
        assert_eq!(classify("usb", Some(0x2341), Some(0x0036)).0, "bootloader");
    }
    #[test]
    fn usb_serial_and_kind_survive_enumeration() {
        let view = PortView::from(PortSummary {
            name: "COM8".into(),
            kind: selfrionette_serial::PortKind::Usb,
            serial_number: Some("A".into()),
            product: None,
            manufacturer: None,
            vid: Some(0x1b4f),
            pid: Some(0x9206),
        });
        assert_eq!(view.target.serial_number.as_deref(), Some("A"));
        assert_eq!(view.target.kind, "usb");
        let mut other = view.target.clone();
        other.serial_number = Some("B".into());
        assert_ne!(other, view.target);
    }
    #[test]
    fn identical_descriptor_candidates_without_serial_are_ambiguous() {
        let a = PortView::from(PortSummary {
            name: "COM8".into(),
            kind: selfrionette_serial::PortKind::Usb,
            serial_number: None,
            product: Some("same".into()),
            manufacturer: None,
            vid: Some(0x1b4f),
            pid: Some(0x9206),
        });
        let mut b = a.clone();
        b.target.name = "COM9".into();
        assert!(validate_candidate_topology(&[a.clone(), b], &a.target).is_err());
        assert!(validate_candidate_topology(std::slice::from_ref(&a), &a.target).is_ok());
    }
}
