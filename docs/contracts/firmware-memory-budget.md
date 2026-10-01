---
status: canonical
owner: architecture
last_verified: 2026-10-01
canonical_for:
  - ATmega32U4 firmware memory budget
  - firmware build resource gate
related:
  - docs/decisions/0001-retain-pro-micro-cpp-firmware.md
  - docs/operations/validation.md
  - firmware/loadcell_7ch/README.md
---

# Firmware Memory Budget Contract

## Target

current firmware target:

- MCU: ATmega32U4
- board: SparkFun Pro Micro 5V / 16 MHz compatible
- physical SRAM: 2560 bytes
- physical Flash: 32768 bytes
- PlatformIO application Flash capacity: 28672 bytes

bootloader / board configurationのため、CI budgetは32 KB physical FlashではなくPlatformIOがreportする28672 bytesを基準にする。

## Baseline

2026-10-01、Protocol v2 / device identity foundation build:

- PlatformIO Core: 6.1.19
- environment: `pro_micro_7ch`
- static RAM: 485 / 2560 bytes (18.9%)
- application Flash: 11172 / 28672 bytes (39.0%)

この値はsoftware-only compile evidenceであり、runtime stack high-water markまたはhardware stabilityの証拠ではない。

## Hard gates

| Resource | CI maximum | Remaining minimum at gate |
|---|---:|---:|
| build-reported static SRAM | 1024 bytes | 1536 bytes |
| application Flash | 24576 bytes | 4096 bytes |

`tools/check_firmware_size.py`とGitHub Actionsでこのgateを適用する。

## SRAM interpretation

PlatformIOのRAM usageは主にglobal / static allocationを表し、runtime stack peakを完全には表さない。

したがってstatic SRAMが1024 bytes以下でも次を守る。

- large local arrayを追加する場合はサイズをreviewする
- recursionを追加しない
- dynamic heap allocationをnormal pathへ追加しない
- unbounded buffer / queueを追加しない
- `String`等のheap-owning objectをhot pathへ追加しない
- stack使用量が不明なままstatic SRAM gateを緩和しない

current calibration pathはlocal working bufferを使用するため、static SRAM headroomをstack用に十分残す。

## Flash interpretation

24576 bytes gateはapplication capacity 28672 bytesに対し4096 bytes以上の余白を残す。

gate超過が必要な場合は、まず不要code / diagnostic text / duplicate abstractionを削減する。機能要件上 unavoidableな場合だけbudget変更をdesign reviewする。

## EEPROM partition

ATmega32U4 EEPROM total: 1024 bytes。

initial partition:

| Range | Owner |
|---|---|
| 0..31 | device identity area |
| 32..1023 | reserved for future persistent state |

identity schema v1自体は0..22の23 bytesだけを使用する。23..31はidentity schema extension用に予約する。

persistent calibrationを実装する場合、address 32以降を使用し、identity recordと独立したversioned schemaにする。

## Change policy

firmware changeでは:

1. PlatformIO compileを成功させる
2. static RAM / Flashを記録する
3. hard gateを確認する
4. baselineからmaterialに増加した場合は理由をPRへ記録する

budget thresholdをimplementation都合だけで引き上げない。
