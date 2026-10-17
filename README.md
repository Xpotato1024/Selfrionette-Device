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

Windows 11 x64向けのインストーラー、portable ZIP、2系統のFirmware packageを配布対象とします。このsourceは **0.2.0-preview.2** の未公開候補です。公開済みの **0.2.0-preview.1**（source `9dbc372b`）は[GitHub Releases](https://github.com/Xpotato1024/Selfrionette-Device/releases/tag/device-manager-v0.2.0-preview.1)から取得できます。公開済みasset/tagは変更しません。

## Device Managerを使う人（ビルド不要）

ビルド済みのWindows用setup.exeまたはZIPを使います。[取得・起動・保存先](docs/operations/device-manager-distribution.md)を参照してください。
配布物の利用にRust、Node.js、Visual Studio、PlatformIOやcloneは不要です。品質付きFirmwareの装置設定・tare・個人校正・profile保存/再利用とFirmware更新を扱います。既知荷重によるN係数の校正、実boardでの更新・EEPROM保持・操作精度の受入は未実施です。

## Device Managerを開発・ローカル検証する人

Tauri GUIと、Nodeだけで表示を確認できる手順は[app README](apps/device-manager/README.md)を参照してください。
Windows native Debug build・起動・Rust IPC・合成記録の保存/再読込を確認しました。実機受信モニターの実装はありますが、対象実機での受入と物理校正は未実施です。

    # 自分のclone・作業用ローカルブランチのrepository rootから
    cd apps/device-manager
    npm ci
    .\run-native.cmd build
    .\src-tauri\target\debug\selfrionette-device-manager.exe

終了はnative windowを閉じます。Nodeだけの表示確認はapp READMEの npm run preview を参照してください。

## 入力適合のオフライン比較

7軸の記録から単純補正と干渉補償を比較するCLIを用意しています。実機への書込み・校正済み認定は行いません。
[合成例・記録JSON・実行手順](docs/operations/input-calibration-replay.md)を参照してください。

## 30〜60分の実機観測

[試験計画・記録CLI](docs/operations/field-test-30-60min.md)。実機書込みを行わない受信観測で、最終校正とは別段階です。
