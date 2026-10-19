# Selfrionette Pro Micro 7ch Firmware

Selfrionetteのcurrent Pro Micro / ATmega32U4 firmware。

## Hardware

- Board: SparkFun Pro Micro compatible / ATmega32U4 16 MHz
- Load-cell frontend: existing 7-channel HX717/HX711-compatible acquisition path
- USB serial: 115200 baud
- Protocol v2 compatibility sample rate: 80 Hz
- opt-in Protocol v3: 80 Hzの送信試行を既定とする開発候補（実機freshレートは未確認）

pin assignmentと実機wiringのcurrent specificationは、hardware inventoryを移行・検証した後に`hardware/`へ正本化する。

## Build

```bash
cd firmware/loadcell_7ch
SELFRIONETTE_SOURCE_SHA="$(git rev-parse HEAD)" pio run -e pro_micro_7ch
```

build requires `SELFRIONETTE_SOURCE_SHA`, a nonzero 40-character lowercase Git SHA, which is embedded in Protocol v3 `info3` output. CI supplies the exact checked-out source revision. Compileはhardware validationではない。upload / serial open / EEPROM mutationは[hardware safety](../../docs/operations/hardware-safety.md)に従う。

### 20/40/80 Hzの候補を比較する

| PlatformIO environment | sample3送信試行上限 | 現候補が返すFirmware版 |
|---|---:|---|
| `pro_micro_7ch`（既定） | 80 Hz / 12,500 μs | `1.1.0-preview.1` |
| `pro_micro_7ch_40hz` | 40 Hz / 25,000 μs | `1.1.0-preview.1.40hz` |
| `pro_micro_7ch_20hz` | 20 Hz / 50,000 μs | `1.1.0-preview.1.20hz` |

上のbuild commandの`-e`を対象environmentへ変更する。複数environmentの選択や指定なしのbuildでも、自動uploadは行わない。rateは`custom_quality_stream_rate_hz`から本体版と一緒に注入し、20/40/80以外は拒否する。既定版は`firmware/editions.json`を正とする。runtimeのrate変更commandは追加しない。

これらは同じ実装の比較buildであり、旧正式20 Hz版や別PRの40 Hz版とbit-for-bitで同一ではない。通常の配布packageは既定80 Hz候補だけを対象とする。比較buildの環境・source SHA・本体版・HEX SHA-256を別に記録する。[実機比較・退避手順](../../docs/operations/quality-stream-rate-acceptance.md)が揃うまで書き込まない。

## Protocol v2

producer contract:

- [Firmware Protocol v2](../../docs/contracts/firmware-protocol-v2.md)
- [Firmware Protocol v3](../../docs/contracts/firmware-protocol-v3.md)
- [Device Identity](../../docs/contracts/device-identity.md)
- [Device Persistent Settings](../../docs/contracts/device-persistent-settings.md)

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

## Protocol v3

v3は`info3`の後にhostが明示的にsessionを開始した場合だけ送信する。raw count、quality flags、settings generation、session tareの形式と失敗時の扱いはcanonical contractを参照する。取得timeout後も前値をrawとして示すが、held/timeout flagsを付けてfresh sampleとしては扱わない。

`tare3`は旧zeroを直ちに無効にし、7chすべてのfresh・非飽和sampleを各8個集める。全channelが2秒以内に揃い、各channelのmax-minが2000 count以下の場合だけ平均をsession zeroとして確定する。この2000-count条件はsoftware gateであり、計量学的精度の保証ではない。設定の実変更、新しいsession challenge、tare失敗ではv3のtareを有効のままにしない。同一challengeの`stream3 stop` / `stream3 start`はbounded pause/resumeとしてsequenceと成功済みtareを保持するが、pause前のfresh/rejected frame状態は捨て、resume処理後に取得したsampleからfreshとして扱う。

## Memory discipline

- receive buffer: fixed 88 bytes (NUL込み); v3 command payload upper bound is 87 bytes, v2 command payload remains limited to 63 bytes
- transmit line queue: fixed 256 bytes; output is drained in bounded chunks using `Serial.availableForWrite()`
- dynamic `String`不使用
- status / warning tokenはAVR flash string helperを使用
- sample / diagnostic historyをfirmware内へ蓄積しない
- EEPROMへsample loopからwriteしない

2026-10-19 pinned PlatformIO buildでは、全3環境のstatic SRAMは1019 / 2560 bytes、Flashは80 Hzで20564 bytes、40/20 Hzで20574 bytesだった。SRAMは1024-byte gateまで5 bytesで、runtime stack peakは未測定。[memory budget contract](../../docs/contracts/firmware-memory-budget.md)と[software-only検証記録](../../docs/reports/2026-10-19-quality-stream-80hz.md)を参照する。取得品質の判定・診断・buffer上限を削減してgateを通したものではない。

## Provenance

`HX711.h` / `HX711.cpp`は旧Xpotato-Sim firmwareから移行した、Bogdan Necula氏のHX711 Arduino library由来のMIT License実装を保持する。

Upstream `read()` / `wait_ready()` can wait until DOUT becomes ready ([source](https://github.com/bogde/HX711/blob/master/src/HX711.cpp)). Current firmware checks readiness before reading and uses finite readiness waits for bounded setup calibration. This change does not establish the cause or repair of the observed `-65536` impulse.

license全文は[THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)を参照する。

旧firmwareはmigration referenceであり、このrepositoryのcurrent firmwareが移行後のsource of truthになる。
