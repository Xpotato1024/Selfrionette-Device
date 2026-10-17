use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    #[default]
    Modern,
    Legacy7ch,
}
pub const LEGACY_MANIFEST_PIN: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_LEGACY_MANIFEST_SHA256");
pub const LEGACY_IMAGE_PIN: Option<&str> = option_env!("SELFRIONETTE_EXPECTED_LEGACY_IMAGE_SHA256");

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
