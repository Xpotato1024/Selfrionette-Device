---
status: canonical
owner: architecture
last_verified: 2026-10-01
canonical_for:
  - host core protocol semantics
  - initial selfrionettectl surface
related:
  - docs/decisions/0005-rust-host-core-cli.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/contracts/device-identity.md
---

# Host Device Core Contract

## Scope

`selfrionette-core`はfirmware protocolとdevice identityをhost側で扱うpure logicを所有する。

このcontractはOS serial open、port enumeration、GUI、Xpotato-Sim integrationをまだ含まない。

## Device identity

coreの`DeviceId`はcanonical `srn-<32 lowercase hex>`をparse / formatする。

次をrejectする。

- wrong prefix
- wrong byte length
- uppercase hex
- non-hex
- all-zero payload

## Protocol parse

`parse_line`はProtocol v2 lineを次へ分類する。

- `Device`
- `Vector`
- `Status`
- `Warning`
- `Unknown`

unknown prefixはsampleとして扱わない。

### Device

exactly 5 fieldsを要求する。

parsed device infoに対しcompatibility checkは:

- protocol major = 2
- channel count = 7

を要求する。

unprovisionedはparse可能だが、stable identity必須operationでは別layerがready扱いしない。

### Vector

exactly 9 fields、7 finite float channelを要求する。

timestampはfirmwareの32-bit unsigned `millis()`に合わせ`u32`で保持する。

### Diagnostic

`status` / `warn`はnon-empty tokenを要求し、残りfieldをargumentとして保持する。

## Host command encoding

coreはcanonical commandのみencodeする。

- `info\n`
- `tare\n`
- `provision,<device_id>\n`

legacy `c` aliasをhost coreから生成しない。

## CLI surface

pure protocol / developer commands:

```text
selfrionettectl parse-line <line>
selfrionettectl validate-id <device-id>
selfrionettectl encode info
selfrionettectl encode tare
selfrionettectl encode provision <device-id>
```

live serial commands:

```text
selfrionettectl list
selfrionettectl info --port <port>
selfrionettectl provision --port <port> --id <device-id> --yes
```

live serialのownership、bounded query、mutation gateは[serial transport contract](serial-transport.md)を正とする。

real serial `tare`はcalibration success contract確定前のためまだ公開しない。

## Failure policy

invalid protocol / identityをsilent correctionしない。

compatibility failureをport順、firmware version文字列、過去bindingから推測補完しない。
