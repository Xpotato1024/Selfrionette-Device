# Selfrionette Device Manager v0.2.0-preview.1

**Windows 11 x64向けプレビュー版**です。7チャンネル機器の実設定・個人校正・Firmware更新を行うための候補です。

- [Windows用セットアップインストーラー]({{SETUP_URL}})
- [Portable ZIP]({{PORTABLE_URL}})
- [同じ版のFirmware package]({{FIRMWARE_URL}})
- [SHA-256チェックサム]({{CHECKSUM_URL}})
- [アプリケーション画面]({{SCREENSHOT_URL}})
- ソースコミット: [`{{SOURCE_SHA}}`](https://github.com/Xpotato1024/Selfrionette-Device/commit/{{SOURCE_SHA}})

## 今回の機能

- **7ch実機セットアップ:** 各センサーの取付位置・向き・極性を、実際のチャンネル応答を確認しながら設定し、対象機器へ保存して読み戻せます。設定は装置IDに結び付き、COM番号や接続順から個体を推定しません。
- **明示tareと個体確認:** 対象装置を選んでからtareを要求し、装置ID・設定世代・完了状態を照合します。再接続後に古い接続やtareの状態を引き継ぎません。
- **ライブ個人校正:** 操作者が自然な姿勢・握りで中立位置を取得し、指示に沿った方向別入力を記録します。共通のRust fitterで候補A/Bを作り、fitに使っていない方向・同時入力・除荷で比較してから、利用者が採用・保存・今回への適用を明示します。保存profileを再利用する場合も、接続やtareの後には新しい中立位置が必要です。
- **GUIと共有する入力runtime / CLI:** standaloneの`distribution/selfrionettectl.exe`をインストーラーとZIPの両方に同梱します。保存profileを使った入力取得でも、対象identity、tare、品質、fresh neutralを確認します。
- **同梱Firmware更新:** SparkFun Pro Micro 16 MHz / ATmega32U4向けの検証済みmanifestとIntel HEX、ハッシュ固定したAVRDUDE v8.3 Windows runtime、ライセンス情報を含みます。Applyを明示した後にbootloaderを再列挙して対象を確認し、再接続後に装置ID・設定・Firmware情報を読み戻します。対象が曖昧、または読戻しに失敗した場合は`needs_recovery`で停止します。

## 検証範囲と制限

本候補では実機を使ったセンサー取付確認、tare、個人校正、EEPROM設定保存、Firmware書込み、再接続後の読戻しを実施していません。ソフトウェア試験や配布物のハッシュ検査は、実機での受入試験を代替しません。実機適合性とハードウェア受入れは未確認です。

インストーラーは初回導入・更新向け、ZIP版は展開して使う形式です。両方にstandalone CLIが含まれます。アプリは未署名のため、Windows SmartScreenや組織の実行ポリシーで警告されることがあります。

## 配布・公開条件

タグに対応するソースから候補を一度buildし、別のWindows runnerで同じartifactの入力fingerprint、チェックサム、第三者ライセンス、インストール・起動・更新・削除を検証します。これらの配布ゲートがすべて成功した場合に限りPre-releaseを公開します。未実行・保留・失敗のゲートは成功として扱いません。このテンプレートのURLとsource SHAは、その候補の検証後に置き換えます。

`0.1.0-preview.5`は[source `3435764`の公開済みプレビュー版](https://github.com/Xpotato1024/Selfrionette-Device/releases/tag/device-manager-v0.1.0-preview.5)です。既存の検証済み8 assetsを公開し、旧artifactは変更していません。本書の`0.2.0-preview.1`とは別の配布版です。
