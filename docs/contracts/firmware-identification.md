---
status: canonical
owner: firmware
last_verified: 2026-10-19
canonical_for:
  - firmware family distribution identity and reported version separation
---
# Firmwareの名前・版・通信方式

## 名称の境界

Selfrionette入力装置はロードセルとマイコンを備えるハードウェア。Selfrionette-Deviceはソフトウェアプロジェクトであり、Selfrionette Device Managerは設定・校正・更新を行うWindowsアプリである。

アプリ版、Firmware配布版、本体が返すFirmware版、通信Protocol番号を別項目にする。アプリ1.0.0とProtocol 3は異なる版体系である。

## 三系統の正本

名前・配布識別版・対応Protocol・ソース位置は [firmware/editions.json](../../firmware/editions.json) を正本とする。

| 配布名 | 位置付け | 本体の版照会 | 通信と機能 |
|---|---|---|---|
| Selfrionette Original 1.0.0 | 先輩由来の最初の取込みソースをそのまま保管・再ビルドする版 | なし | Legacy 7ch。版番号なし、プロジェクト上はv1相当 |
| Selfrionette Compatible 1.0.0 | 7ch通信を維持して整理・改善したユーザー側の互換版 | なし | Legacy 7ch。単文字の校正命令等はOriginalと同一ではない |
| Selfrionette Managed 1.0.0 | 装置管理・取得品質を備える新版 | あり。Firmware 1.0.0 | Protocol 2互換と明示開始のProtocol 3。個人補正はPC側 |

Original/Compatibleの1.0.0は今回付けた**配布識別版**であり、原作者が当時付けた版番号や実機が返した版ではない。凍結ソースには版照会を追加しない。先輩の実物に書き込まれている画像と同一かは、ソースの由来だけでは証明できない。

ManagedのFirmware版はPlatformIOがeditions.jsonからコンパイル定数へ注入する。GUI版との一致を要求せず、配布manifestの版と更新後に本体が返した版を照合する。Protocol番号はFirmware版から推測しない。

## 配布manifestと照合

manifest schema 3はfirmware_name、distribution_version、firmware_version、supported_protocolsを別に保持する。旧画像ではfirmware_versionを空、supported_protocolsを空とし、Protocol 0という実在の通信方式と解釈しない。

source commit、source tree SHA-256、image SHA-256、board、flash範囲、機能宣言を既存の検査で保持する。catalog schema 2は三系統を固定の位置で束縛する。旧catalog/schemaの読取りは明示的に互換範囲を検証する。

本体のvectorだけではOriginal、Compatible、Managedの互換streamを識別できない。画面は「未識別」と表示する。選択した書込み先の版を現在の実機版へコピーしない。書込み後に全flash照合と有限stream確認が成功した旧画像についても、本体ID/版/EEPROM保持は未確認のまま表示する。

## 安全な照会

正体不明の旧Firmwareへinfo/info3を自動送信しない。Originalでは先頭のiが校正を開始し得る。現行GUIは両凍結ソースが無視する?2/?3だけを使い、無応答から版を推測しない。

実機接続、書込み、EEPROM保持、センサの物理的精度は別の検証である。配布名やハッシュ一致は、それらの受入証拠ではない。
