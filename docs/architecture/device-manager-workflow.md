---
status: canonical
owner: architecture
last_verified: 2026-10-13
canonical_for:
  - Device Manager product scope and information architecture
  - synthetic application workflow and action availability
related:
  - docs/contracts/device-host-boundary.md
  - docs/contracts/calibration-preview.md
  - apps/device-manager/README.md
---
# Device Managerの製品・操作設計

## 製品の目的と今回の到達範囲
Selfrionetteの各個体を識別し、接続・組立対応・入力品質を確認し、必要な校正と使用前調整を行い、由来と有効性を追跡できる入力を利用先へ渡せるようにする。
GUIはその操作窓口であり、校正機能を一画面へ並べることを製品の目的にしない。

現実装は合成fixture専用。実機接続、物理校正の成功判定、firmware変更、EEPROM、ロボット制御は提供しない。
hardware_verified / control_readyは常にfalse。プレビューの確認・記録保存を実機の利用可能性と同一視しない。
Device / Xpotato-Simの既存境界を維持し、channel→XYZ/joint、ロボット速度gain、座標系・実験はSimが所有する。

## 操作体系
通常利用は「対象個体・現在の状態・入力・次に必要な操作」を中心にする。初回の組立対応・個体校正と、毎回の使用前調整は後続の別作業として設計する。
実機対応前に、合成工程を実機の校正wizardと称して公開しない。

| 作業場所 | 現在提供するもの | 提供しないもの |
|---|---|---|
| デバイス | 左右の合成個体選択、調整/保存状態、7ch一覧と選択chの詳細 | 自動接続、実測値、校正済み認定 |
| 調整 | 現在工程、合成基準・応答・比較・確認・記録 | 全工程の主操作の常設、ロボットの動きから符号を推測 |
| 診断・記録 | 全演算値、根拠、最新記録の閲覧、明示的な再開 | 履歴を開くことによる暗黙のsession変更 |
| 保守（後続） | 画面・操作はまだ提供しない | 更新や永続変更の見せかけの操作 |

合成品質シナリオ・失効試験は「診断・記録」の開発用詳細に分離する。合成という由来と実機未接続は全画面共通で示す。
Tauri+Rust coreを継続し、GUI framework、daemon、DBを増設しない。

## 個体・選択・調整
合成の左右2個体は明示したfixture IDを持ち、各個体のsession・epoch・閲覧記録・保存結果を別々に保持する。
「表示対象を選択」と「調整を開始/やり直す」は異なるcommand。対象選択だけでは一方の基準・候補・確認を消さず、他方へコピーもしない。
この固定fixture定義は実機role bindingではない。COM順、形状の左右、ID文字列から実機の役割を導出しない。

## 状態の単一性
CalibrationSessionのphaseは基準、候補、現在preview、承認から導出する。GUI工程番号を独立に保存しない。
workflow.rsがcore状態からview、次操作、許可操作と不許可理由を作り、coreのpreconditionで実行する。
frontendは表示・入力収集・画面切替だけを持ち、stageの数値比較で保存や確認を許可しない。

保存済みは現在のpreview全体との一致で評価する。確認済み、保存済み、実機で有効は別状態。
現在の結果の二重保存は拒否し、再観測・変更後は新しい記録として保存できる。
物理位置未確認の段階では受信ch 0–6を保持し、physical=nullとする。wire+1を実番号として表示しない。
未測定逆方向はnull、表示範囲超過はclamp前の値と別状態。0や実機センサ飽和へ置換しない。
入力バーは表示値の大きさを0–1で示す冗長表現。数値と注意表示を残し、未測定時はバーを描かない。

## 型付きcommandと対象照合
private IPCはdevice_command(Request)。Requestは型付きcommandと期待対象tokenを持つ。コロン連結の自由文字列をbusiness APIにしない。
status以外は選択個体ID・epoch・workspace revisionが現在状態と完全一致したときだけ処理する。
不一致の要求は現在の個体を変更せず拒否する。同じtokenの再送も拒否する。
構造不正な未解釈要求はdeserialize段階で拒否する。解釈済み変更要求の値検証が失敗した場合は依存する旧preview承認を残さない。
再判定の入口では候補も失効させ、失敗後に手動反転で旧候補へ復帰できない。

結果は現在のviewと構造化errorを返す。frontendは成功を仮定せず、そのviewを描画する。
frontend controllerは要求を同時送信せず、古いrevisionの応答を描画しない。通信失敗時はstatusで復旧し、それも失敗した場合は値と変更操作を消して再取得を求める。
再開・やり直しの確認dialogも対象tokenを保持し、確認中に対象が変わった場合は実行しない。

## 閲覧と再開
read_historyは対象個体に適合する最新記録を検証して閲覧する。調整未開始でも閲覧できる。
成功・失敗とも現在のCalibrationSession、epoch、core revision、基準、候補、承認を変更しない。失敗した場合は古い閲覧記録だけを消す。
workspace revisionは画面snapshotの順序を守るため更新されるが、これは調整の失効ではない。
現段階では同じ個体・firmware・組立・利用者・姿勢に適合する最新1件だけを表示する。複数世代を任意選択する履歴ブラウザーは後続。

resumeは明示確認後に現在の調整を失効させ、閲覧した記録と条件を照合して空のsessionを開始する。
過去の基準・符号・span・readinessは自動復元しない。失敗後も旧承認を使えない。
単なる閲覧との違いをボタンと確認dialogで説明し、現実装は設定値の取り込みを提供していないことを明示する。

## ローカルスタイル
デジタル庁の指針を基盤に、装置の情報密度へ適合させる。政府ロゴ・認定表示を使わない。
白と中立灰の作業面、細い罫線、控えめな青緑の操作色を基本とする。影付きカードや大見出しのキャッチコピーを並べない。
本文・入力・ラベル・表見出しは16px。数値は桁を揃え、説明と演算の詳細は必要な場所に置く。
全画面で対象IDを保持し、主操作は現在工程の一つだけを強調する。再比較・手動変更・試験操作は詳細へ分離する。
キーボード、可視フォーカス、関連づけた入力エラー、色以外の状態表現、320/640 CSS pxのreflowを試験する。
入力エラーは集中して読み上げるlive領域に重複登録しない。成功通知は専用のpolite statusで知らせる。

## モデル資産
候補原本: https://drive.google.com/drive/folders/1fOsqB8q4594Iq4uLSbua7ylhxqM6wWd7
前段の調査でv4のbase_layer.3mfを確認したが、印刷配置だけから完成時の組立transform・配線番号・荷重方向を確定しない。
今回のUIには未確認のモデル・hotspot・方向矢印を実装しない。採用revision、原本ID/hash、組立姿勢、channel対応を照合してから静止図を優先し、回転が作業に必要な場合だけ3D化する。
モデルが利用不能でもchannel一覧と数値から全操作を完了できる設計を維持する。

## 後続の実機受入
実機の取得品質と失効条件、日常準備と個体校正の分離、二台のidentity/role、serial所有権と利用側への引渡し、firmware保守は別段階で定義する。
実機未測定項目の値を今回の合成fixtureから推奨値として転用しない。

## 参照
- [デジタル庁: ローカルデザインシステム](https://design.digital.go.jp/dads/guidance/local-design-system/)
- [デジタル庁: 文字](https://design.digital.go.jp/dads/foundations/typography/)
- [デジタル庁: 入力欄のアクセシビリティ](https://design.digital.go.jp/dads/components/input-text/accessibility/)
- [Playwright: WebView2](https://playwright.dev/docs/webview2)
