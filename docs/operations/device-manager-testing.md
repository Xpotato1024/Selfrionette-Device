---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-13
canonical_for:
  - Device Manager test layers, change scope and binary reuse
---
# Device Managerの検証設計

## 開発中の入口
appディレクトリの npm run check:fast は単体試験・配布条件試験・実HTML/CSS/JSの動的表示試験を別processで並列実行する。Rust build/NSIS生成/実機アクセスはしない。表示試験は既存Edgeを一時profileで使用し、in-memory IPC fixtureだけを渡す。既定のEdge配置がない環境はSELFRIONETTE_UI_BROWSER_EXEで検証用Chromium系browserを明示する。実機認定ではない。
時間と結果はsrc-tauri/target/desktop-qa/fast-report.json、個別の画面証拠はその配下に保存する。署名には手を加えない。

## CIの層と並列化
Host Rustはpull_request、mainへのpush、手動で実行する。codex branch pushとPRの二重起動をしない。同じPRの古いrunはcancelする。tools/ci-scope.mjsがbase SHAとheadの差分を分類する。
CSS/描画専用JSは高速表示試験。app/hardwareのIPC結線JSは実native境界も選ぶ。試験専用tool修正は製品入力を変えないため、既存artifactを再利用してnative受入を実行できる。Rust共通処理はWindows/Linuxのcore試験とnative境界。shell/configはnative境界。未知の変更は全層を選択する。型付き通信や保存/失効の既存試験を削除しない。
core OS間、frontend、nativeは独立job。可変の記録ディレクトリ、レジストリ、実機handleを共有して並列化しない。
キャッシュはOS・Rust版・lockfile・profile別。依存取得物とビルド中間出力を再利用するが、キャッシュhitを試験PASSにしない。結果は毎回実行する。

## 配布候補
Windows配布workflowの自動起動はインストーラー/配布設定/依存/配布ツールの変更に限定。通常UIの反復では生成しない。配布候補を渡す時点では手動または配布変更によるrunを完了させる。自動公開Releaseはしない。
build jobがRelease EXE/ZIP/setupを一度生成し、未検証のbuild artifactとして保存。verify jobが同じバイト列を取得し、導入/実Tauri/更新/取消/削除を検証する。verify失敗後はそのjobだけ再実行でき、成功したbuildを繰り返さない。
同一利用者のインストール/更新/削除は直列とし、別試験に分ける場合は別runner/VMを使う。旧版更新試験を新規導入試験で代用しない。

## 別checkoutや試験ツール修正時の再利用
npm run reuse:artifact -- <展開したbuild artifactのディレクトリ> はSHA-256、製品入力一覧、EXEを照合してこのcheckoutの検証先を設定する。ビルドはしない。以後npm run test:distributionで同じEXEを試験できる。
製品入力はRust/依存/設定/画面/アイコン/インストーラー/配布実装/同梱READMEを含む。追加/削除/変更があれば拒否し、再生成する。試験専用スクリプトの変更は製品ビルドを要求しない。BUILD-INFOのsource_commitは元のままで、現在のtester commitを製品commitとして上書きしない。古い形式で入力fingerprintを持たない配布物は再利用を拒否する。

## 表示の受入
符号・小数・桁上がり・指数表記・未受信/失効の系列で列境界、小数点位置、行のDOM同一性、表の矩形が不変であることを測る。800/1100/1600の論理幅と1/1.5/2のdeviceScaleFactorを試験。縦手順は常設し、ページ全体ではなく必要なペインだけスクロールする。DPIの実変更やスクリーンリーダーは別受入として残す。

## 通常モード・COM候補の回帰試験
Fast UIはtest-onlyのin-memory adapterで候補2台/その他1台、descriptor変更、明示開発者確認、接続中切替拒否、読取待機中の切断を再現する。adapterは配布frontendに含めない。
実Tauri試験は通常起動で合成IPCが拒否されることとWin32 GetMenuによるnative menuの実在を確認する。開発者機能は実menu commandから有効化し、その後は従来の合成校正/履歴/失効試験を維持する。CLIオプションと再起動resetも別processで確認する。実機portは開かない。
