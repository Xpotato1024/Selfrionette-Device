---
status: canonical
owner: operations
last_verified: 2026-09-29
canonical_for:
  - validation categories
  - firmware resource validation
related:
  - docs/decisions/0001-retain-pro-micro-cpp-firmware.md
  - docs/operations/hardware-safety.md
---

# Validation

## Principle

validationはchanged / materially affected riskに比例させる。compile成功、software test、hardware validationを混同しない。

## Documentation-only

- link / path sanity
- UTF-8 / mojibake check
- `git diff --check`
- Source of Truth Map consistency

## Firmware build

firmwareを変更した場合は最低限:

- PlatformIO compile
- compiler / linker result
- Flash usage
- SRAM usage
- warnings
- protocol / state-machineのhost-test可能部分

resource usageは前回baselineと比較できる形で残す。

旧Simから取り込んだ互換2ターゲットもCIでcompileしFlash/SRAMを記録する。
現行v2の厳しいbudgetを旧referenceの安全性保証に流用しない。詳細は
[旧firmware build](legacy-sim-firmware.md)を参照する。

build成功をhardware validation成功と書かない。

## Host Rust

host-side Rustを変更した場合は必要に応じて:

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- protocol fixture / golden test

## GUI

GUI追加後はframeworkに応じて:

- typecheck / compile
- unit test
- build
- core logicをGUI専用testだけで検証しない

## Protocol

protocol変更では少なくとも:

- valid frame
- malformed frame
- incompatible version
- boundary value
- channel count mismatch
- duplicate / missing identityに関係するhost behavior

をfixtureで検証する。

## Hardware validation

hardware validationはoperator-gatedな別categoryである。

実行時に記録する:

- device identity
- port
- board / firmware revision
- command
- expected behavior
- observed behavior
- stop procedure
- rollback
- persistent state mutation
- sample / log provenance

hardware validationを自動CIへ入れない。
