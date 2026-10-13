---
status: historical
owner: apps/device-manager
date: 2026-10-14
---
# 通常UI・native menu・COM候補の実装と検証

## 対象
基点: PR #13 / 6eae0e591b02d7999510706580d826f95d486996。
作業branch: codex/device-native-menus-discovery。アプリ: 0.1.0-preview.5。
設計正本: ../contracts/device-manager-session.md。mainと既存PRのheadは変更しない。

## 実装
通常起動から合成試験の選択UIを除き、Tauri native menuからの明示有効化とCLIの起動引数を用意した。有効化と入力切替を分け、Rustの単一runtimeで合成IPC・hardware IPC・モード変更を排他管理する。Debug/環境変数では有効にならない。
COM候補はUSB種別・serial・VID/PIDを保持し、出典付きの基板候補/その他/情報不足/bootloaderへ分類する。接続dialogは選択時のdescriptorを保持し、再列挙後の一致を確認して一台だけ開く。infoの送信は確認可能なチェック項目で選択する。旧Firmwareの無送信受信経路は残す。
バックグラウンドreadは操作buttonを無効化しない。利用者操作は一つの待機欄へ保持し、接続sessionが変化すれば拒否する。切断は進行中readの直後へ送る。
ch行選択と履歴を連動させ、自動/固定軸、停止した履歴の保持、欠測区間の分断、新接続の履歴消去を追加した。現在値の500ms失効と固定セルは維持する。

## ローカル検証
- Rust workspace tests: PASS。serial/common parser、既存CLI境界を維持。
- アプリuse-case/Rust: 34件PASS。新しいmode/descriptor/bootloader/optional info/reconnectの試験を含む。
- workspace/app Clippy: 警告なしPASS。format PASS。
- 高速frontend: 単体23件、配布/CI条件25件、表示9組と候補選択・接続待機操作の模擬試験。最終コマンドは14.26秒、RustビルドとNSIS生成は0回。fast-report.jsonに記録。
- Windows Debug buildおよび実Tauri/Rust IPC: 10群PASS（run-Pq7PgJ）。Win32 GetMenuで本物のmenuを検査し、所有windowへのmenu commandでGUIを進めた。CLIの3起動形態、無効引数、再起動reset、通常モードでの合成IPC拒否も検査した。
- native-menu.ps1は所有processのwindowだけを取得・撮影する。desktop全体や他アプリは操作しない。
- 動的画面: 800/1100/1600論理幅 × deviceScaleFactor 1/1.5/2。数値DOM・小数点・失効時の矩形維持、模擬COM3/8/9、descriptor差し替え、切断待機、developer確認取消を検査。
- 試験専用変更は製品fingerprintに含めず、既存artifactでnative試験を再実行できる。一方app/hardwareのIPC結線JS変更ではCIのnative境界も選択するよう補強した。

## 境界
実機のportを開く、Firmware upload、EEPROM、tareは実行していない。COM3/8/9の実物descriptorは未採取であり、同番号を使った模擬試験を実機自動判定成功とは扱わない。
USB descriptorが同一でUSB serialもない同型機の差し替えは列挙だけで区別できない。候補判定は認証・物理校正・左右割当ではない。個体の恒久記憶、二台同時監視は未実装。
Windows実DPI/文字サイズ/スクリーンリーダーは未受入。署名は今回対象外、NotSignedを維持。CI/最終配布結果は対応PRへ記録する。
