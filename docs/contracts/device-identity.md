---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - stable device identity format
  - provisioning semantics
  - identity / role separation
related:
  - docs/decisions/0004-eeprom-provisioned-device-identity.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/contracts/device-host-boundary.md
---

# Device Identity Contract

## Device ID

canonical text representation:

```text
srn-<32 lowercase hexadecimal digits>
```

properties:

- prefix: `srn-`
- payload: exactly 128 bits
- text payload: exactly 32 lowercase hexadecimal digits
- no spaces
- no UUID hyphens inside payload
- case-foldingによる曖昧さを避けるためcanonical outputはlowercase

Example:

```text
srn-0123456789abcdef0123456789abcdef
```

example値を実deviceへprovisionしない。

## Binary representation

firmware persistent stateでは128-bit payloadを16 bytesとして保持する。

text prefixやhex textをEEPROMへそのまま保存する必要はない。

## Reserved / invalid states

次はvalid device identityではない。

- all-zero 128-bit value
- invalid EEPROM record
- unsupported storage schema
- failed integrity check
- malformed text

firmwareはこれらを`unprovisioned`としてreportする。

## EEPROM record

initial logical record:

```text
magic
schema_version
device_id[16]
integrity
```

current schema version 1のrecordはEEPROM address 0から23 bytesを使用する。

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII magic `SRN2` |
| 4 | 1 | schema version `1` |
| 5 | 16 | 128-bit device ID payload |
| 21 | 2 | CRC-16/CCITT-FALSE, big-endian |

CRC parameters:

- polynomial: `0x1021`
- initial value: `0xFFFF`
- input: magic + schema version + 16-byte device ID
- stored high byte first
- no final XOR

C++ structのnative paddingをwire / EEPROM formatとして暗黙利用しない。

provision時はmagicを先にinvalid化し、schema / payload / CRCを書いた後にmagicを最後に書く。power lossによるpartial writeをvalid recordとして誤認しないためである。

## Provisioning

1. hostが128-bit random IDを生成する
2. target portをopenする
3. `info`で`unprovisioned`を確認する
4. `provision,<device_id>`を送る
5. firmwareがformat validationを行う
6. EEPROMへwriteする
7. firmwareがread-back / integrity validationする
8. hostが再度`info`を行う
9. returned device_idがrequested IDと一致することを確認する
10. host registry / label等へ記録する

step 8-9が成立する前にprovisioning成功と報告しない。

## Existing identity

valid identityが既に存在する場合、initial normal provisioning commandはoverwriteしない。

re-provision / repairは通常operationとは別のmaintenance procedureとして設計する。

## Role and friendly name

次をdevice_idへ含めない。

- left / right
- left_hand / right_hand
- participant
- experiment number
- COM port
- PC hostname
- friendly name

これらはhost-side configuration / registryが所有する。

## Duplicate ID

hostは同一sessionでduplicate stable IDを検出した場合fail closedとする。

port order、physical position、last-used portから自動解決しない。

## Labels

physical label / QRへdevice_idを表示またはencodeしてよい。

labelはoperator verificationの補助であり、software identityの代替SoTではない。

## Logging

実験 / hardware validation logでは、必要に応じて次を記録する。

- device_id
- ephemeral port
- firmware version
- protocol version

portはtransport evidenceでありidentityではない。

## Privacy / security

device_idはsecretではない。

authentication token、credential、participant identifierとして再利用しない。
