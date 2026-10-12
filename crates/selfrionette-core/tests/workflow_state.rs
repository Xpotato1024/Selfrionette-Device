use selfrionette_core::VectorFrame;
use selfrionette_core::calibration::*;

fn session() -> (CalibrationSession, Context) {
    let context = Context {
        source: Source {
            id: "srn-11111111111111111111111111111111".parse().unwrap(),
            firmware: "synthetic-v2".into(),
            assembly: "fixture-assembly-1".into(),
            epoch: 1,
        },
        user: "fixture-user".into(),
        posture: "fixture-neutral".into(),
    };
    (CalibrationSession::new(context.clone()).unwrap(), context)
}
fn candidates(s: &mut CalibrationSession) {
    s.set_candidates(
        std::array::from_fn(|_| Assessment::Candidate {
            sign: 1,
            positive_span: 20.0,
        }),
        std::array::from_fn(|_| "synthetic known direction".into()),
    )
    .unwrap();
}
#[test]
fn progress_is_derived_and_revocation_clears_the_preview() {
    let (mut s, context) = session();
    assert_eq!(s.phase(), CalibrationPhase::NeedsBaseline);
    s.set_baseline([10.0; N]).unwrap();
    assert_eq!(s.phase(), CalibrationPhase::NeedsAssessment);
    candidates(&mut s);
    assert_eq!(s.phase(), CalibrationPhase::NeedsPreview);
    let sample = FirmwareSample::from_vector(
        context,
        &VectorFrame {
            timestamp_ms: 0,
            channels: [20.0; N],
        },
        Quality::KnownGoodSynthetic,
    )
    .unwrap();
    let p = s.preview(&sample).unwrap();
    assert_eq!(s.phase(), CalibrationPhase::NeedsConfirmation);
    assert_eq!(s.current_preview(), Some(&p));
    s.approve(&p).unwrap();
    assert_eq!(s.phase(), CalibrationPhase::Confirmed);
    let revision = s.revision();
    s.revoke_preview();
    assert_eq!(s.phase(), CalibrationPhase::NeedsPreview);
    assert_eq!(s.revision(), revision);
    assert!(s.current_preview().is_none());
    assert!(s.draft().is_err());
    s.revoke_candidates();
    assert_eq!(s.phase(), CalibrationPhase::NeedsAssessment);
    assert!(
        s.manual_flip(0, "cannot recover revoked candidates")
            .is_err()
    );
    assert!(s.revision() > revision);
    s.invalidate();
    assert_eq!(s.phase(), CalibrationPhase::NeedsBaseline);
}
#[test]
fn failed_assessment_cannot_become_confirmed_through_the_view() {
    let (mut s, _) = session();
    s.set_baseline([10.0; N]).unwrap();
    candidates(&mut s);
    let result = s.set_candidates(
        std::array::from_fn(|_| Assessment::Inconclusive("unknown quality")),
        std::array::from_fn(|_| "fixture".into()),
    );
    assert!(result.is_err());
    assert_eq!(s.phase(), CalibrationPhase::NeedsAssessment);
    assert!(s.current_preview().is_none());
}
