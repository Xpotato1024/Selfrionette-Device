---
status: accepted
owner: architecture
date: 2026-09-29
canonical_for:
  - initial MCU selection
  - firmware language selection
  - AVR memory constraints
related:
  - docs/README.md
  - docs/architecture/repository-boundaries.md
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
---

# ADR-0001: Pro Microを継続し、初期firmwareはC++で実装する

## Context

Selfrionette Deviceは、既存の表面実装済みPro Micro系hardwareを利用して7 channelのload-cell frontendを取得する研究用入力デバイスである。

現在の実機系ではATmega32U4搭載Pro Micro、USB serial、7 channel取得が成立しており、hardwareを変更すると基板再製作、配線・bring-up・再検証が必要になる。今回の優先事項は新MCUへの置換ではなく、二台運用、device identity、calibration、diagnostics、設定管理を安定して成立させることである。

RustによるAVR firmwareも候補に含めて検討したが、2026-09時点で次の制約がある。

- Rustの`avr-none` targetはTier 3であり、Rust Projectによるbuild/test保証対象ではない。
- `avr-none`は`avr-gcc`と`build-std=core`を必要とする。
- AVR向けRust HAL ecosystemは存在するが、ATmega32U4 / Pro MicroのUSBを含むproject-readyな経路を当然には仮定できない。
- Embassy executorにはAVR向けの実験的なplatform supportが見られるが、Pro Micro / ATmega32U4を対象に、USB、HAL、async I/Oまで一貫した成熟経路として採用できる状態とは判断しない。
- AVR Rust stackは発展途上であり、toolchain / HAL / USB周辺のecosystem固有問題を切り分ける追加コストを、研究日程を優先する本projectではmaterialなriskとして扱う。

一方、ATmega32U4自体にも明確なresource制約がある。

- Flash: 32 KB
- SRAM: 2.5 KB
- EEPROM: 1 KB
- 8-bit AVR / 最大16 MHz級

bootloaderやlink結果によりapplicationが実際に使用できるFlash量は小さくなる可能性があるため、32 KB全量をapplication budgetとして扱わない。

## Decision

初期Selfrionette Device firmwareでは以下を採用する。

1. MCU / boardは既存Pro Micro（ATmega32U4）を継続する。
2. firmwareはC++で実装する。
3. build / upload toolchainはPlatformIO + AVR/Arduino系を第一候補とする。
4. Rustへの移行は初期firmwareの要件に含めない。
5. firmware設計ではFlash / SRAM / EEPROMを明示的な有限resourceとして扱う。
6. host側toolingの言語選択はfirmware言語と独立させる。host core / CLI / GUI backendではRustを使用できる。
7. MCU変更またはRust firmware移行は、新しいADRでこのdecisionをsupersedeする場合だけ行う。

## Memory policy

ATmega32U4の制約を前提に、firmwareでは次を標準とする。

- heap allocationを通常経路で使用しない。
- Arduino `String`等、heap fragmentationを起こし得る所有文字列をhot pathへ導入しない。
- protocol buffer、diagnostic buffer、sample bufferは固定長またはbounded構造にする。
- 7 channel sampleを必要以上に履歴保持しない。統計量は可能ならstreaming計算する。
- recursionを避ける。
- large local arrayをstackへ置かない。
- calibration、identity、diagnostic、streamingで同じ情報を重複保持しない。
- firmware build時にFlash / SRAM使用量を記録し、増加をレビュー対象にする。
- EEPROM writeは寿命を考慮し、毎sample / 毎tick書込みを禁止する。
- protocol version、device identity、persistent calibrationの保存形式はversion付きcontractで定義する。

初期段階では任意のpercentage thresholdを固定しない。現行firmwareをbaselineとしてbuild後の実測resource usageを取得し、その値を基にbudget thresholdを別contractで確定する。

## Concurrency policy

初期firmwareにRTOSを導入しない。

取得、USB serial command、calibration、diagnosticsは、boundedで明示的なstate machine / cooperative loopとして設計する。blocking operationを無制限に待たせず、timeoutまたは状態遷移を明示する。

async/awaitを使えないこと自体を理由に独自executorを実装しない。

## Consequences

### Positive

- 表面実装済みhardwareを再利用できる。
- 既存bring-up資産と測定結果を継承しやすい。
- firmware migrationに研究時間を消費しにくい。
- Arduino / AVR toolingによるhardware debugging経路を維持できる。
- host側Rust採用とfirmware側C++採用を独立に最適化できる。

### Negative

- Flash / SRAM / EEPROM制約が厳しい。
- 32-bit MCU向けRust ecosystemほど型・async abstractionを活用できない。
- USB、sampling、command処理を小さなresource内で設計する必要がある。
- firmware側に高度なUI / storage / update orchestrationを持たせる余裕は少ない。

### Mitigation

device management、calibration workflow、設定編集、visualization、firmware update orchestrationはhost側へ寄せる。firmwareはsensor acquisition、persistent device state、bounded diagnostics、versioned transportに集中させる。

## Alternatives considered

### Rust on current Pro Micro

採用しない。

理由はRustそのものではなく、AVR target / HAL / USB pathの成熟度と研究日程に対するdebug riskである。将来toolchainとATmega32U4 supportが十分安定し、実機で同等以上の再現性が確認された場合は再評価できる。

### MCUをRP2040 / RP2350 / STM32等へ変更

現段階では採用しない。

技術的には有力だが、表面実装済みhardwareの再製作とbring-upが必要で、現在の目的に対する時間コストが大きい。

### 現行firmwareを無変更で継続

採用しない。

hardwareは維持するが、device identity、versioned protocol、calibration ownership、diagnostics、二台運用、memory budgetを正式に設計し直す。

## Review triggers

次のいずれかが成立した場合、このADRを再検討する。

- ATmega32U4 resource不足が実測でblockerになる。
- required sample rate / channel count / USB機能が現行MCUで成立しない。
- hardware revisionを作り直す別理由が発生する。
- AVR Rust / ATmega32U4 USB stackがproject-readyになり、実機比較で明確な利点が確認できる。
- firmware要件がRTOS / async executorを必要とする規模へ拡大する。

## References

- Microchip ATmega32U4 product page: https://www.microchip.com/en-us/product/atmega32u4
- Rust AVR target support: https://doc.rust-lang.org/nightly/rustc/platform-support/avr-none.html
- Rust target tier policy: https://doc.rust-lang.org/rustc/target-tier-policy.html
- avr-hal: https://github.com/Rahix/avr-hal
- avr-hal Arduino Micro / ATmega32U4 discussion: https://github.com/Rahix/avr-hal/discussions/691
- Embassy executor AVR build metadata: https://github.com/embassy-rs/embassy/blob/main/embassy-executor/Cargo.toml
