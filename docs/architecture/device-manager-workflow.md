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
Selfrionetteの各個体を、装置固有設定を保持して正しい意味の入力を出力するデバイスとして構成し、その設定・校正・診断・Firmware更新を管理する。通常利用の必須中継processにはしない。
GUIはその操作窓口であり、校正機能を一画面へ並べることを製品の目的にしない。

実機モードはOSのport列挙と明示接続による受信確認を提供する。実機の物理校正、firmware変更、EEPROM、ロボット制御は提供しない。合成調整と実機モニターの状態・値は別経路。実機受信の正本は[GUI実機モニター契約](../contracts/gui-live-monitor.md)。
hardware_verified / control_readyは常にfalse。プレビューの確認・記録保存を実機の利用可能性と同一視しない。
Device / Xpotato-Simの既存境界を維持し、channel→XYZ/joint、ロボット速度gain、座標系・実験はSimが所有する。

## 操作体系と情報密度
横方向は同格の作業種別を切り替えるタブ、手順のある作業だけ左側に縦の番号付きステップナビゲーションを置く。
中央には現在閲覧している一手順だけを表示する。一項目ごとに過剰分割せず、一つの判断・実行単位を一画面にする。

| 作業種別 | 今回の公開範囲 | 内部の構成 |
|---|---|---|
| 入力確認 | 実機の受信値 / 明示選択した合成個体の入力・状態 | 監視はwizardにしない。選択ch詳細は必要時に開く |
| 調整 | 合成基準・応答・比較・確認・PC記録 | 1 基準 / 2 応答 / 3 比較 / 4 確認 / 5 保存 |
| 診断・記録 | 実機の機器情報・診断 / 合成演算値・根拠・保存履歴 | 観測/閲覧は自由に移動。調整を暗黙に再開しない |
| 保守（後続） | 実機更新backendができるまでは未公開 | firmware-update.mdの対象/ファイル/保護/書込み/確認 |

製品全体では装置設定と使用前調整を別workflowとする。実機未対応の段階で合成処理を物理校正と称さない。

### 表示位置と実行状態を分離する
Rustのphase/steps/operationsが達成状態と実行許可の正本。frontendは閲覧中の手順を個体別に持つが、クリックで達成状態を変えない。
完了手順と現在実行可能な手順を閲覧でき、未到達手順は理由付きで実行不可。戻って見るだけではcommandを送らない。
完了手順の再測定・変更は明示確認を通し、変更後はRustが返した未完了工程へ移る。単なる画面移動で旧承認を失効しない。
作業種別を切り替えても調整を再開/保存しない。再開後に依存状態が失効したら、古い閲覧位置から未達工程へ飛び越させない。

### 重複を減らす規則
- 個体選択とIDは共通contextに一度、合成/実機未接続は共通toolbarに一度だけ表示する。
- 同じ由来・未検証・次操作をheader、本文、footerで繰り返さない。常設の成功message欄を置かない。
- 現在手順の説明は一つ、主操作は一つ。保存手順に7ch結果表を再掲しない。
- エラーは操作の近傍に原因と復帰操作を一度示す。技術詳細は展開式。数値の未測定や範囲超過は数値に関連づける。
- 破壊的な実機書込み直前の対象・変更内容確認は重複注釈ではなく誤操作防止なので残す。
- 16pxの本文/入力/ラベルを維持し、文字を縮小して詰め込まず、現在不要な面を非表示にして密度を制御する。

### アクセシビリティ
横タブはtablist/tab/tabpanelとroving tabindex、左右矢印/Home/End、Enter/Spaceで選択する。移動だけで機器操作しない。
縦手順はordered listとbuttonによるステップナビゲーション。見た目が似ていても独立した内容タブとして扱わない。
aria-current=stepは閲覧位置、完了/未完了は別の文字状態で示す。後続ロックはaria-disabledと実クリック拒否の両方で制御。
デスクトップの縦手順は常設し、幅に応じた縦積みや折畳みをしない。最小サイズと作業ペイン内のスクロールで処理し、DOM順とキーボード順を一致させる。
非表示panelのinput/操作はフォーカス対象にしない。画面切替/エラー後のfocus位置を試験する。

Tauri+Rust coreを継続し、GUI framework、daemon、DBは追加しない。

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

## デスクトップ用の画面構成
製品名はWindows標準タイトルバーに集約し、作業面にWebサイト型のブランドheaderを重ねない。
標準のウィンドウ枠・移動・最大化・システムメニューを維持し、自作caption buttonやdrag領域は追加しない。
作業面は上端の対象/ID/mode toolbar、横の作業tab、手順と操作paneの順。アプリ情報は要求時だけdialogで表示する。
本文16pxを維持しながら、マウス/keyboard用controlは32px以上へpaddingを縮める。coarse pointerは44px以上を維持する。
stepに固定の310px空白を作らず、非表示commandの案内や空footerは表示しない。開始前の閲覧可能な第1手順は「開始前」と表示する。
keyboard focusは2pxの可視枠を維持。tab/step内側へ置き、headingのfocus枠は文字幅に合わせる。
初期windowは1100×720、最小800×520（論理サイズ）。上部操作・作業タブを固定し、作業領域だけ伸縮/スクロールする。任意のスマートフォン幅への適合は目的とせず、小さいSnap領域には入らない場合がある。画素密度の模擬試験とWindows DPI変更の実機受入は区別する。
Tauri/WebView2を維持するため、WinUI等のOS標準control自体への置換ではない。OS完全一致が要件になる場合は別途frameworkを選定する。

## ローカルスタイル
デジタル庁の指針を基盤に、装置の情報密度へ適合させる。政府ロゴ・認定表示を使わない。
白と中立灰の作業面、細い罫線、控えめな青緑の操作色を基本とする。影付きカードや大見出しのキャッチコピーを並べない。
本文・入力・ラベル・表見出しは16px。数値は桁を揃え、説明と演算の詳細は必要な場所に置く。
対象IDは共通contextに集約し、主操作は閲覧中の手順に一つ。実行許可はRustの結果に従う。
キーボード、可視フォーカス、関連づけた入力エラー、色以外の状態表現、800/1100/1600 CSS pxのデスクトップ配置を試験する。
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

- [デジタル庁: ステップナビゲーション](https://design.digital.go.jp/dads/components/step-navigation/)
- [W3C: Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/)

- [Microsoft: タイトルバー](https://learn.microsoft.com/ja-jp/windows/apps/design/basics/titlebar-design)
- [Microsoft: デスクトップ用のコンパクト密度](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/compact-sizing)

## 受信値の表示規約（preview.4）
読み取り専用の表示層は七行と数値ノードを保持する。符号を含む整数部/小数部/指数を別の固定幅領域とし、値や桁数から列幅を決めない。小数部は表示上2桁、絶対値1e9以上または0より大きく0.005未満は指数表記。表示丸めを元値や校正処理へ返さない。未受信/失効は同じセルの—であり、偽のゼロを入れず表全体を消さない。
数値ペインは22rem、右の表示履歴ペインは残りの幅・高さを使用する。グラフは選択chの直近30秒・最大200表示点であり、取得全件のログではない。入力消失時は履歴も消す。軸自動調整を明示する。
接続先一覧の新しい選択フローとpoll中の操作状態分離は未適用。既存の接続前確認/明示接続/要求直列化は維持する。
