# Selfrionette-Device

Selfrionette入力装置のFirmwareと、PC側の設定・個人校正・診断ソフトウェアを管理するプロジェクトです。物理的なロードセル入力装置の名前と、Windowsアプリ **Selfrionette Device Manager** を区別します。

## 利用する

**[公式リリースとWindows用ダウンロード](https://github.com/Xpotato1024/Selfrionette-Device/releases)**

Windows 11 x64向けのsetup.exe、portable ZIP、3系統のFirmwareパッケージを配布します。本ソースは正式版1.0.0用で、公開済みの版とAssetsはRelease一覧を正とします。利用にRust、Node.js、Pythonやソースのcloneは不要です。

最初に **[初回利用・研究室内共有ガイド](docs/operations/first-use-and-sharing.md)** を確認してください。[保存先・更新・配布物の説明](docs/operations/device-manager-distribution.md)も用意しています。

## Firmwareを区別する

| 配布名・配布版 | 目的 | 通信 |
|---|---|---|
| Selfrionette Original 1.0.0 | 先輩由来の最初の取込みソースを変更せず保管・再ビルド | Legacy 7ch。本体版の照会なし |
| Selfrionette Compatible 1.0.0 | 7ch通信を維持して整理した互換改善版 | Legacy 7ch。本体版の照会なし |
| Selfrionette Managed 1.0.0 | 装置管理・品質付き入力・PC上の個人補正に使う新版 | Protocol 2互換 / Protocol 3 |

Original/Compatibleの1.0.0は今回付けた配布識別版であり、原作者の版番号や実機が返す版ではありません。アプリ版・Firmware配布版・本体Firmware版・通信Protocolは[識別契約](docs/contracts/firmware-identification.md)の別項目です。

## 設定と通常利用

Device Managerで個体設定、無荷重tare、個人校正を行います。個人profileはPCに保存し、通常操作はGUIを終了して共通Rust機能/同梱CLIから補正入力を利用できます。[Managerなしの利用手順](docs/operations/normalized-input-consumer.md)。Xpotato-Simへの統合は別プロジェクトの作業で、このアプリにロボット制御は含みません。

Firmware書込みには実際の工程と進捗を表示します。書込み100%の後も全域照合・再接続確認が必要です。未確認の版・個体ID・設定保持は不明のまま表示し、検証済みと推測しません。

## 検証範囲

正式なソフトウェア配布と、装置の物理的精度・EEPROM電源断耐性・実ロボット操作の認証は別です。Releaseの検証記録・既知の制限を確認してください。指定基板はPro Micro互換ATmega32U4 16 MHz。アプリは未署名で、発行元表示はコード署名ではありません。

## 開発する

- Firmware: firmware/、PC共通Rust core/CLI: crates/、Windows GUI: apps/device-manager/。
- [開発者向けGUI手順](apps/device-manager/README.md)、[仕様索引](docs/README.md)、[検証手順](docs/operations/validation.md)。
- 実装前に [AGENTS.md](AGENTS.md) を確認してください。
- 個体identity/校正/品質は本プロジェクト、ロボット運動への写像・物理計算はXpotato-Simの責務です。
