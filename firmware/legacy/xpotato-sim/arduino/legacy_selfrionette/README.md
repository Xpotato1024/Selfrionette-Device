# Legacy Selfrionette Firmware Reference

> このdirectoryはhistorical referenceであり、current operationのsource of truthではない。
> current protocolとoperator gateは
> [旧serial producer仕様](../../../../../docs/contracts/legacy-sim-serial-producer.md)と
> [hardware safety](../../../../../docs/operations/hardware-safety.md)を参照する。

This directory is copied from old Selfrionette `/firmware/arduino/`.

- Reference-only import.
- No firmware behavior was intentionally changed.
- No build / upload validation was performed in this PR.
- Source path: `Xpotato-apps/Selfrionette/firmware/arduino/`
- Imported path: `firmware/arduino/legacy_selfrionette/`

Known hardware:

- Pro Micro
- HX717
- 20 kg load cell
- USB-C communication / power

HX717 supports 10 Hz / 80 Hz rate setting.
Current circuit is expected to use 80 Hz, but code / wiring confirmation is still required.

既存subdirectoryのREADMEは当時のboard、transport、取り込み経緯を保持するため、
current仕様へ合わせて改稿しない。
