---
status: canonical
owner: operations
last_verified: 2026-09-29
canonical_for:
  - serial access safety
  - firmware flashing safety
  - persistent calibration mutation safety
related:
  - AGENTS.md
  - docs/operations/validation.md
---

# Hardware Safety

## Permission boundary

明示的なhardware taskでない限り、次を実行しない。

- serial port open
- firmware upload / flashing
- bootloader操作
- EEPROM / persistent calibration mutation
- hardware resetを伴う自動操作
- connected deviceへのconfiguration write

code inspection、compile、fixture parser testはhardware accessではない。

## Before hardware access

最低限確認する。

- target physical device
- expected stable device identity（既知の場合）
- OS port
- board revision
- firmware target
- operation / command
- persistent stateを書き換えるか
- expected output
- stop procedure
- rollback / recovery procedure

port番号だけでdeviceを同定しない。

## Calibration safety

tare / calibrationは測定基準を変更する操作として扱う。

- operatorが無負荷 / reference load条件を確認する
- calibration revisionまたは実行時刻をhost evidenceへ残す
- previous persistent calibrationを上書きする場合は意図を明示する
- failed calibrationをvalidとして継続しない

## Flashing safety

- target board / environmentを確認する
- source revisionを記録する
- build artifactとtargetを対応づける
- upload後にfirmware / protocol versionを確認する
- upload failure時に別deviceへ無差別retryしない

## Dual-device safety

二台接続時はstable identityでtargetを確認する。

`COM5` / `COM6`等のport順だけでleft / right、flash target、calibration targetを判断しない。
