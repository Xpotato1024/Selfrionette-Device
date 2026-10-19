---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-19
canonical_for:
  - Device Manager test layers, change scope and binary reuse
---
# Device Managerの検証設計

## 開発中の入口
appディレクトリの npm run check:fast は単体試験・配布条件試験・実HTML/CSS/JSの動的表示試験を別processで並列実行する。Rust build/NSIS生成/実機アクセスはしない。表示試験は既存Edgeを一時profileで使用し、in-memory IPC fixtureだけを渡す。既定のEdge配置がない環境はSELFRIONETTE_UI_BROWSER_EXEで検証用Chromium系browserを明示する。実機認定ではない。
時間と結果はsrc-tauri/target/desktop-qa/fast-report.json、個別の画面証拠はその配下に保存する。署名には手を加えない。

## CIの層と並列化
Host Rustはpull_request、mainへのpush、手動で実行する。codex branch pushとPRの二重起動をしない。同じPRの古いrunはcancelする。tools/ci-scope.mjsがbase SHAとheadの差分を分類する。
CSS/描画専用JSは高速表示試験。app/hardware/applicationのIPC結線JSは実native境界も選ぶ。試験専用tool修正は製品入力を変えないため、既存artifactを再利用してnative受入を実行できる。Rust共通処理はWindows/Linuxのcore試験とnative境界。Firmware変更も共有protocolとnative境界を選ぶ。shell/configはnative境界。未知の変更は全層を選択する。型付き通信や保存/失効の既存試験を削除しない。

品質付き操作のnative試験は `--developer-mode --scripted-hardware` と実際の `application_command` IPCを使用し、実portを開かずに同じRust service/parser/fitter/store/runtimeを通す。static表示fixtureや従来のsynthetic previewをこの操作の受入代替にしない。画面と取得根拠はtarget配下に保持する。実board・実EEPROM・実avrdude書込みは別の明示許可されたhardware試験で行う。
developer-onlyの`ScriptedTransport`は読取り時に新sampleを生成し、最短50 ms間隔とする。`device_ms`にはtransport生成からの実monotonic経過時間を32-bitのwrapで記録する。GUI/IPC待ちを固定50 msずつの架空のproducer時計に置き換えない。共通`LiveService`の250 ms freshness/backlog上限は維持し、共有fake clockで遅いconsumer・250 ms超の待ち・時計wrap・session変更を検査する。この模擬経路の成功を、実deviceの送信HzやUSB/OS遅延の証拠にはしない。
`tools/live-native-qa.mjs`は実WebViewのGUIを一操作一クリックで進め、操作受付番号・完了/error・新sample・capture完了を有限待機する。固定sleepやbusy時の再クリックで成功を作らない。113手順、別試行64区間、GUIからのprofile保存/一覧/再読込み、新neutral、明示apply、逆極性の正規化と不良品質による失効を同じexeで確認する。`SELFRIONETTE_QA_CDP_URL`指定時は外部起動の既存WebViewへ接続でき、registry変更を必要としない。reportのexe hashと3画面は配布対象の同一性を検査するために保持する。
core OS間、frontend、nativeは独立job。可変の記録ディレクトリ、レジストリ、実機handleを共有して並列化しない。
キャッシュはOS・Rust版・lockfile・profile別。依存取得物とビルド中間出力を再利用するが、キャッシュhitを試験PASSにしない。結果は毎回実行する。

## 配布候補
Windows配布workflowは製品に含まれるUI/core/Firmware/updater/配布設定の変更をPRでも検証する。高速UI job自体は配布物を生成しない。配布候補を渡す時点では該当sourceの配布runを完了させる。公開は明示的な`device-manager-v*` tagを入口とし、main履歴・独立検証・日本語notes・公開asset検査がすべて通った場合だけprereleaseにする。PRから公開しない。
build jobがRelease EXE/portable ZIP/setup/Firmware ZIPを一度生成し、未検証のbuild artifactとして保存。verify jobが同じバイト列を取得し、導入/実Tauri/更新/取消/削除を検証する。同梱Firmware・固定updater runtime・licenseのhashと、portを指定しないruntime起動も確認する。品質付きGUIは同じexeのstartup IPC、実測fixtureによる校正・保存・現在入力までのreportと3画面を要求する。古いsynthetic screenshotだけで公開gateを満たさない。verify失敗後はそのjobだけ再実行でき、成功したbuildを繰り返さない。
同一利用者のインストール/更新/削除は直列とし、別試験に分ける場合は別runner/VMを使う。旧版更新試験を新規導入試験で代用しない。

## 別checkoutや試験ツール修正時の再利用
npm run reuse:artifact -- <展開したbuild artifactのディレクトリ> はSHA-256、製品入力一覧、EXEを照合してこのcheckoutの検証先を設定する。ビルドはしない。以後npm run test:distributionで同じEXEを試験できる。
製品入力はRust/依存/設定/画面/アイコン/インストーラー/配布実装/同梱READMEを含む。追加/削除/変更があれば拒否し、再生成する。試験専用スクリプトの変更は製品ビルドを要求しない。BUILD-INFOのsource_commitは元のままで、現在のtester commitを製品commitとして上書きしない。古い形式で入力fingerprintを持たない配布物は再利用を拒否する。

## 表示の受入
符号・小数・桁上がり・指数表記・未受信/失効の系列で列境界、小数点位置、行のDOM同一性、表の矩形が不変であることを測る。800/1100/1600の論理幅と1/1.5/2のdeviceScaleFactorを試験。縦手順は常設し、ページ全体ではなく必要なペインだけスクロールする。DPIの実変更やスクリーンリーダーは別受入として残す。

## 通常モード・COM候補の回帰試験
Fast UIはtest-onlyのin-memory adapterで候補2台/その他1台、descriptor変更、明示開発者確認、接続中切替拒否、読取待機中の切断を再現する。adapterは配布frontendに含めない。
実Tauri試験は通常起動で合成IPCが拒否されることとWin32 GetMenuによるnative menuの実在を確認する。開発者機能は実menu commandから有効化し、その後は従来の合成校正/履歴/失効試験を維持する。CLIオプションと再起動resetも別processで確認する。実機portは開かない。

## preview.2の回帰範囲

`tools/usability-ui.mjs`は既存desktop試験から呼び、800/1100/1600pxと125/150%相当の利用可能幅で、更新画面への候補引継ぎ、再検索、ラベル/control矩形、横overflow、handoff、計画＋確認による書込みgate、系統変更による承認失効、pause/resumeの古い表示消去を検査する。`SELFRIONETTE_UI_CDP_URL`は既に起動済みの検証用browserを利用する入口で、製品には含めない。実OS DPI切替の証拠にはしない。

Rustのnative adapterとupdater mockは、受信失敗後の再open、known ID差替え、descriptor変更・重複、owner排他、operation epoch、modern/legacy往復、全application領域のdense HEXとverify-only、中断journalを検査する。native GUI通し試験には明示pause/resumeを加える。実機portは開かない。

ローカル実行環境で子process起動が拒否された場合、GUI/installer検証をPASSに置き換えない。primaryは確定HEADで既存のfast/native/distribution/installer gatesを実行し、同じexe hashの画面証拠を保持する。
