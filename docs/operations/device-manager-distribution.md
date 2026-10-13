---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-13
canonical_for:
  - Windows binary usage and distribution
related:
  - apps/device-manager/README.md
---
# Device Managerをビルドせずに使う

## 対象と配布物
Windows 11 x64向けの開発中プレビューです。実機のシリアル入力確認と合成調整試験を提供します。実機校正・装置設定保存・Firmware書込みは未対応です。
Rust、Node.js、Visual Studio、Windows SDK、Git、ソースのcloneは使用者に不要です。
ソースの開発・ローカルブランチの検証は[開発者向けREADME](https://github.com/Xpotato1024/Selfrionette-Device/blob/main/apps/device-manager/README.md)に従ってください。

| 配布物 | 使い方 | 実行時の前提 |
|---|---|---|
| 名前末尾が -setup.exe | 実行して案内に従い、スタートメニューから起動 | WebView2未導入時のオフラインインストーラーを同梱 |
| 名前末尾が -portable.zip | 全体を展開し、selfrionette-device-manager.exeを実行 | Microsoft Edge WebView2 Runtimeが導入済みであること |

通常利用者にはsetup.exeを推奨します。ZIP内から直接実行せず、必ず展開してください。
ZIP版はインストール不要ですが、設定・記録をUSB等の配布フォルダー内だけに保存する意味のportableではありません。
VCランタイムは静的リンクし、画面のHTML/CSS/JavaScriptはexe内に含めます。開発サーバーやビルドツールは起動しません。
Windows ARM64/x86、Windows 10、未導入WebView2環境への実インストールは別途受入対象です。

## 取得と起動
公開版は[GitHub Releases](https://github.com/Xpotato1024/Selfrionette-Device/releases)のAssetsから取得します。
まだ公開版がない場合、[Windows distribution workflow](https://github.com/Xpotato1024/Selfrionette-Device/actions/workflows/device-manager-distribution.yml)の成功したrunのArtifactsを使用します。Artifactsの取得はGitHubへのログインが必要で、保持期限があります。
Draft Releaseは一般利用者には見えません。Source code.zipは実行バイナリではありません。

setup.exeは現在の利用者向けにインストールします。組織の実行・インストール制限を回避するものではありません。
WebView2は実行時依存です。ZIP版で起動しない場合は同じ版のsetup.exeかMicrosoft公式のWebView2 Runtimeを利用してください。
WebView2同梱インストーラーは初回のRuntime導入にネット接続を要求しない設定です。Runtimeの将来の自動更新にはネット接続が必要です。

起動時は「実機」です。USB接続後に「再検索」、接続先を選んで「接続」、確認画面で「接続する」を押します。一覧取得だけではポートを開きません。選択ポートを一台だけ115200 baudで開き、7chのvector形式を受信します。
値が届くまで「受信待ち」、更新が止まれば旧値を消します。OS一覧にない場合と、ポートはあるが接続/受信できない場合を分けて確認してください。機器時刻後退や不正データでは停止し、自動で別個体へ接続しません。
「診断・記録」の個体情報取得はProtocol v2のinfoだけを送信します。旧Firmwareの7ch入力も受信できますが、個体IDは応答しない場合があります。
GUI操作確認は入力元を「合成試験」へ切り替え、対象個体と「調整」を選びます。実機接続中は先に切断してください。終了はウィンドウを閉じます。
合成モードの成功は実機の使用可能性を意味しません。

## 認識しない場合
この版より前の0.1.0-preview.1は合成専用で、USBを接続しても一覧は変わりません。
この版で一覧にない場合は、Windowsのデバイスマネージャーのポート表示とUSB接続を確認します。ポートはあるが接続できない場合は、シリアルモニターやSimなどによる占有を確認します。受信待ちの場合は通信形式/Firmwareを確認します。ドライバー不良やFirmware書換えが必要とは、GUIの未表示だけで決めつけません。

## 保存先と更新
PC記録は原則として %APPDATA%\dev.selfrionette.manager\synthetic-fixtures\ に保存します。exeのある場所は書込み先にしません。
開発者・試験者がSELFRIONETTE_GUI_FIXTURE_DIRを設定した場合だけ保存先を変更します。通常使用では環境変数設定は不要です。
更新はアプリを終了してから行います。ZIP版は旧版へ上書きせず新しいフォルダーへ展開すると比較できます。
記録の互換性は別契約です。版が異なる記録を読み込めると無条件に保証しません。

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
THIRD-PARTY-NOTICES.txtとDEPENDENCIES.jsonを同梱します。公開crateにライセンス本文がない依存は原本ソースも同梱し、公開Release前の確認対象をBUILD-INFO.jsonへ残します。

## 保守担当者が配布物を作る
開発環境のあるWindows x64で、対象のclone・ローカルブランチに移動し、変更をcommitしてから実行します。

    cd apps/device-manager
    npm ci
    .\run-native.cmd package
    npm run test:distribution

Developer Command Promptではnpm run package:windowsでも同じ処理を実行できます。
出力はapps/device-manager/dist/配下です。版・source commit・lockfile hash・ビルド環境を保存し、dirtyな作業ツリーから配布候補を作りません。
通常のDebug build/devは従来どおり未commitの開発作業に使用できます。
CIはPR・手動実行・device-manager-v* tagで配布候補を生成し、成功した検証の後にArtifactsへ保存します。自動公開Release・自動マージは行いません。
公開前に署名方針、同梱依存の表示条件、クリーンPC、WebView2未導入/オフライン導入、インストール・アンインストールを確認します。

## 一次資料
- [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)
- [Microsoft WebView2の配布](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution)

- [Tauri: publisher](https://v2.tauri.app/reference/config/#publisher)
- [Microsoft: コード署名](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
