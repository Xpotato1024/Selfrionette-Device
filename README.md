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


## Windows用Device Managerをダウンロード

**[Selfrionette Device Manager — GitHub Releases](https://github.com/Xpotato1024/Selfrionette-Device/releases)**

Windows 11 x64向けのインストーラー版（推奨）と、インストール不要のZIP版を公開します。現在の配布版は[0.1.0-preview.5](https://github.com/Xpotato1024/Selfrionette-Device/releases/tag/device-manager-v0.1.0-preview.5)です。プレビュー版のため実機校正・Firmware書込みは未実装です。

## Device Managerを使う人（ビルド不要）

ビルド済みのWindows用setup.exeまたはZIPを使います。[取得・起動・保存先](docs/operations/device-manager-distribution.md)を参照してください。
Rust、Node.js、Visual Studio、Gitの導入やcloneは不要です。現在は実機受信モニターと合成調整のプレビュー版です。実機校正・Firmware更新は未対応です。

## Device Managerを開発・ローカル検証する人

Tauri GUIと、Nodeだけで表示を確認できる手順は[app README](apps/device-manager/README.md)を参照してください。
Windows native Debug build・起動・Rust IPC・合成記録の保存/再読込を確認しました。実機受信モニターの実装はありますが、対象実機での受入と物理校正は未実施です。

    # 自分のclone・作業用ローカルブランチのrepository rootから
    cd apps/device-manager
    npm ci
    .\run-native.cmd build
    .\src-tauri\target\debug\selfrionette-device-manager.exe

終了はnative windowを閉じます。Nodeだけの表示確認はapp READMEの npm run preview を参照してください。
