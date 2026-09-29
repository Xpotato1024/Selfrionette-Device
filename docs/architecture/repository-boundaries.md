---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - repository layout
  - component ownership
  - cross-repository responsibility
related:
  - docs/decisions/0001-retain-pro-micro-cpp-firmware.md
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
  - docs/contracts/device-host-boundary.md
---

# Repository Boundaries

## Purpose

Selfrionette-Device内のcomponent ownershipと、Xpotato-Simとの境界を固定する。

## Top-level layout

| Path | Responsibility |
|---|---|
| `firmware/` | Pro Micro / ATmega32U4 firmware |
| `crates/` | host-side Rust core、protocol、CLI等 |
| `apps/` | Device Manager等のhuman-facing application |
| `hardware/` | board revision、pinout、wiring、physical reference |
| `docs/` | current architecture、contracts、ADR、operations、evidence |
| `tests/` | cross-component / repository-level fixture and contract tests |
| `tools/` | build、inspection、migration、support tools |

初期段階では空directoryを`.gitkeep`で保持してよい。実装を置く時点で不要な`.gitkeep`は削除する。

## Firmware ownership

firmwareは小さなdevice runtimeに限定する。

所有する:

- 7 channel sensor acquisition
- device-local timestamp / sequence
- stable device identity
- firmware / protocol version
- physical tare / calibration
- persistent calibration / device settings
- bounded sensor diagnostics
- hostとのversioned transport
- explicit command handling

所有しない:

- robot endpoint / joint mapping
- world / tool / task frame
- MuJoCo
- experiment task
- participant evaluation
- GUI rendering

## Host core ownership

host coreはdevice lifecycleとoperator-facing management logicを所有する。

候補責務:

- discovery
- identity resolution
- protocol encode / decode
- compatibility validation
- calibration orchestration
- diagnostics
- configuration persistence on host
- firmware update orchestration
- dual-device inventory

firmware内部のsensor acquisition logicをhostへ複製しない。

## CLI ownership

CLIはautomationとdiagnosticsの第一級interfaceとする。

GUIだけでしか実行できない必須管理操作を作らない。GUIとCLIは可能な範囲で同じhost coreを使用する。

## GUI ownership

Device Manager GUIはdevice setup / calibration / health / visualizationを担当する。

robot simulation、experiment control、MuJoCo viewerを内包しない。

UI frameworkは別decisionで固定する。

## Xpotato-Sim boundary

Xpotato-SimはSelfrionetteをInput Source Pluginとして扱う。

Selfrionette-Deviceから提供されるstable identity、sample、health、protocol metadataをconsumerとして利用し、そこからrobot control intentへのMappingを所有する。

firmware source、calibration wizard、firmware flashing UIをXpotato-Simへ移さない。

## Role binding

`device_id`はphysical device identityである。

`left_hand` / `right_hand`等はhost / experiment roleであり、device identityと同一視しない。

OS serial port名はtransport locationでありidentityではない。

## Deferred components

次は実需要が確認されるまで実装しない。

- resident daemon / service
- generic plugin framework inside this repository
- network transport
- binary wire protocol
- remote firmware update
- cloud device registry
