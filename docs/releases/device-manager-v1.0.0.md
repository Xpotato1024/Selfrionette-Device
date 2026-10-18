# Selfrionette Device Manager 1.0.0

Selfrionette入力装置の設定・個人校正・診断・Firmware更新を行うWindows 11 x64用アプリです。初回の正式ソフトウェアリリースです。

## ダウンロード

- [Windowsインストーラー]({{SETUP_URL}})
- [インストール不要のZIP版]({{PORTABLE_URL}})
- [3系統のFirmware・対応ソース・manifest]({{FIRMWARE_URL}})
- [SHA-256一覧]({{CHECKSUM_URL}})

使用者にRust、Node.js、Python、Git、ビルド環境は不要です。ZIPは全体を展開してください。

## 主な変更

Firmware書込み中は進捗バー・対象・工程を表示します。対象確認、待機モード切替、書込み、全域照合、再接続確認を分け、割合が不明な工程は不定表示にします。書込み工程100%を更新成功とは扱いません。更新中の再書込み・条件変更・通常のウィンドウ終了を防ぎます。

アプリの版、Firmwareの名前/配布版、本体が返したFirmware版、通信Protocolを分けて表示します。

| 配布名 | 配布版 | 本体Firmware版と通信 |
|---|---|---|
| Selfrionette Original | 1.0.0 | 初期保管ソース。本体版の照会なし、Legacy 7ch |
| Selfrionette Compatible | 1.0.0 | 互換改善ソース。本体版の照会なし、Legacy 7ch |
| Selfrionette Managed | 1.0.0 | Firmware 1.0.0、Protocol 2互換/Protocol 3 |

Original/Compatibleの配布版は今回付けた識別子です。凍結ソースを変えて後付けの版を本体の応答として偽装していません。先輩の実物の画像と同一かは未確認です。

## 初回の利用・共有

まず [初回利用・研究室内共有ガイド](https://github.com/Xpotato1024/Selfrionette-Device/blob/device-manager-v1.0.0/docs/operations/first-use-and-sharing.md) を確認してください。旧版の受信確認と、新しい個人補正のためのManaged導入を区別しています。

個人profileはPCに保存され、GUIを終了した後も対応する共通Rust機能/CLIから利用できます。Firmware更新や装置・操作者条件が変わった場合は互換性確認と再検証が必要です。更新で個人データを削除しないでください。

## 検証範囲と制限

配布物は同じsourceから作成し、ソフトウェアの回帰、Firmwareビルド、hash/manifest、Windows配布内容、起動・GUI/IPC、更新経路のsoftware fixtureを検証します。詳細は添付のBUILD-INFO.json、VALIDATION.json、UPGRADE-VALIDATION.jsonを参照してください。

これらは現物への新たな書込み、EEPROM電源断試験、既知荷重によるN精度、実操作者の操作性能、実ロボット制御の認証ではありません。Managedの品質付きProtocol 3は現在最大20 Hzです。過去に観測された約−65536 countのimpulseの原因・修復は、このUI/配布変更では証明していません。旧7chの受信のみでは本体版やセンサ品質を確定できません。

アプリは未署名です。発行元名の表示はWindowsの検証済み署名ではありません。セキュリティ機能を無効化しないでください。指定基板はPro Micro互換ATmega32U4・16 MHzに限定します。

以下の画面は合成transportによるソフトウェア検証であり、実機校正の証拠ではありません。

![個人設定を用いた入力表示のソフトウェア検証]({{SCREENSHOT_URL}})

Source commit: {{SOURCE_SHA}}
