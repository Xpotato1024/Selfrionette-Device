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

## Architecture

| Topic | Canonical document |
|---|---|
| repository ownership / directory boundary | [repository-boundaries.md](architecture/repository-boundaries.md) |

## Contracts

| Topic | Canonical document |
|---|---|
| documentation lifecycle / SoT rules | [documentation-contract.md](contracts/documentation-contract.md) |
| device / host / Xpotato-Sim boundary | [device-host-boundary.md](contracts/device-host-boundary.md) |
| firmware protocol v2 | [firmware-protocol-v2.md](contracts/firmware-protocol-v2.md) |
| stable device identity / provisioning | [device-identity.md](contracts/device-identity.md) |
| firmware memory budget | [firmware-memory-budget.md](contracts/firmware-memory-budget.md) |
| host device core | [host-device-core.md](contracts/host-device-core.md) |
| serial transport | [serial-transport.md](contracts/serial-transport.md) |

## Operations

| Topic | Canonical document |
|---|---|
| Git / PR workflow | [git-pr-workflow.md](operations/git-pr-workflow.md) |
| validation categories | [validation.md](operations/validation.md) |
| hardware / serial / flashing safety | [hardware-safety.md](operations/hardware-safety.md) |

## Planned canonical topics

まだ仕様を固定していない項目は、実装前に必要な範囲だけ追加する。

- calibration / tare / persistent calibration
- firmware update
- Device Manager GUI
- dual-device role binding

未確定項目をこのindexだけで仕様化しない。
