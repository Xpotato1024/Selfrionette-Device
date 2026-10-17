//! Local candidate only. Reading preferences never opens or certifies a device.
use crate::discovery::PortKey;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    schema: u8,
    candidate: PortKey,
}
pub fn load(root: &Path) -> Result<Option<PortKey>, String> {
    let path = root.join("connection-preferences.json");
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 4096 {
        return Err("接続先の保存情報が大きすぎます。".into());
    }
    let record: Preferences = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if record.schema != 1 {
        return Err("未対応の接続先保存形式です。".into());
    }
    validate(&record.candidate)?;
    Ok(Some(record.candidate))
}
pub fn save(root: &Path, target: PortKey) -> Result<(), String> {
    validate(&target)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    fs::write(
        root.join("connection-preferences.json"),
        serde_json::to_vec(&Preferences {
            schema: 1,
            candidate: target,
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn validate(target: &PortKey) -> Result<(), String> {
    if target.name.is_empty() || serde_json::to_vec(target).map_err(|e| e.to_string())?.len() > 3072
    {
        return Err("接続先の保存情報が不正です。".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_loads_only_candidate_descriptors_and_corruption_fails_closed() {
        let root =
            std::env::temp_dir().join(format!("connection-preferences-{}", std::process::id()));
        assert!(!root.exists());
        let target = PortKey {
            name: "COM8".into(),
            kind: "usb".into(),
            serial_number: Some("candidate".into()),
            product: None,
            manufacturer: None,
            vid: Some(0x1b4f),
            pid: Some(0x9206),
        };
        assert!(load(&root).unwrap().is_none());
        save(&root, target.clone()).unwrap();
        assert_eq!(load(&root).unwrap(), Some(target));
        fs::write(root.join("connection-preferences.json"), b"{}").unwrap();
        assert!(load(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
