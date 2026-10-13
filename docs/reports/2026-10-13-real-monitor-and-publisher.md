---
status: evidence
owner: apps/device-manager
date: 2026-10-13
related:
  - docs/contracts/gui-live-monitor.md
  - docs/architecture/device-manager-workflow.md
  - docs/operations/device-manager-distribution.md
---
# 実機の未認識・デスクトップ表示・発行元の修正

## 原因
配布済み0.1.0-preview.1 / b59c3e2はGUIが固定合成fixtureだけを扱い、OS列挙やserial backendへ接続していなかった。USBを挿しても合成個体の一覧は変化しない。これはアプリの実機機能未実装であり、利用者のUSBドライバー不良の証拠ではない。

## 実装
通常起動を実機モードへ変更。OS列挙はread-onlyで、接続前の確認を通したportだけを既存serial layerで開く。旧Sim/v2共通の7ch vectorを既存core parserで読み、値・受信停止・機器診断を表示する。個体情報はinfoの明示送信と応答で扱い、未応答を合成IDで補わない。
実機と合成調整の状態・値を分離。実機接続中は切断後に合成へ切り替える。校正/tare/reset/provision/EEPROM/flash/robot writeを追加しない。
受信は40ms/128行単位、最新7値と直近8診断に限定。500ms失効、重複timestampで鮮度を更新しない、metadata変化/時刻後退/known frame不正/読込み失敗で旧値を消してhandleを解放する。GUI側もIPC待ち中の旧値を期限で隠す。

ウィンドウ標準タイトルバーを残し、本文側の製品名headerを削除。32px基準のデスクトップ操作寸法、本文16px、小さい対象toolbarと横tab/縦手順を使用。固定310pxの手順余白と空の下線を削除し、非表示操作の説明が現れる重複も修正した。タッチ環境は44px以上の対象を維持する。
発行元はtauri.conf.jsonのXpotato.net、Webサイトはhttps://xpotato.net。アプリ情報は埋込み設定を読み、インストーラー/EXEと同じ名称を使う。Tauri生成Configがpublisher/homepageを落とすことを実行試験で確認し、source-of-truthを埋込み参照するよう修正した。identifierはdev.selfrionette.managerのままで、保存先を改名しない。名称の表示とAuthenticodeによる本人性は別であり、署名証明書・信頼ストアは変更しない。

## ローカル検証
- Rust workspace: 77件PASS。
- GUI Rust use cases: 25件PASS（既存12 + 実機モニターfake backend 13）。
- frontend/controller/navigation: 12件PASS。
- 配布/metadata invariant: 11件PASS。
- workspace/application fmt・Clippy: PASS、警告ゼロ。
- Windows native Debug build: PASS。
- 実Tauri/WebView2/Rust IPC: 9群PASS。最終run-NDJZL7。

列挙だけではopen/writeしないこと、未列挙port拒否、旧要求拒否、無送信の旧形式受信、infoだけの送信、7chの値、不正数値/切断/再初期化/metadata変更、重複時刻/失効、診断上限、drop時解放をfake backendで試験した。
nativeでは実機モードで起動してport列挙を確認し、接続handleがないことを確認。その後明示的に合成モードを選んで従来の全校正/失効/保存/再開試験を実行した。実機/合成両モードの3画面を320/640 CSS pxで確認。タイトル重複なし、アプリ情報を開いてもsession不変、publisher/版表示、Escapeとfocus復元を確認。

## 配布と境界
版は0.1.0-preview.2。Release/NSIS/ZIPに同じsource commitを付け、EXEのCompanyName、setupの会社名、インストール後のPublisherを生成/配布試験で照合する。CI結果と配布物はPR #12で記録し、ローカルDebug試験と区別する。
対象の物理デバイスに対するserial openはこの作業中に行っていない。実機から実際に値が届くこと、センサの物理極性、実機校正、Firmware更新、異なるPCでの実機受入は未検証。実機監視コードの完成と物理受入を混同しない。

## 配布CIのPowerShell境界修正
Windows hosted runnerのpwshからNode経由でpowershell.exeを起動すると、PS7のPSModulePathが継承され、Get-AuthenticodeSignatureのmodule読込みが失敗した。metadata確認の子processだけからPSModulePathを除外し、Windows PowerShellが標準値を構築するように修正。署名/Publisherチェックは削除せず、親の環境を変更しない回帰試験を追加した。根拠: https://learn.microsoft.com/powershell/module/microsoft.powershell.core/about/about_psmodulepath#starting-windows-powershell-from-powershell-7

CIの初回WebView起動ではCDP endpoint確立直後にpage targetがまだ存在せず、旧試験コードがundefinedを参照した。実アプリのtauri.localhost pageが公開されるまで最大20秒の有限待機を追加。別pageを生成せず、未公開/無関係なpage/早期process終了を拒否する3件の回帰試験を追加した。アプリ本体の動作・検証assertionは変更していない。
