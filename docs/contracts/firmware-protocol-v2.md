---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - firmware protocol v2
  - USB serial wire grammar
  - management command grammar
related:
  - docs/decisions/0003-versioned-ascii-protocol-v2.md
  - docs/contracts/device-identity.md
  - docs/contracts/device-host-boundary.md
---

# Firmware Protocol v2 Contract

## Scope

Selfrionette firmwareとhost-side device coreのUSB serial contractを定義する。

robot mapping、Xpotato-Sim command semantics、GUI behaviorは対象外。

## Transport

- USB serial
- 115200 baud
- ASCII
- one frame / command per line
- comma-delimited field
- CRLF / LFをhost parserは許容する
- quoting / escaping / multiline fieldは使用しない

## Connection model

firmwareは通常streaming中にもmanagement commandを受け取れる。`info`はstreamを止めずに応答する。`tare`等、sensor stateを変更するcommandはcontractで定義したboundedな期間だけsample出力を一時停止してよい。

hostはport open後、bounded deadline内に`info`を送信し、validな`device` frameを確認してからstable identityを必要とするoperationへ進む。

port openした事実だけをdevice readyの証拠にしない。

## Device frame

```text
device,2,<firmware_version>,<device_id_or_unprovisioned>,7
```

Fields:

1. literal `device`
2. protocol major version: `2`
3. firmware version
4. stable device IDまたはliteral `unprovisioned`
5. channel count: `7`

device frameはsensor sampleではない。

hostはunsupported protocol major、missing field、extra field、invalid ID、channel count mismatchをrejectする。

## Vector frame

```text
vector,<timestamp_ms>,<ch0>,<ch1>,<ch2>,<ch3>,<ch4>,<ch5>,<ch6>
```

- exactly 9 fields
- exactly 7 channel values
- `timestamp_ms`はdevice boot基準のunsigned millisecond counter
- device timestampとhost monotonic clockを直接減算しない
- device_idはvectorへ繰り返し付加しない
- protocol versionはvectorへ繰り返し付加しない

hostはconnection時に確認したdevice metadataをsampleへ関連付ける。

## Status frame

```text
status,<token>[,<arg>...]
```

initial token setは必要なimplementation時に限定して定義する。

既存bring-upで使用していたsensor init / tare progress tokenはmigration inputとして再利用してよいが、無制限なfree-form messageをprotocolへ導入しない。

## Warning frame

```text
warn,<token>[,<arg>...]
```

warnはsensor sampleではない。

hostはwarningをbounded diagnosticとして保持し、warning lineをvectorへ変換しない。

## Canonical host commands

### info

```text
info
```

firmwareはvalidな`device` frameを返す。

streaming中でも使用できる。

### tare

```text
tare
```

physical zero / tare workflowをtriggerする。

exact algorithm、persistence、ready-state semanticsはcalibration contractで別途定義する。

### provision

```text
provision,<device_id>
```

unprovisioned deviceへstable device_idを書き込む。

device_id grammarは[device identity contract](device-identity.md)を正とする。

## Legacy command

legacy `c` commandはmigration firmwareがaliasとして受理してよい。

Protocol v2 consumer / test / docsは`tare`をcanonical commandとして使用する。

legacy aliasを永続public contractとして扱わない。

## Command parser constraints

firmware command parserは:

- fixed-capacity input bufferを使用する
- buffer capacityをcompile-time定数にする
- newlineまで無制限に蓄積しない
- overlength commandを実行しない
- malformed commandをside effectへ変換しない
- unknown commandをsilent state mutationへ変換しない

exact capacityはmemory budget測定後に固定する。

## Interleaving

management responseを待っている間にも`vector` / `status` / `warn`が届き得る。

hostは特定prefixだけを待つblocking parserではなく、lineを分類してboundedに処理する。

例:

```text
vector,...
status,...
device,2,0.1.0,srn-...,7
vector,...
```

`info` callerは途中のvectorをdevice responseと誤認しない。

## Bounded acquisition

host management queryはline count / byte count / deadlineを有限にする。

exact値はhost core implementationとvalidationで固定する。

firmwareもcommand input長を有限にする。

## Compatibility with existing Xpotato-Sim parser

v2の`vector` frame shapeは既存v1と同じである。

そのため、device management capabilityを追加してもsample parserを一度に破壊する必要はない。

ただしXpotato-Simでstable identityを利用するには、接続時device information handshakeとmetadata integrationを別変更で追加する必要がある。

## Non-goals

- binary encoding
- CRC付きsample frame
- sequence number追加
- device_idを毎sampleへ埋め込む
- compression
- network transport
- authentication / encryption
