---
status: proposed
owner: architecture
date: 2026-09-29
canonical_for:
  - stable device identity decision
  - provisioning ownership
related:
  - docs/README.md
  - docs/contracts/device-identity.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/decisions/0003-versioned-ascii-protocol-v2.md
---

# ADR-0004: Stable device identityはhostで生成しEEPROMへprovisionする

## Context

同型のSelfrionetteを二台以上接続すると、COM番号、USB列挙順、接続したportは再接続やPC変更で変化し得る。

一方でATmega32U4に、projectが依存できるglobally unique device identifierが当然に存在すると仮定しない。

deviceを`left` / `right`としてfirmwareへ固定すると、physical identityとexperiment roleが結合する。

## Decision

各Selfrionetteにはhostで生成した128-bit random identifierを一度provisionし、ATmega32U4 EEPROMへbinaryで保存する。

human-facing canonical representation:

```text
srn-<32 lowercase hexadecimal digits>
```

例:

```text
srn-0123456789abcdef0123456789abcdef
```

`device_id`はphysical device identityであり、`left` / `right` / `left_hand` / `right_hand`等のroleを含まない。

## Generation

device_idはhost-side provisioning toolがcryptographically suitableなrandom sourceを使って生成する。

firmware自身がADC noise、boot time、uninitialized SRAM等からIDを推測生成しない。

## Persistent storage

EEPROMには少なくとも次のlogical fieldsを保存する。

- record magic
- storage schema version
- 128-bit device id
- integrity check

exact byte layout、endianness、integrity algorithmはimplementation contractで固定する。

EEPROM recordがinvalid、unknown schema、integrity failure、reserved valueの場合は`unprovisioned`として扱う。

## Provisioning policy

normal provisioningはunprovisioned deviceに対してのみ許可する。

Protocol v2:

```text
provision,<device_id>
```

成功後、firmwareはread-back validationを行い、hostは再度`info`を取得して一致を確認する。

既にvalid device_idを持つdeviceのre-provisionは通常commandの初期要件に含めない。誤provisionやrepairのためのrecovery procedureは別operations contractとして設計する。

## Role binding

role bindingはhost / experiment configurationが所有する。

例:

```text
left_hand  -> srn-...
right_hand -> srn-...
```

firmware EEPROMへ`left` / `right`を保存しない。

friendly nameも初期firmware persistent stateには含めず、host側metadataとして扱う。

## Duplicate handling

firmware単体では他deviceとのduplicateを検出できない。

host discovery / provisioning toolは、接続中deviceと既知registryを確認し、duplicate device_idをfail closedで扱う。

Xpotato-Simは同一session内にduplicate stable identityを検出した場合、port順で推測継続しない。

## Unprovisioned behavior

unprovisioned deviceはbring-up / diagnostics / provisioningのため接続できる。

ただしstable identityを必要とするdual-device experimentではreadyとして扱わない。

Protocol v2 device frame:

```text
device,2,<firmware_version>,unprovisioned,7
```

## EEPROM wear

device_idは通常一度しか書き込まない。

sample loop、tare、通常起動でdevice_id recordを書き直さない。

persistent calibrationはdevice_idとは別record / lifecycleとして設計する。

## Security scope

device_idはauthentication secretではない。

- 秘密情報として扱わない
- host authorizationを証明しない
- firmware signing / secure bootを提供しない

目的はstable physical identityと誤binding防止である。

## Consequences

### Positive

- COM番号とphysical identityを分離できる
- 二台以上でも接続順に依存せず識別できる
- 16 byteのbinary IDでEEPROM costが小さい
- role変更でfirmwareを書き換える必要がない
- host側でQR / label / registryへ同じIDを使用できる

### Negative

- 初回provisioning stepが必要
- EEPROM record formatとrecovery procedureが必要
- duplicate防止はhost側inventoryにも依存する

## Review triggers

- secure device authenticationが必要になる
- factory provisioningを大量生産工程へ移す
- MCU変更でhardware-backed unique IDを利用できる
- identity以外のpersistent metadataが増え、EEPROM layoutを再設計する必要が生じる
