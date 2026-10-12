# Selfrionette Device Manager（合成試験版）

Rust coreを共有するTauri v2 GUIです。製品の目的・状態・操作体系は[製品・操作設計](../../docs/architecture/device-manager-workflow.md)を正とします。
Windows native Debug build・起動・Rust IPC・合成記録の保存/再読込をLLM-01で確認しました。
合成fixture専用です。実機ID/番号/極性/校正値は未測定で、hardware_verified/control_readyは常にfalseです。

## native GUIのsetup/start/stop
Rust stable、Node.js 24、MSVC v143 x64/x86、Windows SDK、WebView2が必要です。
[公式prerequisites](https://v2.tauri.app/start/prerequisites/)を確認してください。
LLM-01の確認済みtoolchainは D:\Xpotato-apps\toolchains\vs2022-msvc です。
既存の別保存先を使う場合は当該processの SELFRIONETTE_VS_ROOT に指定します。
run-native.cmdはsetlocal/VsDevCmdで環境を設定し、global PATHや認証設定を変更しません。
npm/native Cargoのlockfileを同梱しています。PowerShellで次を実行します。

    cd D:\Xpotato-apps\Selfrionette-Device\apps\device-manager
    npm ci
    .\run-native.cmd build
    .\src-tauri\target\debug\selfrionette-device-manager.exe

開発中は .\run-native.cmd dev を使えます。
起動だけでは接続も校正も行いません。「デバイス」で合成個体を選び、「調整」で開始します。
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
「診断・記録」→「最新の保存記録を表示」は、個体/firmware/組立/ユーザー/姿勢を照合した比較専用の閲覧です。成功・失敗とも現在の調整を変更しません。
「条件を照合して再開」は確認dialogを経て現在の調整を失効させ、新しいsessionを開始します。過去の基準・符号・span・確認は復元しません。
再判定・基準・手動変更・再開等の失敗時に、依存する旧承認を利用できない規則を維持します。
「合成sessionを失効」は開発用のローカル試験であり、実機へtare/resetを送信しません。

## 検証
npm testで表示専用adapterと通信controllerの回帰テストを実行します。
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
「デバイス」「調整」「診断・記録」に分け、全工程の操作を常時表示しません。
対象IDと合成モードを共通表示し、調整はRustが返した現在工程と操作可否で描画します。
16pxの本文・ラベル・表見出し、数値の桁揃え、選択chの詳細、表示値に対応する入力バーを使います。
未測定方向は数値・バーを捏造せず、値なしと理由を表示します。モデルや物理位置は未確認です。
デジタル庁のローカルデザインシステムの考え方を参考にし、政府の公式製品・認定・ロゴは表示しません。
入力エラーを関連づけ、Tab/矢印/Enter/Space、確認dialogのcancel、320/640 CSS pxの全ページreflow、主要文字色のコントラストを検証します。
スクリーンリーダー、native 200%/Windows DPI、Release/bundle配布、実機受入、ユーザビリティ評価は未実施です。

参照: [ローカルデザインシステム](https://design.digital.go.jp/dads/guidance/local-design-system/)、[文字](https://design.digital.go.jp/dads/foundations/typography/)、[入力欄のアクセシビリティ](https://design.digital.go.jp/dads/components/input-text/accessibility/)。
