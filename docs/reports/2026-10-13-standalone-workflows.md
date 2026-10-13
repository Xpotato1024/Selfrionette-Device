---
status: evidence
owner: apps/device-manager
date: 2026-10-13
related:
  - docs/architecture/device-manager-workflow.md
  - docs/decisions/0009-standalone-device-configuration.md
  - docs/contracts/device-persistent-settings.md
  - docs/contracts/firmware-update.md
---
# 既存PR統合・ブランチ整理と単独利用設計の初期実装

## 統合
本人の統合・ブランチ整理指示に基づき、PR #9と#10のhead・CI・実差分・reviewコメントを確認した。
PR #9: head 211cbf25a17280f1d328ec1d8eb04f40f78a6155、merge dce47ba780d26498b95fbeb65e6439f82eb8b5ce。
PR #10: head c9fd42b6ac1b7c8e9bd36a80ea7559edd32bac73、#9統合後にbaseをmainへ変更し、merge 6b7451f710f5f5ab851504f250ef45f15a48e1c4。
統合後mainのtreeが、検証済み#10 headのtreeと完全一致することを確認した。root checkoutはmainへfast-forwardした。

統合済みPRのheadとremote refが一致し、そのmerge commitがmainへ到達済みで、開いたPRが参照していないことを確認した次の9本だけを削除した。
bootstrap-device-repo / protocol-identity-design / firmware-v2-foundation / host-core-cli-foundation / serial-transport-foundation / identity-bound-monitor / sim-firmware-import-20261009 / calibration-gui-prep / device-workflow-redesign（すべてcodex/ prefix）。
remote refの期待SHAを照合するlease付き削除を使用した。作業対象だったlocal branch 2本も統合確認後に削除した。
旧device-workflow-redesign worktreeはcleanなdetached HEADで保持し、過去の合成store・スクリーンショットは削除していない。
新実装は統合後mainからcodex/device-standalone-workflowsへ分離した。mainへの直接commitはしていない。

## 設計から実装した範囲
装置内の極性保存・一度だけの補正、GUI常駐不要、RAM tare、operator設定とrobot mappingの境界、Firmware更新と設定保護のcontractを先に文書化した。
GUIは横の作業種別タブと縦の番号付き手順へ変更。中央に一手順だけを表示する。
達成状態と閲覧位置は分離し、戻って読む・タブ切替・確認取消はdomain stateを変えない。基準/応答の再実行は確認を通して後続を失効する。
対象/合成モードの重複表示、保存面の結果表、空の注意列、未比較7行、重複した再比較ボタンや入力エラーを削減した。
実Tauri試験で対象selectが一時無効化されてkeyboard focusを失う問題を確認し、対象切替後だけ必要なfocusを復元した。ArrowDown→ArrowUpの回帰試験で検証した。

装置設定はfirmware/shared/device_settings.hに、64 bytes×2のversioned journalを実装した。
ID領域を避け、known/negative mask、generation、CRC、commit marker、読戻し確認、stale要求/破損/同世代競合の拒否を備える。
設定の破損時に過去slotを黙って使用せず、未確認chを+1で埋めない。値が変わらなければwriteは0回。
符号付き24-bit ADC差分に対する極性適用関数を用意したが、これをv2のnegation後の値へ二重適用していない。

## ローカル検証
LLM-01 / Windows、既存Rust 1.94.1、Node.js 24.14.0、MSVC/WebView2を使用した。global toolchain/PATH/認証設定は変更していない。

| 検証 | 結果 |
|---|---|
| Rust workspace | 77件 PASS（core 36 / serial 27 / CLI 14） |
| GUI Rust use cases | 12件 PASS |
| frontend/controller/navigation/表示adapter | 9件 PASS |
| workspace/application fmt・Clippy all-targets | PASS、警告ゼロ |
| Tauri native Debug build（bundleなし） | PASS |
| 実Tauri/WebView2/Rust IPC | 9群 PASS |
| C++ setting journal（MSVC /W4 /WX） | 6群 PASS |
| journalのwrite中断→再起動模型 | 132境界ケース PASS |

native最終run: apps/device-manager/src-tauri/target/native-qa/run-cIP79Q/。
report.json、合成store、各手順・入力・履歴・範囲超過・エラー・320/640pxの画像を保持した。生成物はcommitしない。
最終の結果確認・PC保存・狭幅エラー画面を目視確認した。
3画面の320/640 CSS pxで全ページ横はみ出しなし。サンプルしたbody/p/th/入力エラーのコントラストは4.5:1以上。
単一panel、未到達step拒否、back/tab/cancelによる無変更、明示再実行の失効、keyboard focus、左右状態保持、保存/閲覧/再起動、旧tokenと再送拒否、7品質不良、逆方向未測定、unclamped値を検証した。

C++試験はbinary golden、64 byte各位置の破損、ID/mask/partial/CRC/未知形式、世代競合・上限、same-value no-write、stale generation、任意byte update中断、readback失敗、commit後応答喪失、identity領域不変、24-bit差分端値を扱う。
byte storeは模型であり、実EEPROMのbrownout・書換え耐久・Fuse/BOD・実配線・物理極性を検証したものではない。

## CIと実機の区別
Firmware CIにGNU C++でのjournal実行試験とATmega32U4向けcompile-onlyを追加した。
既存v2 Firmware buildとstatic SRAM/Flash gate、legacy reference buildを維持する。新moduleは現行main.cppへ未結線なので、既存imageへ新しい永続処理を注入していない。
ローカルPASSはGitHub CI完了を意味しない。新headのCI結果はPR checksで確認する。

## 未実装・未実施
- Firmwareのstaging/readback/commit command、EEPROM実adapter、設定適用と取得品質を表す新出力contract/consumer接続。
- Firmware update package検査、更新実行backend、実機用保守画面、bootloader再列挙・設定migration/復旧。
- GUIなし・別PC・再起動を含む実機受入、model/配線/感度軸・荷重経路の照合。
- スクリーンリーダー、native 200%/Windows DPI、Release/bundle、利用者による操作性評価。

実機serial open、reset/tare送信、flash、EEPROM変更、robot操作は実行していない。ハードウェアを接続しないcode/testの進捗と完成品を区別する。
