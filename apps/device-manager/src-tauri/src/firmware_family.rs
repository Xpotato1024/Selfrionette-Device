use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    #[default]
    Modern,
    #[serde(rename = "legacy_7ch", alias = "legacy7ch")]
    Legacy7ch,
    #[serde(rename = "compatible_7ch")]
    Compatible7ch,
}
pub const LEGACY_MANIFEST_PIN: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_LEGACY_MANIFEST_SHA256");
pub const LEGACY_IMAGE_PIN: Option<&str> = option_env!("SELFRIONETTE_EXPECTED_LEGACY_IMAGE_SHA256");

pub const COMPATIBLE_MANIFEST_PIN: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_COMPATIBLE_MANIFEST_SHA256");
pub const COMPATIBLE_IMAGE_PIN: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_COMPATIBLE_IMAGE_SHA256");

impl Family {
    pub fn is_archive(self) -> bool {
        self != Self::Modern
    }
    pub fn directory(self) -> &'static str {
        match self {
            Self::Modern => "",
            Self::Legacy7ch => "legacy-7ch",
            Self::Compatible7ch => "compatible-7ch",
        }
    }
    pub fn source_path(self) -> &'static str {
        match self {
            Self::Modern => "firmware/loadcell_7ch",
            Self::Legacy7ch => {
                "firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy"
            }
            Self::Compatible7ch => {
                "firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_pro_micro"
            }
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Edition {
    pub family: Family,
    pub name: String,
    pub distribution_version: String,
    pub firmware_version: Option<String>,
    pub protocols: Vec<u8>,
    pub description: String,
    pub source_path: String,
}
/// 配布版は初期ソースに後付けした識別子。本体が返す版を捏造しない。
pub fn editions() -> Result<Vec<Edition>, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Catalog {
        schema: u8,
        editions: Vec<Edition>,
    }
    let catalog: Catalog = serde_json::from_str(include_str!("../../../../firmware/editions.json"))
        .map_err(|e| e.to_string())?;
    if catalog.schema != 1 || catalog.editions.len() != 3 {
        return Err("Firmware一覧の形式が不正です。".into());
    }
    for (edition, family) in
        catalog
            .editions
            .iter()
            .zip([Family::Modern, Family::Legacy7ch, Family::Compatible7ch])
    {
        if edition.family != family
            || edition.source_path != family.source_path()
            || edition.name.is_empty()
            || edition.distribution_version.is_empty()
            || (family.is_archive()
                && (edition.firmware_version.is_some() || !edition.protocols.is_empty()))
            || (!family.is_archive()
                && (edition.firmware_version.as_deref().unwrap_or("").is_empty()
                    || edition.protocols != [2, 3]))
        {
            return Err("Firmware系統の定義が不正です。".into());
        }
    }
    Ok(catalog.editions)
}

/// A dense application image makes AVRDUDE compare every application flash byte,
/// including gaps. The fixed 0x7000 boundary excludes the bootloader and EEPROM.
pub fn full_application_hex(bytes: &std::collections::BTreeMap<u32, u8>) -> String {
    let mut result = String::with_capacity(79000);
    for address in (0u16..0x7000).step_by(16) {
        let mut record = vec![16, (address >> 8) as u8, address as u8, 0];
        for offset in 0..16 {
            record.push(*bytes.get(&(u32::from(address) + offset)).unwrap_or(&0xff));
        }
        let checksum = 0u8.wrapping_sub(record.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
        record.push(checksum);
        result.push(':');
        for b in record {
            result.push_str(&format!("{b:02X}"));
        }
        result.push('\n');
    }
    result.push_str(":00000001FF\n");
    result
}

#[cfg(test)]
mod edition_tests {
    use super::*;
    #[test]
    fn published_family_ids_and_old_journal_spelling_are_explicit() {
        for name in ["legacy_7ch", "legacy7ch"] {
            assert_eq!(
                serde_json::from_value::<Family>(serde_json::json!(name)).unwrap(),
                Family::Legacy7ch
            );
        }
        assert_eq!(
            serde_json::to_value(Family::Legacy7ch).unwrap(),
            "legacy_7ch"
        );
        assert_eq!(
            serde_json::to_value(Family::Compatible7ch).unwrap(),
            "compatible_7ch"
        );
        assert_eq!(editions().unwrap().len(), 3);
    }
}
