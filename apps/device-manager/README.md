# Selfrionette Device Manager（offline準備）

Rust coreを共有するTauri v2 GUIのローカル実装案です。
合成fixtureのみを扱い、実機ID/極性/校正値は未測定です。
Windows native buildはBuild Tools不足のため未検証。ブラウザー表示確認をnative GUIの成功と扱いません。

## すぐに表示を確認する
Node.js 24を用意し、PowerShellで実行します。追加packageのinstallは不要です。

    cd D:\Xpotato-apps\Selfrionette-Device\apps\device-manager
    npm run preview

同じPCのブラウザーで http://127.0.0.1:4173 を開きます。
合成fixtureを開始 → 基準を作成 → 正常fixtureの候補確認 → 0.5のpreview、と進めます。
このモードは固定の合成表示です。Rust校正判定、preview承認、draft保存は行いません。
終了はterminalでCtrl+C。listenerは127.0.0.1だけで、起動時にserialを開きません。

## native Rust GUIのsetup/start/stop
Rust stable、Node.js、Microsoft C++ Build Tools（C++/Windows SDK）、WebView2が必要です。
[公式prerequisites](https://v2.tauri.app/start/prerequisites/)に沿って環境を準備した後、次を実行します。

    cd D:\Xpotato-apps\Selfrionette-Device\apps\device-manager
    npm ci
    npm run dev

native windowには「Rust coreを使うoffline GUI」と表示します。
起動だけでは接続も校正も行いません。合成fixtureを明示選択します。
候補がinconclusiveなら先へ進みません。手動override後はpreviewを作り直します。
exportは常にhardware_verified=false/control_ready=falseの合成draftです。
再ロードでbaseline/readinessを復元する機能はありません。
終了はwindowを閉じ、開発terminalもCtrl+Cで停止します。

## 検証と未完了
core fixture 10件、presentation adapter 2件、ブラウザーの7ch表示/保存禁止/epoch失効のQAがPASS。
native Tauri bridgeのcompile/起動、実機capture、実機profileの保存/再検証は未完了です。
実機のUSB/serial、info、tare、EEPROM、flash、robot接続/actuationをこのappから実行しません。
