# Selfrionette Pro Micro 7ch Firmware

Selfrionetteのcurrent Pro Micro / ATmega32U4 firmware。

## Hardware

- Board: SparkFun Pro Micro compatible / ATmega32U4 16 MHz
- Load-cell frontend: existing 7-channel HX717/HX711-compatible acquisition path
- USB serial: 115200 baud
- target sample rate: 80 Hz

pin assignmentと実機wiringのcurrent specificationは、hardware inventoryを移行・検証した後に`hardware/`へ正本化する。

## Build

```bash
cd firmware/loadcell_7ch
pio run -e pro_micro_7ch
```

compileはhardware validationではない。upload / serial open / EEPROM mutationは[hardware safety](../../docs/operations/hardware-safety.md)に従う。

## Protocol v2

producer contract:

- [Firmware Protocol v2](../../docs/contracts/firmware-protocol-v2.md)
- [Device Identity](../../docs/contracts/device-identity.md)

high-rate sampleは既存shapeを維持する。

```text
vector,<timestamp_ms>,<ch0>,<ch1>,<ch2>,<ch3>,<ch4>,<ch5>,<ch6>
```

management commands:

```text
info
tare
provision,srn-<32 lowercase hexadecimal digits>
```

legacy `c` lineは一時的に`tare` aliasとして受理する。

## Identity storage

- 128-bit device ID
- EEPROM record: 23 bytes
- magic: 4 bytes
- schema version: 1 byte
- device ID: 16 bytes
- CRC-16/CCITT-FALSE: 2 bytes
- valid identityの通常overwriteは禁止

exact semanticsはcanonical identity contractを参照する。

## Memory discipline

- command buffer: fixed 64 bytes
- dynamic `String`不使用
- status / warning tokenはAVR flash string helperを使用
- sample / diagnostic historyをfirmware内へ蓄積しない
- EEPROMへsample loopからwriteしない

Flash / SRAMの実測値はCI buildからbaseline化し、memory budget contractへ反映する。

## Provenance

`HX711.h` / `HX711.cpp`は旧Xpotato-Sim firmwareから移行した、Bogdan Necula氏のHX711 Arduino library由来のMIT License実装を保持する。

license全文は[THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)を参照する。

旧firmwareはmigration referenceであり、このrepositoryのcurrent firmwareが移行後のsource of truthになる。
