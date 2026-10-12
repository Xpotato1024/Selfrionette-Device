---
status: draft
owner: architecture
last_verified: 2026-10-12
related:
  - docs/contracts/device-host-boundary.md
  - docs/contracts/sample-monitor.md
  - docs/operations/calibration-observation.md
---
# 向き補正と操作範囲のpreview

## 現在の範囲
純粋なRust coreとoffline fixture GUIの契約案。実機のphysical calibration成功契約はまだ確定していない。
schemaが妥当であること、previewを確認したこと、実機でverifiedであること、control readyであることは別状態。
今回のdraftは常にhardware_verified=false、control_ready=false。実機のID・番号・極性・値は未設定。

## 元値と補正
current firmwareのvectorは全ch negation、RAM tare、spike抑制後のfirmware sampleである。
raw ADC、N単位のforceとは呼ばない。timeoutでは前値を保持し得る。boot autotareとcalibration_endは成功保証ではない。
host session baselineは既存firmware sampleから作る相対基準であり、firmwareのtare offsetをもう一度引かない。

正方向のpreviewは次の式とする。
normalized = polarity * (firmware_sample - session_baseline) / positive_span
positive_spanは有限の正数。元の7ch値と番号を保持し、差分・向き補正後・無次元値・表示値を分ける。
表示を[-1,1]へclampする場合も、clamp前の値とsaturationを独立して表示する。
逆方向を測定していない場合はreverse span=None、負側normalized=Noneとする。対称性を捏造しない。
FirmwareSampleとPreviewを別型にし、Previewを補正入力に再投入できないAPIにする。

## 分けて持つデータ
physical draft: stable ID、firmware sample由来、組立revision、7ch ±1、revision、evidence、override origin。
operation draft: 上記profile参照、ユーザー、姿勢、session epoch、正方向span、測定した場合だけ逆方向span。
hostのleft/right roleはIDと別binding。COM順やIDの文字列からroleを決めない。
operation spanはN単位の物理gainでもSimのrobot gainでもない。stable IDだけで恒久再利用しない。
Device hostが所有する無次元入力previewまでを今回の範囲とし、robot sign/gain/mappingはXpotato-Simの責務を保つ。

## 判定と失敗
合成fixtureでは各phase 3〜64 samples、2〜8 repeatsの有限観測を使用する。
実機のnoise/stability/信号対雑音比/crosstalk閾値は未確定。fixtureの数値を推奨実測閾値にしない。
既知方向が欠ける、基準が不安定、signalがnoise付近、繰返し符号が一致しない、除荷で戻らない、
crosstalkが不明/結合応答がある、reset/spike/timeout/clipping/品質不明はinconclusiveとする。
全chの応答を記録し、7chを7本の独立した指とは仮定しない。未知の軸や荷重経路を推測matrixで補わない。
manual overrideは品質を満たす候補にだけ適用し、理由を記録する。不良品質や失敗を上書きして通さない。
符号・基準・span・previewの変更で古い確認は失効する。通常入力からsign/spanを自動更新しない。
tare/reset/reconnect/epoch変化ではbaselineとreadinessを失効させる。再ロードだけでは復元しない。
未知profile、source/context不一致、非有限値、欠落ch、重複ID/実番号を拒否する。

## GUIと後続境界
Tauri GUIはRust coreを直接利用し、CLI subprocessをbusiness APIにしない。
現在は合成fixture専用で、serial open、info、tare、flash、EEPROM、robot接続を実装しない。
ブラウザー表示確認は固定のpresentation fixtureであり、Rust判定・draft保存を行わない。
current 1〜64 sample monitorは通信smoketest用であり、校正シーケンスを所有しない。

原本rawが必要な場合、negation/tare/filter前の別bounded診断経路を将来設計する。
ch mask、有限capture数/時間、raw integer、boot/tare epoch、timeout/spike/clippingの状態を明示し、
normal streamとは別frame型にする。現在のfirmwareには追加せず、書込みも行わない。

## 合成recordの保存契約（実機未対応）
個体別storeはcanonical stable ID配下、20桁の増加連番file、新規作成だけを使用する。
1 ID 32件、1 record 32768 bytes。symlink、非canonical名、破損、未知schemaを拒否する。
recordのhardware_verified/control_readyはfalseを必須とし、synthetic firmware/fixture assemblyだけを受け付ける。
確認時のtimestamp、元7ch、履歴baseline、差分、補正後値、正規化値を別々に保存する。
firmware timestampはu32範囲。元値-baseline=差分、符号×差分=補正後、spanによる正規化の整合性を検証する。
履歴baselineは証拠の比較専用。active baseline/符号/span/readinessの復元とは区別する。
再読込でID/firmware/組立/ユーザー/姿勢を照合し、新epochの空sessionを作る。失敗でも旧確認を失効させる。
次回の基準と品質観測から候補を作り直す。通常入力や保存済み値だけで校正済みにしない。

## 再判定と失敗後の状態
再判定要求の入口で旧preview承認を失効させ、不正引数でもsave/exportへ持ち越さない。
assess失敗後はqualified stageを失い、手動flipで旧候補へ復帰できない。新しい品質観測が必要になる。
GUIはエラー後にRustの現在状態を読み取り、旧active rowsや確認ボタンを成功状態のまま残さない。
この失効は基準・符号・spanの自動再校正を意味しない。coreのrevoke_previewは確認状態だけを消す。
