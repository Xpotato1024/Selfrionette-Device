---
status: accepted
owner: architecture
date: 2026-10-01
canonical_for:
  - host device core language
  - initial CLI architecture
related:
  - docs/README.md
  - docs/contracts/host-device-core.md
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
---

# ADR-0005: Host device coreとCLIをRust workspaceで実装する

## Context

Selfrionette-Deviceではfirmwareとhost-side device managementを分離する。

host側には今後、protocol parse、stable identity、device discovery、calibration orchestration、diagnostics、CLI、Device Manager GUIが必要になる。GUI専用logicやPython script群として分散させると、automationとGUIで同じdevice semanticsを二重実装することになる。

firmwareはATmega32U4制約のためC++を採用したが、その判断をhost側へそのまま適用する理由はない。

## Decision

host-side reusable device logicとCLIはRust workspaceで実装する。

初期workspace:

- `selfrionette-core`: I/O非依存のdevice identity / protocol / compatibility logic
- `selfrionettectl`: coreを利用する薄いCLI

初期foundationでは外部crate dependencyを追加せず、OS serial backend、daemon、GUIは含めない。

## Boundary

`selfrionette-core`はserial portを直接openしない。

protocolとidentityのpure logicを先に固定し、OS transportは別layerとして追加する。

GUIは将来同じcoreを再利用し、CLI subprocessをbusiness logic APIとして利用しない。

## CLI policy

CLIはautomation可能なfirst-class interfaceとする。

初期developer surface:

- protocol line parse
- device ID validation
- canonical management command encoding

実serialの`list / info / tare / provision / monitor`はtransport layer追加後に拡張する。

## Consequences

### Positive

- GUI / CLI / future automationでprotocol semanticsを共有できる
- firmware C++とhost Rustを責務ごとに独立最適化できる
- pure logicをhardwareなしでunit testできる
- serial backend導入前にprotocol compatibilityを固定できる

### Negative

- C++ firmwareとRust hostの二つのtoolchainを持つ
- cross-language contract synchronizationが必要
- serial backend導入まではCLI単体で実device管理はできない

## Review triggers

- Rust toolchainがdeployment上のblockerになる
- CLI / GUIでcore boundaryが不適切と判明する
- daemonを導入しserial ownershipを一元化する
