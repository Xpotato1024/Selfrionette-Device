# Selfrionette Device Manager（offline準備）

Rust coreを共有するTauri v2 GUIのローカル実装案です。
合成fixtureのみを扱い、実機ID/極性/校正値は未測定です。
Windows native compile/起動は未検証。ブラウザー表示QAと区別します。

## すぐに表示を確認する
Node.js 24を用意し、PowerShellで実行します。追加packageのinstallは不要です。

    cd D:\Xpotato-apps\Selfrionette-Device\apps\device-manager
    npm run preview

同じPCで http://127.0.0.1:4173 を開きます。
合成fixture開始 → 基準 → 正常fixtureの候補確認 → 0.5のpreview、と進めます。
このモードは固定の合成表示です。Rust校正判定、承認、保存、再読込は行いません。
終了はterminalでCtrl+C。listenerは127.0.0.1だけです。

## native Rust GUIのsetup/start/stop
Rust stable、Node.js、MSVC v143 x64/x86、Windows SDK、WebView2が必要です。
[公式prerequisites](https://v2.tauri.app/start/prerequisites/)を確認してください。
npmとnative Cargoのlockfileを同梱しています。

    cd D:\Xpotato-apps\Selfrionette-Device\apps\device-manager
    npm ci
    npm run dev

native windowには「Rust coreを使うoffline GUI」と表示します。
起動だけでは接続も校正も行いません。合成fixtureを明示選択します。
終了はwindowを閉じ、開発terminalもCtrl+Cで停止します。

## 合成記録の保存・再読込
fresh baseline → 品質を満たす7ch候補 → preview → 確認、の順で保存を許可します。
個体別保存はapp config配下のsynthetic-fixtures/<stable ID>/に新しい有限記録を作ります。
1 IDにつき32件、1記録32768 bytesまで。自動削除や既存記録の上書きは行いません。
SELFRIONETTE_GUI_FIXTURE_DIRを当該processだけに設定すると、絶対pathの試験storeを指定できます。
実機のrecordは扱いません。firmware=synthetic-*、assembly=fixture-*を要求します。

保存原本にはtimestamp、7ch firmware sample、履歴baseline、差分、符号補正後、無次元値を分離して残します。
元値はnegation/tare/filter後であり、raw ADCやN単位のforceではありません。
physical draft（ID/組立/firmware/符号/revision/根拠）とoperation draft（ユーザー/姿勢/epoch/span）を分けます。
hardware_verified/control_readyは常にfalse。逆方向spanは未測定のままです。
再読込は個体/firmware/組立/ユーザー/姿勢を照合し、比較専用欄へ表示します。
履歴baseline・符号・span・確認状態をactive sessionへ復元しません。基準と観測をやり直します。
再読込失敗、role変更、基準変更、手動override、失効操作でも古い確認を使えません。
「tare/resetの失効を確認」はローカル状態の失効だけを行います。

## 検証と残る条件
Rust coreの校正10件、保存/再読込3件、presentation adapter 2件をofflineで検証しました。
独立レビューのP2（連番file名と履歴値の整合性）は修正済みです。
native bridgeのcompile/起動はBuild Tools導入前なので未完了です。
LLM-01にはWindows SDK 10.0.26100.0/WebView2があり、MSVC linkerはありません。
新規契約への同意前に停止する本人指示に従い、installerはdownload/起動/導入していません。
[Microsoft公式契約ページ](https://visualstudio.microsoft.com/license-terms/vs2022-ga-diagnosticbuildtools/)を確認後、導入を再開します。
USB/serial、info、tare、reset、EEPROM、flash、robot接続/actuationはこのappに実装していません。
