# Selfrionette Device Manager（合成試験版）

Rust coreを共有するTauri v2 GUIです。製品の目的・状態・操作体系は[製品・操作設計](../../docs/architecture/device-manager-workflow.md)を正とします。
Windows native Debug build・起動・Rust IPC・合成記録の保存/再読込をLLM-01で確認しました。
合成fixture専用です。実機ID/番号/極性/校正値は未測定で、hardware_verified/control_readyは常にfalseです。

## 使用者向け（ビルド不要）

[バイナリの取得・起動・更新](../../docs/operations/device-manager-distribution.md)を参照してください。setup.exeとZIPを提供する構成です。
以下は開発者向けの手順です。使用者にビルド用環境変数やMSVCの設定を求めません。

## 開発者向け: native GUIのsetup/start/stop
Rust stable、Node.js 24、MSVC v143 x64/x86、Windows SDK、WebView2が必要です。
[公式prerequisites](https://v2.tauri.app/start/prerequisites/)を確認してください。
標準のVisual Studio/Build Toolsはvswhereで自動検出し、Developer Command Promptの環境も利用できます。
カスタム保存先は当該terminalのSELFRIONETTE_VS_ROOTに指定します。特定PCのDドライブをデフォルトにしません。
LLM-01固有の既存検証環境は、必要なときだけ$env:SELFRIONETTE_VS_ROOT = 'D:\Xpotato-apps\toolchains\vs2022-msvc'と明示します。
run-native.cmdはsetlocal/VsDevCmdで環境を設定し、global PATHや認証設定を変更しません。
npm/native Cargoのlockfileを同梱しています。PowerShellで次を実行します。

    # 任意のclone・作業用ローカルブランチのrepository rootから
    cd apps/device-manager
    npm ci
    .\run-native.cmd build
    .\src-tauri\target\debug\selfrionette-device-manager.exe

開発中は .\run-native.cmd dev を使えます。
起動だけでは接続も校正も行いません。共通の「対象」で合成個体を選び、横タブの「調整」で開始します。
左右の選択は表示対象だけを変え、各個体の基準・候補・確認を保持します。やり直しは別の明示操作です。
終了はnative windowを閉じます。devでは開発terminalもCtrl+Cで停止します。
今回確認したのはDebug executableであり、Release/bundle配布や実機受入は未実施です。

## 合成記録の保存・閲覧・再開
基準 → 品質を満たす7ch候補 → preview → 確認、の順で保存を許可します。
個体別保存はapp config配下のsynthetic-fixtures/<stable ID>/に新しい有限記録を作ります。
SELFRIONETTE_GUI_FIXTURE_DIRで当該processだけに絶対pathの試験storeを指定できます。
1 ID 32件、1記録32768 bytes。自動削除・既存記録の上書き・実機recordの保存は行いません。
firmware=synthetic-*、assembly=fixture-*だけを受け付けます。

timestamp、元7ch、履歴baseline、差分、符号補正後、無次元値を分離して保存します。
元値はnegation/tare/filter後であり、raw ADCやN単位のforceではありません。
physical draft（ID/組立/firmware/符号/revision/根拠）とoperation draft（ユーザー/姿勢/epoch/span）を分けます。
逆方向spanは未測定のままです。表示clamp前の値とsaturationも区別します。
「診断・記録」→「保存記録」→「最新記録を表示」は、個体/firmware/組立/ユーザー/姿勢を照合した比較専用の閲覧です。成功・失敗とも現在の調整を変更しません。
「条件を照合して調整を再開」は確認dialogを経て現在の調整を失効させ、新しいsessionを開始します。過去の基準・符号・span・確認は復元しません。
再判定・基準・手動変更・再開等の失敗時に、依存する旧承認を利用できない規則を維持します。
「この個体の調整を失効」は開発用のローカル試験であり、実機へtare/resetを送信しません。

## 検証
npm testで表示専用adapter、通信controller、閲覧専用ステップナビゲーションの9件の回帰テストを実行します。
cargo test --workspace --lockedでcore/serial/CLIを、cargo test --manifest-path apps/device-manager/src-tauri/Cargo.toml --lib --lockedでGUI use caseを検証します。
npm run test:nativeは本物のTauri executable/WebView2/Rust IPCを使い、9群を実行します。
保存・閲覧・明示再開・再起動、左右の独立session、旧tokenの拒否、失敗後の承認失効、品質不明、表示範囲超過、未測定逆方向と画面QAを含みます。
試験はsrc-tauri/target/native-qa/run-*/へ合成store・report.json・native-device.png・native-adjust.png等を残します。
[WebView2の公式試験手順](https://playwright.dev/docs/webview2)に沿い、試験processだけにlocalhost CDPと専用profileを設定します。
試験後は自分のnative windowを通常closeし、終了code 0を確認します。実機I/Oはありません。
Host Rust CIにはWindowsのGUI software test/Clippy/Debug buildも含めます。native画面QAはローカル検証と区別します。

## Build Tools導入の記録
Microsoft署名済みbootstrapper/呼出wrapperは D:\Xpotato-apps\codex_room\device-calibration-prep にあります。
helper本体/回帰テストはrepositoryの tools/windows-msvc/ で管理します。
最小v143 componentと必須MSBuildのみ、recommended無効、再起動なしで導入しました。
registration metadataはinstance f89da585、Build Tools 17.14.37710.0、MSVC 14.44.35207を示します。
この実行環境ではvswhere列挙は空のままですが、metadata・署名Validのcl/link・library・既存SDKを照合し、実ビルドで利用を確認しました。
helperは空のJSON配列を誤検出しない修正を行い、PowerShell 5.1の21件の回帰テストがPASSしています。
不完全登録・不正JSON・複数instance・他installer・未登録の既存保存先は導入を止め、重複作成を避けます。
[Microsoft契約条項](https://visualstudio.microsoft.com/license-terms/vs2022-ga-diagnosticbuildtools/)への本人同意は取得済みです。

## ブラウザーの表示確認
Nodeだけで npm run preview を実行し、同じPCの http://127.0.0.1:4173 を開きます。終了はCtrl+Cです。
固定のpresentation表示で、Rust判定・承認・保存/再読込を実行しません。native GUIの代替にはしません。
USB/serial、info、tare、reset、EEPROM、flash、robot接続/actuationはこのappに実装していません。

## 画面の読みやすさと操作
横タブは「入力確認」「調整」「診断・記録」の作業種別です。調整だけ縦の番号付き手順を持ち、本文は一手順だけを表示します。
完了した手順へ戻るだけでは設定を変更しません。基準や応答を再実行するときは、後続確認が失効することを確認dialogで示します。
未到達手順は実行不可です。閲覧位置とRustが持つ達成状態・操作許可を別に扱います。
タブは左右矢印/Home/Endでフォーカスを移動し、Enter/Spaceで表示を選びます。表示移動だけではcommandを送りません。
対象とID、合成モードはそれぞれ共通領域に一度だけ表示します。保存手順では結果表を再掲せず、PCへの保存と装置設定の変更を区別します。
16pxの本文・ラベル・表見出しを維持し、未使用の注意列や未比較の数値表は表示しません。選択ch詳細は展開式です。
狭幅では手順一覧を開閉でき、中央の作業内容を読める幅を確保します。
デジタル庁のローカルデザインシステムの考え方を参考にし、政府の公式製品・認定・ロゴは表示しません。
入力エラーは入力欄に関連づけます。Tab/矢印/Enter/Space、対象切替後のfocus保持、確認取消、320/640 CSS pxのreflow、主要文字色のコントラストを検証します。
スクリーンリーダー、native 200%/Windows DPI、Release/bundle配布、実機受入、利用者による操作性評価は未実施です。

## 装置単独利用・Firmware更新への実装計画
[ADR-0009案](../../docs/decisions/0009-standalone-device-configuration.md)でManager常駐不要の方針を、[装置設定契約](../../docs/contracts/device-persistent-settings.md)でEEPROM recordを定義しています。
今回追加したC++の保存journalはハードウェア非依存の部品です。現行Firmwareのserial/EEPROMへは結線していません。
[Firmware更新契約](../../docs/contracts/firmware-update.md)は対象確認・設定保護・書込み・再接続確認の設計です。書込みbackendと実機向け保守画面は未実装です。
現行の合成記録をEEPROMへコピーしたり、未確認のモデルから極性やch位置を推測したりしません。

参照: [ローカルデザインシステム](https://design.digital.go.jp/dads/guidance/local-design-system/)、[文字](https://design.digital.go.jp/dads/foundations/typography/)、[ステップナビゲーション](https://design.digital.go.jp/dads/components/step-navigation/)、[Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/)。
