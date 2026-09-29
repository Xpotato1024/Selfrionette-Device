---
status: accepted
owner: architecture
date: 2026-09-29
canonical_for:
  - repository responsibility boundary
  - Xpotato-Sim integration boundary
  - device management layer split
related:
  - docs/README.md
  - docs/architecture/repository-boundaries.md
  - docs/decisions/0001-retain-pro-micro-cpp-firmware.md
---

# ADR-0002: Device管理をXpotato-Simから分離する

## Context

SelfrionetteはXpotato-Simから見るとGamepadやKeyboardと同じInput Sourceである。一方で実デバイスには、sensor acquisition以外にdevice identity、persistent calibration、diagnostics、firmware update、設定、二台接続時の個体管理が必要になる。

これらをXpotato-SimのInput Source Pluginへ集約すると、simulation / experiment側の責務とdevice lifecycle管理が結合する。

## Decision

Selfrionette Device固有の実装と管理機能を`Selfrionette-Device` repositoryへ分離する。

### Selfrionette-Deviceが所有するもの

- Pro Micro firmware
- hardware / pinout / board revision情報
- device identity
- firmware version / protocol version
- sensor acquisition
- physical calibration / tare
- calibration persistence
- sensor health / diagnostics
- firmware build / upload手順
- host-side device discovery / configuration / diagnostics
- Device Manager GUIとCLIのdevice-management logic
- producer側protocol contract

### Xpotato-Simが所有するもの

- `selfrionette/v1` Input Source Plugin
- Gamepad / Keyboard等と同格のruntime selection
- device sampleをrobot control intentへ変換するMapping
- world / tool / task frame等のcontrol semantics
- robot / task / experiment roleへのbinding
- simulation / MuJoCo / evaluation / experiment logging
- consumer側protocol compatibility

### Plugin boundary

Xpotato-SimのSelfrionette pluginはDevice Managerそのものにならない。

pluginが扱うのは、runtimeに必要な次の情報までとする。

- input sample
- sample timestamp / sequence
- stable device identity
- source health
- protocol / firmware compatibilityに必要なmetadata

firmware upload、calibration wizard、persistent settings editor、device provisioning GUIをpluginへ入れない。

## Host-side structure

host toolingはfirmwareから独立させる。

- reusable device core / protocol / discoveryはRust libraryへ置ける
- CLIはcore libraryを利用する
- GUIはCLI subprocessを主APIにせず、同じcore libraryを利用する
- GUI実装はTauriを第一候補とするが、UI framework固定は別ADRで確定する
- background daemon / serviceは初期要件にしない。複数processによるserial ownership競合が実際の要件になった時点で別ADRを作る

## CLI-first policy

device management機能はGUI専用にしない。

初期management capabilityは、将来次のような非対話または明示操作可能なinterfaceから呼べる構造を前提にする。

- list / discover
- info
- stream / monitor
- tare
- calibrate
- validate
- diagnose
- assign friendly name
- provision identity
- firmware information
- firmware update orchestration

具体的なCLI syntaxはimplementation時に確定する。

## Identity and role

device identityとexperiment roleを分離する。

firmware / device persistent stateにはstable device identityを持たせるが、`left` / `right`を恒久identityとして焼き込まない。

`left_hand`、`right_hand`等のroleはhostまたはexperiment configurationでstable device identityへbindする。

これによりUSB port番号や接続順が変わってもroleを維持できる。

## Protocol ownership

Selfrionette-Deviceはproducer側のversioned protocol contractを所有する。

Xpotato-Simはそのprotocolをconsumerとして実装し、互換versionを明示する。両repositoryへ同じ仕様本文を独立複製して第二SoTを作らない。

recorded serial fixtureやsimulation regression fixtureはXpotato-Sim側に保持してよい。

## Consequences

### Positive

- Xpotato-SimのInput Source PluginをGamepad等と同じ抽象度に保てる。
- device lifecycleとrobot mappingを分離できる。
- CLI / GUI / automated experimentが同じdevice coreを再利用できる。
- 二台以上のSelfrionetteをstable identityで扱える。
- firmwareとsimulationのrelease cycleを分離できる。

### Negative

- cross-repository compatibility管理が必要になる。
- firmware protocol変更時にproducer / consumer両側の検証が必要になる。
- host toolingとfirmwareで別のbuild toolchainを持つ。

## Non-goals

- daemon / background serviceの即時実装
- GUI frameworkの最終決定
- binary protocolへの即時移行
- Xpotato-Sim側のrobot mapping仕様の再設計
- firmware implementationそのもの

## Review triggers

- serial port ownershipを複数processで共有する必要が生じる
- Device Manager常駐が実験運用上必要になる
- Xpotato-Sim以外のconsumerが増える
- transportをUSB serial以外へ変更する
- device protocolを破壊的に更新する
