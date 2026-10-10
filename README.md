# Selfrionette-Device

Selfrionette実デバイス側のfirmware、device protocol、identity、physical calibration、diagnostics、host-side device managementを管理するrepository。

Xpotato-Simから見たSelfrionetteはGamepad / Keyboardと同格のInput Sourceであり、robot control mappingやMuJoCo simulationはこのrepositoryの責務ではない。

## Initial direction

- Hardware: existing Pro Micro / ATmega32U4
- Firmware: C++ / PlatformIOを初期方針とする
- Host tooling: Rust workspace（`selfrionette-core` / `selfrionette-serial` / `selfrionettectl`）としてfirmwareと独立に設計する
- Device management: CLI-first、GUIは同じcore logicを再利用する
- Identity: stable device identityとleft/right等のroleを分離する
- Protocol: versioned producer contractをこのrepositoryで管理する
- Resource policy: ATmega32U4のFlash / SRAM / EEPROM制約をbuild evidenceで管理する

設計判断は[docs/README.md](docs/README.md)から辿る。

## Repository layout

- `firmware/`: current Protocol v2 firmwareと旧Sim互換reference
- `crates/`: host-side Rust libraries / CLI
- `apps/`: human-facing applications such as Device Manager
- `hardware/`: board / pinout / hardware reference
- `docs/`: architecture、contracts、ADR、operations
- `tests/`: repository-level / cross-component tests
- `tools/`: repository support tools

旧Simの2ターゲット、software buildと採用順は[firmware移行・build案内](docs/operations/legacy-sim-firmware.md)を参照する。

実装前に[AGENTS.md](AGENTS.md)を確認する。

旧Simのmonitor/measureはDeviceのRust CLIが所有する。[build・help・採用順](docs/operations/legacy-sim-host-tools.md)を参照する。
