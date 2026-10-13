---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - documentation source-of-truth map
related:
  - docs/contracts/documentation-contract.md
---

# Documentation Source of Truth Map

`Selfrionette-Device`のcurrent specificationは、このindexから辿れるcanonical documentを正とする。

## Decisions

| Topic | Canonical document | Meaning |
|---|---|---|
| MCU / firmware language | [ADR-0001](decisions/0001-retain-pro-micro-cpp-firmware.md) | Pro Micro継続、C++ firmware、AVR resource制約 |
| repository / integration boundary | [ADR-0002](decisions/0002-separate-device-management-from-simulation-plugin.md) | Device管理とXpotato-Sim Input Source Pluginの分離 |
| protocol v2 architecture | [ADR-0003](decisions/0003-versioned-ascii-protocol-v2.md) | 既存vector互換を保つversioned management protocol |
| stable device identity | [ADR-0004](decisions/0004-eeprom-provisioned-device-identity.md) | 128-bit IDのEEPROM provisioningとrole分離 |
| host core / CLI | [ADR-0005](decisions/0005-rust-host-core-cli.md) | Rust workspaceでpure coreとCLIを分離 |
| serial transport | [ADR-0006](decisions/0006-explicit-serial-transport.md) | explicit portだけをopenする独立transport layer |
| bounded sample monitor | [ADR-0007](decisions/0007-bounded-identity-checked-monitor.md) | 期待identity確認後の有限diagnostic capture |

## Architecture

| Topic | Canonical document |
|---|---|
| repository ownership / directory boundary | [repository-boundaries.md](architecture/repository-boundaries.md) |
| Device Manager製品・操作体系（合成実装） | [device-manager-workflow.md](architecture/device-manager-workflow.md) |

## Contracts

| Topic | Canonical document |
|---|---|
| documentation lifecycle / SoT rules | [documentation-contract.md](contracts/documentation-contract.md) |
| device / host / Xpotato-Sim boundary | [device-host-boundary.md](contracts/device-host-boundary.md) |
| 旧Sim 7ch producer（互換用） | [legacy-sim-serial-producer.md](contracts/legacy-sim-serial-producer.md) |
| firmware protocol v2 | [firmware-protocol-v2.md](contracts/firmware-protocol-v2.md) |
| stable device identity / provisioning | [device-identity.md](contracts/device-identity.md) |
| firmware memory budget | [firmware-memory-budget.md](contracts/firmware-memory-budget.md) |
| 旧Sim monitor / measure host処理 | [legacy-sim-host-tools.md](contracts/legacy-sim-host-tools.md) |
| GUIの実機列挙・受信確認 | [gui-live-monitor.md](contracts/gui-live-monitor.md) |
| host device core | [host-device-core.md](contracts/host-device-core.md) |
| serial transport | [serial-transport.md](contracts/serial-transport.md) |
| identity-checked sample monitor | [sample-monitor.md](contracts/sample-monitor.md) |

## Operations

| Topic | Canonical document |
|---|---|
| Device Managerの検証層・再利用 | [device-manager-testing.md](operations/device-manager-testing.md) |
| Windowsバイナリの利用・生成 | [device-manager-distribution.md](operations/device-manager-distribution.md) |
| Git / PR workflow | [git-pr-workflow.md](operations/git-pr-workflow.md) |
| 旧Sim firmwareの管理・build・採用順 | [legacy-sim-firmware.md](operations/legacy-sim-firmware.md) |
| 旧Sim monitor / measure操作 | [legacy-sim-host-tools.md](operations/legacy-sim-host-tools.md) |
| validation categories | [validation.md](operations/validation.md) |
| hardware / serial / flashing safety | [hardware-safety.md](operations/hardware-safety.md) |

## Planned canonical topics

まだ仕様を固定していない項目は、実装前に必要な範囲だけ追加する。

- calibration / tare / persistent calibration
- firmware update
- Device Manager GUIの実機接続・校正・保守
- dual-device role binding

未確定項目をこのindexだけで仕様化しない。


## ローカルレビュー中の校正GUI

- [calibration preview契約案](contracts/calibration-preview.md)
- [後続観測手順案](operations/calibration-observation.md)
- [ADR-0008案](decisions/0008-offline-device-manager-calibration.md)
- [GUI setup/start/stopと検証限界](../apps/device-manager/README.md)

実機のverified条件は未確定で、これらのdraftをphysical calibration成功契約として扱わない。

## 装置単独利用と保守（設計・実装中）

- [ADR-0009案: GUI常駐不要の装置設定](decisions/0009-standalone-device-configuration.md)
- [装置設定recordとjournal](contracts/device-persistent-settings.md)
- [Firmware更新と設定保護](contracts/firmware-update.md)

contract案とsoftware-only試験は実機受入を意味しない。現行v2の値の意味・通信は維持する。
