---
status: historical
owner: firmware
date: 2026-10-19
related:
  - docs/contracts/firmware-protocol-v3.md
  - docs/contracts/firmware-memory-budget.md
  - docs/operations/quality-stream-rate-acceptance.md
---

# Protocol 3最大80 Hz候補のsoftware-only検証

Issue #24。最新main `ba37a3f44a03fa0c7aca8725f2d0240b3e158e26`を起点とし、前段Draft PR #23（`e951573`）のwrap対応・catch-upしない送信周期という設計を再利用した。前段PRの状態や正式版1.0.0の配布物は変更していない。

## 変更と設計判断

既定80 Hzに加えて同じ実装の40/20 Hz buildを用意した。rateと本体版をbuild時にまとめて注入し、fallbackの`.40hz` / `.20hz` suffixでsource SHAが同じ場合もprofile bindingを区別する。未契約のrate commandや能力照会fieldは追加しない。

送信判定は4-byteのlast-attempt時刻に対するunsigned経過時間とした。前段のsigned deadline比較の半周制限を避け、2^31 μsを超える一回の長い処理後でも最新frameを一度だけ試行する。送信失敗時も過去の予定を蓄積しない。sequenceとfresh消費はline全体のenqueue成功後に確定する。

## 検証結果

`firmware/tests/quality_stream.cpp`は現行`main.cpp`のnamespaceと`loop()`をそのまま抽出し、ADC、Arduino Print、USB Serial、EEPROMをfakeに置き換えて実行する。startupのboard依存部分とHX711の物理bit読出しはこのfixtureの範囲外で、AVR buildで別にcompileする。

- 20/40/80 Hz全てで期限直前/一致、micros rollover、2^31 μs超stall後の非burst復帰がPASS。
- USB空き0と「空きあり・write進捗0」、1 byteずつのshort write、10周期のbackpressure後に最新rawから再開する試験がPASS。queue内の旧sampleをdrain時刻へ書き換えない。
- command reserve、停止commandを待っている間の取得、same-session pause/resume、新challenge、tare成功/取消/timeout、最大request IDの非同期replyがPASS。
- fresh/held/timeout/saturated/rejected/未取得の7ch混在で、fresh単独以外のorientedを出さない試験がPASS。
- native parser/quality/tare/settings、build版識別と不正rate拒否、既存command safety、配布版のregistry照合を検証する。CIは同じnative fixtureと全3環境のメモリgateを実行する。

次は**時計・ADCを模擬した1秒のsoftware試験**であり、実測機器の数値ではない。全7chを同位相にし、成功したtare/settingsを仮定した。

| 送信設定 | 模擬ADC変換/s（各ch） | frame/s | fresh/s（各ch） | 全7ch同時usable/s |
|---:|---:|---:|---:|---:|
| 20 Hz | 10 | 20 | 10 | 10 |
| 40 Hz | 10 | 40 | 10 | 10 |
| 80 Hz | 10 | 80 | 10 | 10 |
| 20 Hz | 80 | 20 | 20 | 20 |
| 40 Hz | 80 | 40 | 40 | 40 |
| 80 Hz | 80 | 80 | 80 | 80 |

前値再送をfreshにしないことを確認した。新しいADC変換で数値が同一になる場合はfreshになり得るため、入力値を変化させるだけの試験にはしていない。

## リソースとprovenance

PlatformIO Core 6.1.19、atmelavr 5.2.0、framework-arduino-avr 5.3.0、AVR GCC 7.3.0を使用した。toolchain/avrdude/sconsの指定は`platformio.ini`に固定する。開発中の初回compileは上記base SHAを注入したworking-tree差分で行い、出荷物には使わない。clean commitの正確なSHAを注入したbuildとHEX hashの保全を配布時に別途行う。

| Environment | static SRAM / 2560 B | Flash / 28672 B | Gate |
|---|---:|---:|---|
| `pro_micro_7ch` | 1019 B | 20564 B | PASS |
| `pro_micro_7ch_40hz` | 1019 B | 20574 B | PASS |
| `pro_micro_7ch_20hz` | 1019 B | 20574 B | PASS |

同一toolchainでbase mainを`git archive`から独立にbuildし、static SRAM 1019 B / Flash 20548 Bを確認した。候補はstatic SRAM増加0 B、Flash増加16 B（80 Hz）または26 B（40/20 Hz）である。

static SRAM gateは1024 B、Flash gateは24576 Bを維持した。SRAM余白は5 Bであり、stack high-water markは未測定。heap・sample履歴・追加queueは導入していない。fallbackの本体版suffixが既定候補とのFlash差10 Bに対応する。

## 未実施

実serial open、実機flash、EEPROM操作、物理tare、ADC型番/RATE確認、実効fresh/s、USB/AVR CPU timing、実受信間隔p50/p95/p99、操作改善、長時間運転、−65536 impulse原因確認は実施していない。物理受入は#26と[比較手順](../operations/quality-stream-rate-acceptance.md)に従う。この記録は80 Hzの実効fresh入力や公開済み1.1.0を意味しない。
