---
status: canonical
owner: firmware
last_verified: 2026-10-19
canonical_for:
  - quality stream rate comparison and physical acceptance
  - 20/40/80 Hz comparison image selection and rollback
related:
  - docs/contracts/firmware-protocol-v3.md
  - docs/contracts/firmware-identification.md
  - docs/operations/hardware-safety.md
  - docs/operations/field-test-30-60min.md
---

# 品質付き20/40/80 Hzの比較と実機受入

Issue #24のsoftware実装と、#26の実機受入を区別する。最大80 Hzの送信試行がcompile・native testで成立しても、研究室の装置で新しいADC変換80回/sや全7ch usable入力80回/sが得られる証拠にはならない。

## 1. 実機操作前に用意する記録

対象ごとに以下を揃え、serial open・書込み・tare・EEPROM変更それぞれの明示許可を得る。

| 項目 | 記録する内容 |
|---|---|
| 物理装置 | 左右役割、装置ラベル、stable device ID、接続port、配線写真の非機密参照 |
| 基板・ADC | MCU/clock、実チップ型番、全7chのRATE端子の電気的接続と確認方法 |
| 現在の画像 | Original/Compatible/Managedの別、本体版・Protocol・source SHA、取得不能ならunknown |
| 保全 | 現在の正式imageとhash、ID、settings generation/assembly/known/negative、PC側profileの控え |
| 候補画像 | source SHA、build environment、本体版、HEX SHA-256、pinned toolchain、RAM/Flash結果 |
| 操作者・条件 | 無荷重、各指の配置、単ch/複合/持続入力、同じPC・USB topology・同時実行software |
| 停止・復旧 | consumer停止、対象接続の停止、個体を再識別して既知imageへ戻す手順、許可済み操作の範囲 |

port順だけで左右を決めない。旧`vector`だけから現行Firmware版、ADC生値、fresh品質を補完しない。RATE配線が不明なら不明のまま記録する。

## 2. software-onlyで準備できるもの

環境と本体版の対応は[Firmware README](../../firmware/loadcell_7ch/README.md)を正とする。既定80 Hzと40/20 Hz比較buildは同じsource・同じpinned toolchainから作り、実行中のrate切替は行わない。

1. clean source revisionを`SELFRIONETTE_SOURCE_SHA`へ指定する。
2. `pro_micro_7ch` / `pro_micro_7ch_40hz` / `pro_micro_7ch_20hz`を個別にcompileする。
3. `tools/check_firmware_size.py`で各画像のstatic SRAM ≤1024 B / Flash ≤24576 Bを確認する。
4. environment、注入したFirmware版、source SHA、HEX SHA-256を同じ記録へ残す。比較画像を通常Managed packageのmanifestに差し替えない。
5. 比較画像の本体版suffixとsource bindingが既存profileに適合するかをPC側で再検証する。画像を変えただけで古いprofileをreadyにしない。

これらにupload操作は含めない。公開済み1.0.0のタグ・Assetsと、前段の40 Hz Draft PR #23は変更しない。

## 3. 許可後に行う比較

まず現在の画像で書込みなしの観測を行う。候補の書込みが許可された場合だけ、ID/設定の保全、bootloaderの対象再識別、全flash照合、再接続後の版/source/ID/設定読戻しを順に行う。

同一条件で正式20 Hz版と候補20/40/80 Hzを比較する。候補20 Hzは旧1.0.0と同じ画像ではなく、同一候補実装に対する速度比較用である。候補40 Hzも前段PR #23と区別する。

各レートで無荷重、単ch入力、複合入力、持続入力、tare中、停止/再開、USB切断/再接続を観測する。通常条件が成立した候補については既存の[30分必須・60分上限の観測計画](field-test-30-60min.md)に従って長時間確認する。GUI表示更新fpsをSerialの受信fpsと混同しない。

| 指標 | 記録・解釈 |
|---|---|
| 送信/受信frame/s | `sample3`の件数と観測時間。queue skipはsequence欠番に現れないので、欠番ゼロを80 Hz達成としない |
| ADC変換/s | 各chのready/RATE・読出しを独立に確認。数値の変化回数をADC変換回数としない |
| 各ch fresh/s | `flags == fresh`の観測数。フレーム間に複数回取得したADC変換をすべて数えた値ではない |
| 全7ch同時usable/s | 全ch fresh単独、orientedあり、対応settings/tare/sessionが有効なframeの割合と件数 |
| 混合品質 | chごとにheld/timeout/saturated/rejected/未取得を分け、全ch usableを偽装しない |
| 間隔 | hostの単調時計による受信間隔とdevice_msの増分、p50/p95/p99・最大値 |
| 古さ | host側の計時基準・connection epochを明記。device_msとPC時計の直接差を遅延としない |
| 欠落・復旧 | sequence欠番/逆行/停止、malformed line、USB停止、停止から再開までの時間、profileの失効 |
| CPU/USB負荷 | firmware取得・文字列生成・loopの計時、USB滞留、PC受信CPUとUI処理を別に記録。native fixture時間をAVR実行時間へ換算しない |

USB Serialの設定値115200をUSB CDCの物理UART上限として計算しない。最大219 bytesのframe文字量は20/40/80 Hzで4380/8760/17520 bytes/sだが、USB/CPU/ADCを含む実効値は観測が必要である。

CPU・ADCの外部計時に測定用ピンや配線変更が必要なら、その対象と方法も先に確定する。通常画像へ無断で計測用GPIOを追加しない。計測画像は通常候補と別にsource/image hashを保存する。

## 4. 退避・出荷判定

80 Hzでfresh取得が不足する、tail latencyやUSB滞留が悪化する、操作の改善を確認できない場合は、原因をADCのRATE/取得、firmware CPU/USB、PC処理に分ける。品質を緩めず、実測で安定した40 Hzまたは20 Hz比較画像を退避候補にする。

退避は明示的な画像選択・書込みを伴う。送信詰まりを理由にfirmwareが黙ってrateを変更する機構はない。どの画像でもbackpressure時はbounded skipを行う。

復旧時はconsumerを停止し、対象をstable IDと物理ラベルで再確認して、許可された既知imageへ戻す。書込み/読戻しの失敗で別portへ自動retryしない。ID/settings、source/version、tare/profileの適合を再確認し、確認不能なら入力を停止した状態にする。

約−65536 count impulseは原因未確定のままである。再現条件・raw/品質・配線を記録し、単にframe rateが上がったことを修復証拠にしない。80 Hz実効入力の成立条件と制約を実測で説明できるまで、#24/#26を実機受入済みとしてcloseしたり、候補を安定版として公開したりしない。
