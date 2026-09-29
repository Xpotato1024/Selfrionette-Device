---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - firmware / host ownership boundary
  - physical calibration / robot mapping separation
  - Xpotato-Sim consumer boundary
related:
  - docs/architecture/repository-boundaries.md
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
---

# Device / Host Boundary Contract

## Core rule

physical device truth、device management、robot-control semanticsを分離する。

## Firmware -> Host

firmwareがhostへ提供する情報は、device / sensorに属する事実に限定する。

最低限のversioned contract候補:

- protocol version
- firmware version
- stable device identity
- device timestamp / sequence
- channel count
- sensor sample
- calibration state
- health / diagnostic event

具体的なwire grammarはprotocol ADR / contractで別途固定する。

## Physical calibration

physical calibrationはDevice側の責務である。

例:

- tare / zero offset
- sensor gain
- calibration revision
- calibration validity
- sensor fault / timeout

calibrated physical quantityを導入する場合、unit、scale、persistence、versionをprotocol contractで明示する。

## Robot mapping

robot control mappingはDevice側の責務ではない。

例:

- channel -> XYZ
- channel -> joint
- sign / weight for robot motion
- endpoint velocity gain
- contact task gain
- world / tool / task frame

これらはXpotato-SimのMapping / experiment側で管理する。

## Identity

stable device identityをOS port名から導出しない。

同型deviceが二台接続されても、identityにより区別できるprotocolを設計する。

`left` / `right`はpersistent hardware identityにしない。

## Fail-closed

次は自動推測で継続しない。

- unsupported protocol version
- duplicate stable device identity
- required device identity missing
- invalid calibration state when calibrated operation is required
- channel count incompatibility
- malformed sample

operatorが明示的に確認できるdiagnosticを返す。

## Resource boundary

firmwareのmemory節約を理由に、必要なidentity / protocol version / failure状態を削除しない。

一方、長いhuman-readable messageや履歴はhost側へ寄せ、firmwareはbounded numeric / enum diagnosticを優先する。
