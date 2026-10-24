---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-17
canonical_for:
  - Windows binary usage and distribution
related:
  - apps/device-manager/README.md
---
# Device Managerをビルドせずに使う

## 対象と配布物
Windows 11 x64向けのDevice Managerです。正式版1.0.0のソフトウェア配布を対象とします。品質付き入力、装置の実配線・取付方向設定、操作者ごとの個人校正とprofile保存、Selfrionette firmwareの明示的な更新操作を提供します。物理デバイスと実操作者による受入は未実施です。
Rust、Node.js、Visual Studio、Windows SDK、Git、ソースのcloneは使用者に不要です。
ソースの開発・ローカルブランチの検証は[開発者向けREADME](https://github.com/Xpotato1024/Selfrionette-Device/blob/main/apps/device-manager/README.md)に従ってください。

| 配布物 | 使い方 | 実行時の前提 |
|---|---|---|
| 名前末尾が -setup.exe | 実行して案内に従い、スタートメニューから起動 | WebView2未導入時のオフラインインストーラーを同梱 |
| 名前末尾が -portable.zip | 全体を展開し、selfrionette-device-manager.exeを実行 | Microsoft Edge WebView2 Runtimeが導入済みであること |
| 名前末尾が -firmware.zip | manifest・HEX・ライセンスを含む同じアプリに束縛された3系統のFirmware package | 通常の書込みはアプリの更新画面で明示確認する |

通常利用者にはsetup.exeを推奨します。ZIP内から直接実行せず、必ず展開してください。
ZIP版はインストール不要ですが、設定・記録をUSB等の配布フォルダー内だけに保存する意味のportableではありません。
VCランタイムは静的リンクし、画面のHTML/CSS/JavaScriptはexe内に含めます。開発サーバーやビルドツールは起動しません。
両配布物には`distribution/selfrionettectl.exe`も含まれます。これはWindows x64向けのスタンドアロンCLIで、実行時にRust、Node.js、Python、PlatformIOは不要です。シリアル操作やfirmware更新の通常手順にはDevice ManagerのGUIを使用してください。
Windows ARM64/x86、Windows 10、未導入WebView2環境への実インストールは別途受入対象です。

## 取得と起動
**[GitHub Releases（Windows用バイナリのダウンロード）](https://github.com/Xpotato1024/Selfrionette-Device/releases)**

公開済みの正式アプリは1.0.0です。本ソースには次期Managed Firmwareの開発候補も含みます。[Release一覧](https://github.com/Xpotato1024/Selfrionette-Device/releases)の公開済み版とAssetsを正としてください。正式1.0.0と過去のpreview公開物は上書きしません。Source code.zipは実行バイナリではありません。

正式タグdevice-manager-v1.0.0は通常Release、ハイフン付きpreviewタグはPre-releaseとして扱います。

setup.exeは現在の利用者向けにインストールします。組織の実行・インストール制限を回避するものではありません。
WebView2は実行時依存です。ZIP版で起動しない場合は同じ版のsetup.exeかMicrosoft公式のWebView2 Runtimeを利用してください。
WebView2同梱インストーラーは初回のRuntime導入にネット接続を要求しない設定です。Runtimeの将来の自動更新にはネット接続が必要です。

通常起動は実機用画面です。「接続先を選ぶ」で候補を選び、必要に応じて「その他のポートも表示」を使います。接続は明示操作した一台だけです。接続直前に選択時のUSB情報を再照合します。候補は対応基板という意味で、Selfrionette個体や校正済みの認定ではありません。
値が届くまで「受信待ち」、更新が止まれば旧値を消します。OS一覧にない場合と、ポートはあるが接続/受信できない場合を分けて確認してください。機器時刻後退や不正データでは停止し、自動で別個体へ接続しません。
「診断・記録」の個体情報取得はProtocol v2のinfoだけを送信します。旧Firmwareの7ch入力も受信できますが、個体IDは応答しない場合があります。
GUI操作の合成試験は「ヘルプ → 開発者向け機能を有効にする」で確認した後、「開発 → 合成試験」で開きます。有効化だけでは入力を切り替えません。実機接続中は先に切断してください。開発者モードは再起動後に引き継ぎません。
合成モードの成功は実機の使用可能性を意味しません。

## 一時停止と再接続

「一時停止（接続を解放）」でポートを他ツールへ渡せます。「再開・再接続」は前回の候補を再列挙し、USB情報と既知の個体IDを照合します。ID不明なら同じ実物の再接続確認が必要です。COM番号やUSB serialの一致だけでは個体を認定しません。最後の明示候補は`connection-preferences.json`に保存しますが、アプリ起動時には自動接続しません。missing候補も再接続待ちとして残します。

今回の中立値・ready・中断したwizardは再接続で復元しません。保存profileとログは残します。旧版の観測・debugは校正不要で、新版の補正入力だけが装置設定と個人校正を必要とします。

## 認識しない場合
この版より前の0.1.0-preview.1は合成専用で、USBを接続しても一覧は変わりません。
この版で一覧にない場合は、Windowsのデバイスマネージャーのポート表示とUSB接続を確認します。ポートはあるが接続できない場合は、シリアルモニターやSimなどによる占有を確認します。受信待ちの場合は通信形式/Firmwareを確認します。ドライバー不良やFirmware書換えが必要とは、GUIの未表示だけで決めつけません。

## 保存先と更新
個人profile・個体の対応設定と取得根拠は原則として `%APPDATA%\dev.selfrionette.manager\personal-profiles\<device-id>\`、更新前の保護記録と要復旧journalは同じapp設定directory内の`updater\`に保存します。合成試験だけは`synthetic-fixtures\`です。exeのある場所は書込み先にしません。
開発者・試験者がSELFRIONETTE_GUI_FIXTURE_DIRを設定した場合だけ保存先を変更します。通常使用では環境変数設定は不要です。
更新はアプリを終了してから行います。ZIP版は旧版へ上書きせず新しいフォルダーへ展開すると比較できます。
記録の互換性は別契約です。版が異なる記録を読み込めると無条件に保証しません。

## 装置設定・個人校正・保存profile

「調整」の「装置と取付方向」で明示接続し、未登録の場合だけ個体IDを新規登録します。全7chのrawを見ながら実際の部位・論理軸・既知の正方向を確認し、候補極性を別試行で確認して装置へ保存・読戻しします。無荷重tareは個人neutralとは別の明示操作です。

「個人校正」で操作者・姿勢・握りを設定し、画面の指示に従って準備できた手順だけを開始します。痛みのない範囲の正逆反復と、fitに使わない弱い入力・同時入力・除荷を記録し、候補A/Bを比較して採用methodを検証します。できない方向をskipすると欠測を保持します。「保存と現在入力」で確認後にprofileを保存し、今回の個人neutralを取り直して明示適用します。再接続・tare・装置設定や使用条件の変更後にreadyやneutralを自動復元しません。

通常操作にJSON編集やCLIは不要です。Managerを使わないconsumerは同梱CLIまたは共通Rust serviceを利用できます。[正規化入力consumer手順](normalized-input-consumer.md)を参照してください。Xpotato-Simへのadapter・robot制御は同梱しません。

## Firmwareの更新

書込みでは進捗バーと工程を表示します。書込み工程100%の後にも全域照合と再接続確認があり、完了表示までUSBを抜かないでください。通常のアプリ終了は更新中に拒否します。異常時は要復旧の結果と保護記録を残します。

入力確認の選択を更新画面へ引き継ぎます。「受信を停止して更新へ」で接続を解放し、Original（初期保管）／Compatible（互換改善）／Managed（管理対応）を選びます。Firmware名・配布版・本体が返す版・通信Protocolは別に表示します。[初回利用・共有ガイド](first-use-and-sharing.md)を参照してください。

OriginalとCompatibleは元ソースを凍結し、別wrapperで配布します。個体ID・本体版の照会・新しい個人校正には対応しません。Managedへの帰還時だけ既知のID・設定保持を照合します。Originalと先輩の実機画像が同一かは未確認です。

`0.2.0-preview.1`以降の対応版は、同梱された検証済みIntel HEX packageを使って`SparkFun Pro Micro 16 MHz / ATmega32U4`だけを更新します。対象とpackageの確認、Applyの明示操作、再接続後のidentity/settings/version read-backを別々に行います。Applyの前に表示された候補やCOM番号だけでは書き込みを開始しません。USB location pathやserial descriptorでbootloaderを結び付けられない場合は、一候補でも追加のbootloader確認が必要です。重複・曖昧な候補、途中失敗、read-back不一致は`needs_recovery`として停止し、自動で再書き込みしません。

更新時は設定とidentityのpre-update backupを保存します。同梱packageはmanifest、image、source revisionの各hashでこのアプリ版に結び付きます。ユーザーが選んだpackageは内部checksumが一致しても未検証packageとして表示します。初回install等で更新前のidentity/settingsを読めない場合、firmware version/sourceと更新後の状態を確認し、更新前との保持比較ができなかったことを明示します。詳しいpackage・対象・recovery contractは[firmware update contract](../contracts/firmware-update.md)を参照してください。

このsoftware実装の公開時点で、実機のreset、bootloader再列挙、firmware書込み、更新後read-backによるhardware acceptanceは未実施です。物理デバイスの書き込みを必要とする運用では、別途受入記録が揃うまで未検証として扱ってください。

## 旧版からの更新とアンインストール
0.1.0-preview.1からpreview.2への更新では、発行元変更に伴う旧保存先の参照不一致により、インストール前のアンインストーラーが「Error launching installer」で停止し得ます。
preview.3以降は表示用の発行元とインストーラー内部の識別を分離し、同じ製品のInstallLocationとUninstallStringを照合してから旧版を呼び出します。空・相対・不一致の保存先で削除を続行しません。
旧版のアプリを終了し、新しいsetup.exeで更新します。登録先が壊れている場合は更新を中止し、ZIP版を別フォルダーで利用してください。レジストリやAppDataを手動削除したり、CRC検査やセキュリティ機能を無効化したりする必要はありません。
更新時は「アプリデータを削除」を選択しないでください。記録のschema互換性は、ファイルが保持されることとは別です。

## 製品情報と発行元
表示名はXpotato.net、Webサイトはhttps://xpotato.net。正本はtauri.conf.jsonのbundle.publisher/homepageで、exeのCompanyName、NSISのPublisher、アプリ情報dialogへ反映する。
製品名とバージョンは通常の作業面へ常設せず、Windowsタイトルバーと右上の「アプリ情報」で確認する。
アプリ情報は読取り専用であり、表示によって対象個体・調整・確認・記録を変更しない。
製品のidentifierはdev.selfrionette.managerを維持し、発行元表示の変更で既存記録の保存先を変えない。
名称の設定だけでは、UAC/SmartScreenの「確認済みの発行元」やデジタル署名は設定できない。信頼されたコード署名証明書による署名が別途必要で、証明書の本人確認に沿う名称を使用する。
自作証明書の信頼登録、警告抑止、署名用credentialの作成はこの変更で行わない。
配布処理はexeの実CompanyNameと署名状態を検査し、CIではインストール後のPublisherも照合する。

## 未署名の試験版と出所確認
現在の配布候補は未署名です。WindowsのSmartScreenや組織のポリシーによって警告・実行拒否が起こり得ます。
配布元、BUILD-INFO.jsonのsource_commit、SHA256SUMS.txtを照合してください。ハッシュ一致は発行者の本人性を証明しません。
セキュリティ機能や証明書検証を無効化する手順は提供しません。
THIRD-PARTY-NOTICES.txtとDEPENDENCIES.jsonを同梱します。crate本体にライセンス本文がない依存は、バージョンを照合した上流のライセンス本文と原本の`.crate`を添付します。[上流ライセンスの取得元](../../apps/device-manager/licenses/UPSTREAM.md)を記録し、未確認の依存が一件でも残る場合はGitHub Releaseの公開を停止します。

## 保守担当者が配布物を作る
Windows x64開発環境で、対象cloneのcleanなcommitをcheckoutしてから実行します。Python 3.12、Node.js、Rust MSVC toolchain、Visual Studio/Windows SDKを用意します。PlatformIO Core `6.1.19`はisolated Python environmentへ導入し、firmware build/cache、Cargo cache/target、AVRDUDE archiveとbundle stagingは一つの絶対task temporary rootに置きます。

    cd apps/device-manager
    npm ci
    $env:SELFRIONETTE_TASK_TEMP_ROOT = Join-Path $env:TEMP 'codex\selfrionette-device-manager\<unique-task-id>'
    New-Item -ItemType Directory -Path $env:SELFRIONETTE_TASK_TEMP_ROOT
    npm run package:windows
    npm run test:distribution

package scriptはPlatformIO memory-budget gateを通したsource-injected firmware imageを一度ビルドし、manifest/image hashを生成します。AVRDUDE v8.3のWindows x64 upstream archiveは固定SHA-256を照合し、実行ファイル・configurationと上流`COPYING`を同梱します。依存cache、firmware build、Cargo target、staging filesはtask temporary root内で使い終わった後に削除します。
出力はapps/device-manager/dist/配下です。版・source commit・product input・lockfile hash・firmware package・updater runtimeのprovenanceを保存し、dirtyな作業ツリーから配布候補を作りません。
通常のDebug build/devは従来どおり未commitの開発作業に使用できます。
CIはPR・手動実行・`device-manager-v*`タグで候補をbuild-onceし、独立runnerで同じartifactのproduct input fingerprint・checksum・配布試験を検証します。タグ実行では、Windows配布のビルド・別runnerでの検証がすべて成功し、source commit・firmware/runtime SHA-256・第三者ライセンス通知を確認できた場合だけ、日本語Release Notesとともに、タグの版体系に応じてGitHubの正式ReleaseまたはPre-releaseを公開します。既存の配布物を再利用するだけでなく、タグに対応するソースで一度ビルドし直します。Git tagの作成・pushはこの文書やpackage scriptでは行いません。

通常PRからの公開は行わず、公開処理だけに最小限の`contents:write`権限を与えます。署名は未導入であり、クリーンPC・完全オフライン初回導入の確認は引き続き必要です。

## 一次資料
- [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)
- [Microsoft WebView2の配布](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution)

- [Tauri: publisher](https://v2.tauri.app/reference/config/#publisher)
- [Microsoft: コード署名](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)

開発用起動: selfrionette-device-manager.exe --developer-mode（機能を有効化）、--developer-mode --synthetic（合成画面で開始）。--synthetic単独や未知/重複引数は終了コード2で拒否します。通常利用にこれらの指定は不要です。
