# Selfrionette Device Manager（offline準備）

Rust coreを共有するTauri v2 GUIのローカル実装案です。
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
起動だけでは接続も校正も行いません。合成fixtureを明示選択します。
終了はnative windowを閉じます。devでは開発terminalもCtrl+Cで停止します。
今回確認したのはDebug executableであり、Release/bundle配布や実機受入は未実施です。

## 合成記録の保存・再読込
基準 → 品質を満たす7ch候補 → preview → 確認、の順で保存を許可します。
個体別保存はapp config配下のsynthetic-fixtures/<stable ID>/に新しい有限記録を作ります。
SELFRIONETTE_GUI_FIXTURE_DIRで当該processだけに絶対pathの試験storeを指定できます。
1 ID 32件、1記録32768 bytes。自動削除・既存記録の上書き・実機recordの保存は行いません。
firmware=synthetic-*、assembly=fixture-*だけを受け付けます。

timestamp、元7ch、履歴baseline、差分、符号補正後、無次元値を分離して保存します。
元値はnegation/tare/filter後であり、raw ADCやN単位のforceではありません。
physical draft（ID/組立/firmware/符号/revision/根拠）とoperation draft（ユーザー/姿勢/epoch/span）を分けます。
逆方向spanは未測定のままです。表示clamp前の値とsaturationも区別します。
再読込は個体/firmware/組立/ユーザー/姿勢を照合し、比較専用欄へ表示します。
履歴baseline・符号・span・確認状態をactive sessionへ復元しません。新しい基準と観測が必要です。
不正な再判定、再読込失敗、role/基準/手動override変更で古いpreview承認を使えません。
「tare/resetの失効を確認」はローカル状態の失効だけを行います。

## 検証
Windows core 34件（既存20・校正11・保存3）、core/GUI Clippy警告ゼロ、presentation adapter 2件を確認しました。
npm run test:native は本物のTauri executable/WebView2/Rust IPCを使い、7群の合成試験を実行します。
保存/再読込/再起動、失敗後の旧承認・候補の利用拒否、品質不明、飽和、未測定逆方向を含みます。
試験はsrc-tauri/target/native-qa/run-*/へ合成store・report.json・native-preview.pngを残します。
[WebView2の公式試験手順](https://playwright.dev/docs/webview2)に沿い、試験processだけにlocalhost CDPと専用profileを設定します。
試験後は自分のnative windowを通常closeし、終了code 0を確認します。実機I/Oはありません。

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
