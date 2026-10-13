use selfrionette_core::calibration::*;
use selfrionette_core::persistence::*;
use selfrionette_core::{DeviceId, VectorFrame};
use std::fs;
fn context() -> Context {
    Context {
        source: Source {
            id: "srn-11111111111111111111111111111111"
                .parse::<DeviceId>()
                .unwrap(),
            firmware: "synthetic-v2".into(),
            assembly: "fixture-assembly-1".into(),
            epoch: 1,
        },
        user: "fixture-user".into(),
        posture: "fixture-neutral".into(),
    }
}
fn draft() -> Draft {
    let mut s = CalibrationSession::new(context()).unwrap();
    s.set_baseline([10.0; N]).unwrap();
    s.set_candidates(
        std::array::from_fn(|_| Assessment::Candidate {
            sign: 1,
            positive_span: 20.0,
        }),
        std::array::from_fn(|_| "合成fixture evidence".into()),
    )
    .unwrap();
    let sample = FirmwareSample::from_vector(
        context(),
        &VectorFrame {
            timestamp_ms: 0,
            channels: [20.0; N],
        },
        Quality::KnownGoodSynthetic,
    )
    .unwrap();
    let p = s.preview(&sample).unwrap();
    s.approve(&p).unwrap();
    s.draft().unwrap()
}
#[test]
fn roundtrip_and_reload_cannot_restore_readiness() {
    let d = draft();
    let encoded = encode(&d).unwrap();
    let loaded = decode(&encoded).unwrap();
    assert_eq!(loaded.evidence, d.evidence);
    assert_eq!(loaded.signs, d.signs);
    assert_eq!(loaded.observation, d.observation);
    assert!(!loaded.hardware_verified());
    assert!(!loaded.control_ready());
    let mut fresh = context();
    fresh.source.epoch += 1;
    let mut s = resume(&loaded, fresh.clone()).unwrap();
    let sample = FirmwareSample::from_vector(
        fresh,
        &VectorFrame {
            timestamp_ms: 0,
            channels: [20.0; N],
        },
        Quality::KnownGoodSynthetic,
    )
    .unwrap();
    assert!(s.preview(&sample).is_err());
    assert!(s.draft().is_err());
}
#[test]
fn bad_schema_values_identity_and_context_rejected() {
    let text = encode(&draft()).unwrap();
    for invalid in [
        text.replace("srn-preview,1", "srn-preview,2"),
        text.replace("hardware_verified,0", "hardware_verified,1"),
        text.replace("channel,0,1,20", "channel,0,0,20"),
        text.replace("channel,0,1,20", "channel,0,1,NaN"),
        text.replace("channel,0,1,20", "channel,0,1,inf"),
        text.replace("channel,0,1,20", "channel,0,1,0"),
        text.replace("channel,0", "channel,1"),
        text.replace("firmware_sample,20", "firmware_sample,NaN"),
        text.replace("normalized,0.5", "normalized,inf"),
        text.replace("normalized,0.5", "normalized,0.6"),
        text.replace("normalized,0.5", "normalized,?"),
        text.replace("oriented,10", "oriented,11"),
        text.replace("firmware_sample,20", "firmware_sample,21"),
        text.replace("historical_baseline,10", "historical_baseline,11"),
        text.replace("timestamp_ms,0", "timestamp_ms,4294967296"),
        text.lines().take(11).collect::<Vec<_>>().join("\n"),
        "X".repeat(MAX_RECORD_BYTES + 1),
    ] {
        assert!(decode(&invalid).is_err());
    }
    let d = decode(&text).unwrap();
    for kind in 0..4 {
        let mut c = context();
        match kind {
            0 => c.source.id = "srn-22222222222222222222222222222222".parse().unwrap(),
            1 => c.source.assembly = "other".into(),
            2 => c.user = "other".into(),
            _ => c.posture = "other".into(),
        };
        assert!(resume(&d, c).is_err());
    }
}
#[test]
fn per_id_store_save_reload_and_corruption_fail_closed() {
    let root = std::env::temp_dir().join(format!("srn-synthetic-store-{}", std::process::id()));
    assert!(!root.exists());
    let store = ProfileStore::new(root.clone()).unwrap();
    let d = draft();
    let path = store.save(&d).unwrap();
    assert_eq!(store.load(&context()).unwrap().evidence, d.evidence);
    let mut wrong = context();
    wrong.source.id = "srn-22222222222222222222222222222222".parse().unwrap();
    assert!(store.load(&wrong).is_err());
    fs::write(&path, "truncated").unwrap();
    assert!(store.load(&context()).is_err());
    fs::remove_file(path).unwrap();
    for filename in [
        "profile-9.srnp",
        "profile-10.srnp",
        "profile-00000000000000000000.srnp",
    ] {
        let bad = root.join(context().source.id.to_string()).join(filename);
        fs::write(&bad, encode(&d).unwrap()).unwrap();
        assert!(store.load(&context()).is_err());
        fs::remove_file(bad).unwrap();
    }
    let first = store.save(&d).unwrap();
    let mut newer = d.clone();
    newer.revision += 1;
    let second = store.save(&newer).unwrap();
    assert_eq!(store.load(&context()).unwrap().revision, newer.revision);
    fs::remove_file(first).unwrap();
    fs::remove_file(second).unwrap();
    fs::remove_dir(root.join(context().source.id.to_string())).unwrap();
    fs::remove_dir(root).unwrap();
}
