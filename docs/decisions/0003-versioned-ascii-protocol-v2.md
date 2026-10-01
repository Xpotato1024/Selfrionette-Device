---
status: accepted
owner: architecture
date: 2026-09-29
canonical_for:
  - initial device protocol v2 architecture decision
related:
  - docs/README.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/contracts/device-identity.md
  - docs/decisions/0001-retain-pro-micro-cpp-firmware.md
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
---

# ADR-0003: Protocol v2は既存sample frameを維持し、management frameを追加する

## Context

既存Selfrionette firmwareはUSB serial 115200 baud上で、次のline-based ASCII frameを送信する。

- `vector,<timestamp_ms>,<ch0>...<ch6>`
- `status,...`
- `warn,...`

実機では7 channelを約80 Hzで取得できている。Xpotato-Sim側にもこの`vector` grammarを前提としたparser、fixture、finite acquisition policyが既に存在する。

一方、二台接続とdevice managementには少なくとも次が不足している。

- stable device identity
- protocol version
- firmware version
- explicit device information query
- provisioning result
- bounded management command grammar

ATmega32U4はFlash 32 KB / SRAM 2.5 KBであり、identityや長いmetadataを80 Hzのsample frameへ毎回付加する設計は、帯域・Flash・SRAMのいずれにも利益が小さい。

## Decision

Protocol v2はline-based ASCII / comma-delimited / newline-terminatedを維持する。

### Sample compatibility

高頻度sampleは次のgrammarを維持する。

```text
vector,<timestamp_ms>,<ch0>,<ch1>,<ch2>,<ch3>,<ch4>,<ch5>,<ch6>
```

Protocol v2であることを示すために、sample frameへversion fieldやdevice_idを追加しない。

connection/session開始時にdevice informationを別frameで取得し、sampleはそのsessionのdeviceに属するものとしてhostが関連付ける。

### Management information

Protocol v2ではdevice information frameを追加する。

```text
device,2,<firmware_version>,<device_id_or_unprovisioned>,7
```

このframeはsampleではなくmanagement / identity informationである。

### Commands

host -> firmware commandはboundedなnewline-terminated ASCII commandとする。

初期canonical command:

```text
info
tare
provision,<device_id>
```

command inputは固定長bufferで受信し、上限超過をrejectする。exact buffer sizeはfirmware memory budget contractで決定する。

既存のsingle-character `c` calibration commandはmigration compatibilityとして一時的に受理してよいが、Protocol v2のcanonical commandではない。

### Diagnostics

`status` / `warn` prefixを維持し、host parserがsampleとdiagnosticを区別できる状態を保つ。

firmware内で長いhuman-readable error textを生成せず、boundedなtoken / numeric fieldを優先する。

## Why not version every sample

Protocol version、firmware version、device identityはsession内で通常不変である。

これらを80 Hzで送信しても観測価値は増えず、serial bandwidthとformatting costだけが増える。hostは接続時にdevice informationを確認し、そのsession metadataとして保持する。

## Why keep ASCII

初期v2でbinary protocolへ移行しない。

- 現行115200 baud / 約80 HzでASCII streamは成立している
- serial monitorで人間が直接確認できる
- fixtureを小さなtextとして保存できる
- Xpotato-Simの既存parser資産を段階的に移行できる
- CRC / framing / escaping / binary codecを追加する必要がない

binary化は帯域、latency、reliabilityの実測がblockerになった場合に別ADRで検討する。

## Host parser behavior

hostはprefixでframeを分類する。

- `vector`: sensor sample
- `device`: protocol / firmware / identity information
- `status`: state / command progress
- `warn`: recoverable or degraded diagnostic
- unknown prefix: sampleとして受理しない

management responseとsampleは同一stream上でinterleaveしてよい。hostは特定response待ちの間も他prefixをboundedに処理する。

## Failure policy

次をfail closedとする。

- unsupported protocol version
- malformed `device` frame
- wrong channel count
- overlength command
- malformed provisioning id
- management queryがbounded deadline / line budget内に完了しない

失敗時にport番号や接続順からidentityを推測しない。

## Consequences

### Positive

- high-rate sample pathのwire overheadを増やさない
- existing Xpotato-Sim `vector` parserとの段階移行が可能
- operatorがserial monitorで観察できる
- firmware parserを固定長bufferで実装できる
- stable identityとfirmware compatibilityをsession metadataとして保持できる

### Negative

- protocol versionが各sample self-describingではない
- session開始時のidentity handshakeが必須になる
- management responseとsampleのinterleaveをhostが正しく扱う必要がある

## Review triggers

- 115200 baudが実測上のbottleneckになる
- sample rate / channel countが増える
- error detection用CRCが必要になる
- USB serial以外のtransportを採用する
- sample単体をsession contextなしで永続保存する要件が生じる
