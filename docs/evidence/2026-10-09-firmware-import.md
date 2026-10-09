---
status: historical
owner: firmware
last_verified: 2026-10-09
canonical_for: []
related:
  - docs/operations/legacy-sim-firmware.md
---

# Sim firmware取り込み検証記録

## Source identityと復元

- 移行元: [Xpotato-Sim commit](https://github.com/Xpotato1024/Xpotato-Sim/commit/552180b65d2055b63854cd3a25bc1d2e9308fb36)、firmware配下15ファイル。
- 元firmware tree: `7b44904e9b76a434a9e9532a3ab6075047a60cae`
- filtered history head: `480a22e3589b2a021466e2ba76bd8023b629163c`
- Device import commit: `c13bca48df2cc164d2fc396bb275f12e5e2d6799`
- import先: `firmware/legacy/xpotato-sim/`
- 取り込み時の15ファイルtreeは元treeと一致する。C++/header/PlatformIO configは現在もbyte一致する。
- 現行Protocol v2のsource/config、Rust host、identity/EEPROM、Accepted ADRは変更しない。
- 元sourceはSimの指定commitから、取り込み時の全内容はDevice import commitから復元できる。

git subtree splitはfirmwareだけを抽出し、Deviceはsquashなしのsubtree addでその履歴を保持する。
root pathが変わるためSHAは変わる。author/date/messageは全4件で保持され、他のSim source/historyは
Deviceへ取り込まない。

| Original Sim commit | Filtered commit |
|---|---|
| af775a116ab26b47ff77d606419de5e8777023db | 480a22e3589b2a021466e2ba76bd8023b629163c |
| afd41b440657656cceaf720e7110eee5cca093e7 | 03a4322af9cc7eaee1f15a32f1700ef2ab20c0f8 |
| 4a9c16639de98315dab42121f684c41fc39e9256 | be29dc94e2581b5bfca98ae017f44c421afc8c31 |
| 6e4e9f413035e24a7d3e279f7214d423cace3551 | d3ed68660ae417513e562930f596559f8c01c503 |

## Software compile

PlatformIO Core 6.1.19、Atmel AVR toolchain 7.3.0、framework-arduino-avr 5.3.0、
board `sparkfun_promicro16`（Flash capacity 28672 / SRAM 2560 bytes）で3ターゲットをcompileした。
firmware behavior・pin・protocolは変更していない。

| Target | Static SRAM bytes | Flash bytes | Compile |
|---|---:|---:|---|
| 現行v2 | 485 | 11172 | PASS |
| 旧Sim互換pro_micro_7ch | 646 | 9746 | PASS |
| 最初のlegacy reference | 773 | 11168 | PASS |

現行v2は既存budgetのstatic SRAM 1024 / Flash 24576 bytes以下。
旧Sim互換targetと現行v2にはcompiler warningなし。
最初のlegacy referenceには元code由来のsigned/unsigned比較8件とunused variable 1件のwarningがある。
挙動と元sourceを保つため、この移行では修正しない。旧referenceのString/動的allocationを含む
runtime SRAM余裕、noise、cadence、sensor配線、実機互換性はcompileだけでは保証しない。

CIへ旧2ターゲットのcompileを追加し、現行v2の既存memory-budget gateを保持する。
3ターゲットのbuildはupload targetを指定せず、serial/EEPROMへaccessしない。
実機書込、GUI、hardware/participant受入、採用・mergeは未実施である。

consumer側のproducer詳細はDeviceの旧producer契約へ移し、Simはv1受信互換とparser/healthを保持する。
採用順はDevice側の取り込み・検証後にSim側を採用する。Draft PR作成は採用完了ではない。
