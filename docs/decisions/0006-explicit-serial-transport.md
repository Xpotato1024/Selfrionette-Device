---
status: accepted
owner: architecture
date: 2026-10-01
canonical_for:
  - host serial transport library
  - explicit-port device management
related:
  - docs/README.md
  - docs/contracts/serial-transport.md
  - docs/contracts/host-device-core.md
---

# ADR-0006: Serial transportを独立crateに分離し、explicit portだけをopenする

## Context

host coreはI/O非依存として成立した。実device managementにはWindows / Linux / macOSでUSB serialを扱うtransportが必要になる。

一方、OSが列挙するCOM番号やdevice pathはstable device identityではなく、候補portを無差別にopenすると他deviceへの副作用やPro Microの状態変化を招く可能性がある。

## Decision

`selfrionette-serial` crateを追加し、`selfrionette-core`からOS serial dependencyを分離する。

initial backendはRust `serialport` crate 4.10.1を使用する。

Linux native `libudev` dependencyを必須化しないためdefault featuresを無効にする。port enumeration metadataが限定される可能性を受け入れ、列挙結果をidentityとして使用しない。

## Port access policy

initial CLIは次を提供する。

- `list`: port名列挙のみ。portをopenしない
- `info --port <port>`: operatorが指定した1 portだけopenする
- `provision --port <port> --id <id> --yes`: 指定portだけopenし、preflight / write / read-back verifyを行う

全候補portを自動openしてSelfrionetteを探索する機能は初期scopeに含めない。

## Query bounds

initial management query:

- baud: 115200
- per-line maximum: 1024 bytes
- overall deadline: 5 seconds
- line budget: 64
- serial read slice: 20 ms

startup calibration等でresponseが遅れる可能性を考慮し、runtime sample stale boundaryとは別のmanagement deadlineを使う。

## Provision safety

provisionは:

1. input queueをdiscard
2. `info`
3. compatibleかつ`unprovisioned`を確認
4. `provision,<id>`
5. `status,provision_ok`を確認
6. 再度`info`
7. exact ID一致を確認

CLIではさらに`--yes`を必須にする。

## Tare

real serial `tare` commandはこのdecisionではCLI公開しない。

現行firmwareはchannel単位のcalibration失敗があっても最終`calibration_end`を出し得るため、成功判定contractを確定してから追加する。

## Consequences

- coreのpure protocol semanticsを維持できる
- Windows / Linux双方のtransport implementationを一箇所へ閉じ込められる
- port enumerationとstable identityを混同しない
- 自動discoveryよりoperator指定が必要だが、初期hardware operationを安全側に保てる

## References

- https://docs.rs/serialport/4.10.1/serialport/
