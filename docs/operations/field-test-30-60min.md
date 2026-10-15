---
status: canonical
owner: selfrionettectl
canonical_for:
  - 30-to-60-minute passive field observation and field-test CLI
related:
  - docs/operations/hardware-safety.md
  - docs/contracts/input-calibration.md
---
# Selfrionette実機試験: 30分必須・60分上限

初回実機観測計画（2026年10月15日時点）。1人・1台・1姿勢を基本とし、実施日時は記録ログの開始時刻で識別する。

## 成果と範囲
必須成果は、無荷重/操作姿勢の中立、7つの操作方向の繰返し、弱い入力、同時入力、除荷を、7ch全値と診断通知付きで記録すること。
物理校正完了・7軸独立操作の証明・操作者の意図推定精度・実機制御の許可を、この1回で認定しない。
ADC→N校正、Firmware更新、EEPROM書込み、ロボット駆動、当日のGUI実装は範囲外。

新しいfield-testは観測ログの取得用。現行vectorでは取得品質が十分分からないためquality=unknownを維持する。
このログをそのままcalibrate-replayへ渡せる校正用JSONにしたり、usableへ書き換えたりはしない。
受信テキストを保存するが、生ADCではなくFirmware処理済み出力である。装置固有の物理補正とユーザー依存の操作適合は別に扱う。

## Laptopでの試験キット入手・起動（Windows 11）

1. GitHubのSelfrionette-Device PR #16（https://github.com/Xpotato1024/Selfrionette-Device/pull/16）を開く。
2. ChecksからHost Rustの実行画面を開き、Actionsの実行サマリー最下部のArtifactsから
   selfrionette-field-test-win-x64-<SHA> をダウンロードする（GitHubログインが必要）。
   成果物は30日で期限切れ。アプリ本体のRelease installerではなく、このCLI専用キットを使う。
3. ZIPをLaptopの新しいフォルダへ展開。selfrionettectl.exe、field-test.cmd、field-test.ps1、setup-plan.ps1、
   test-plan.json、BUILD.json、README.mdの7ファイルが同じ場所にあることを確認。
4. BUILD.jsonのsource_commitがPRのHEADと一致することを確認。起動wrapperはexe SHA-256を検証する。
5. `field-test.cmd`を起動。未設定ならCOMポート列挙より前に`setup-plan.ps1`が起動する。
   **最初の`Quick initial observation? [Y/n]`でEnter**を押すと、1+3+3の仮ラベルで7軸分を自動設定する。
   続く保存確認で`y`を入力する。装置名や7軸の説明を手入力する必要はない。
   この簡易モードは**初回の探索的観測だけ**に用いる。物理的な正方向・実際の指・配線対応・利用者・姿勢は未確認として記録され、校正済みデータとしない。
   正式な操作意図・方向・繰返しの校正実験では、最初の質問に`n`を入力して詳細情報を定義する。
   `:cancel`で中断すると元の計画は変更せず、保存時はバックアップを残す。
6. 計画を確認・保存するとpreviewでconfigured: trueを再確認する。ここまではserialを開かない。
   再実行時は設定済みなら質問を省略する。別途PowerShellで .\selfrionettectl.exe field-test preview .\test-plan.json も利用可能。
7. ほかのserial利用アプリを閉じ、現物のCOM番号を確認してOPEN COMxと明示入力した場合だけ記録を開始する。
8. 終了後は生成されたcapture-日時フォルダをまるごと保管する。中断記録も破棄しない。

配布キットはソースに追跡できる試験用の未署名exeである。OSの保護機能を無条件に解除せず、
ダウンロード元、ソースコミット、BUILD.jsonのSHA-256を確認する。
Portable CLIなのでLaptopへのRust、Node、Visual Studio導入やインストーラ実行は不要。

## 前日までの準備（当日の30分枠に開発環境構築を持ち込まない）
Windows x64用のビルド済みselfrionettectl.exe、test-plan.json、field-test.ps1、field-test.cmd、BUILD.jsonを同じフォルダに置く。
LLM-01以外で行う場合はフォルダごと試験PCへコピーする。実行時にはRust/Node/開発環境を必要としない。
実機を接続しない状態でfield-test previewを実行し、手順と計画が読めることを確認する。

計画JSONのdevice_labelは紙ラベルA等、operatorは試験者、postureは手・前腕・台座の姿勢、boardは現物/既知仕様、firmware_sourceは書込み履歴（不明ならunknown）を記録する。
axes[0..6]は**操作軸のラベルであってch0..6ではない**。1軸指、3軸指A、3軸指Bの1+3+3を具体的な指・方向へ置き換える。
positiveには本人が再現できる具体的な動作を書く。例の「手前/奥/上」は指や装置の姿勢も含めて定義する。
negativeは安全に逆方向へ入力できる場合だけ動作を記入する。入力不能・未確認はnullを維持し、引っ張るために無理な力を掛けない。
combined_instructionには異なる2方向/2指を同時に操作する動作を1つ定義する。弱めの同時入力を優先する。
実際の装置と手の位置、紙ラベル、向きが分かる写真を1枚以上用意し、notes.txtへファイル名を残す。

空欄のtemplateはpreview可能だがcaptureは拒否する。起動wrapperは未設定を事前に検出し、COM選択前に**簡易観測モード（Enter）または詳細定義モード（n）**へ誘導する。
JSONはUTF-8で保存する。根拠のないch割当や左右IDは埋めない。

## 当日30分の必須工程
| 経過時間 | 作業 | 終了条件 |
|---|---|---|
| 0–5分 | 1台だけ接続。台座・配線・荷重方向と計画を照合。ロボットを切り離す。GUI/Arduino Serial Monitor/Simを終了。 | 装置、指、方向、portが一致し、記録先が新しい。 |
| 5–20分 | wrapperで対象portを確認して開始。画面の指示どおり中立・単軸・弱入力・同時入力・除荷を記録。 | 9分49秒〜13分33秒の連続記録が終了、または異常で停止。 |
| 20–25分 | review.mdで中立差、方向別の7ch応答、2回の符号/大きさを確認。summary.jsonの警告・重複・時間差を見る。 | 基礎応答確認/要再試験/接続未成立のいずれかを記録。 |
| 25–30分 | 指示に遅れた区間、姿勢変更、痛みや疲労、未測定方向、写真名をnotes.txtへ記載。BUILD.jsonとフォルダ全体を保存。 | 証拠の保存を確認して終了。 |

30分で終了する場合、全項目が理想通りである必要はない。問題軸や取得系の不足を特定できれば初回試験の成果とする。
接続で5分消費したら新機能開発・ドライバー再構築・Firmware更新へ進まず、接続診断と残る観測だけで打ち切る。

## 自動記録の内訳
初期45秒は無荷重で接続時の起動・ゼロ点処理を観測。その後、無荷重30秒、自然な操作姿勢の中立30秒を記録する。
各方向の1試行は、予告3秒→入力前中立3秒→穏やかな立上げ2秒→保持4秒→除荷4秒、計16秒。
各操作軸について正方向2回、設定した軸だけ逆方向2回。その軸の後に10秒休む。
基準強度は「無理なく再現できる快適な操作強度」であり、最大筋力・既知荷重ではない。

校正候補用の観測とは別に、7操作軸の弱い正入力を各1回（順序を逆にする）、定義した同時入力を3回、最終中立30秒を記録する。
弱入力の「半分」は本人の主観指示であり、正確な0.5という教師値・既知の力にはしない。

正方向のみは589秒（9分49秒）、全軸で逆方向も実施すると813秒（13分33秒）。安全に測れる逆方向の数でその間になる。
逆方向を省略しても正方向の単軸・弱入力・同時入力を削らない。
画面の指示と任意の端末ベルを使う。ベルが鳴らない環境でも画面で実施できる。自動で一時停止/軸スキップはしない。
取り違え・遅れは当該区間のラベルをnotes.txtに記録する。続行困難ならCtrl+Cで終了し、別名で新しいsessionを作る。

## 30〜60分へ延長する場合
| 経過時間 | 作業 |
|---|---|
| 30–35分 | 追加目的を1つ選ぶ。同じ装置の再装着再現性、または2台目の基礎確認。両方は行わない。 |
| 35–50分 | 同じ手順で1回だけ再記録。装置/姿勢が違う場合は別plan・別sessionへ保存。 |
| 50–55分 | 中立・符号・副次ch応答の再現性を比較。都合のよい試行だけを残さない。 |
| 55–60分 | 中断分も含めたフォルダ、写真、BUILD.json、メモを保存して終了。 |

二台の同時USB試験、反力やロボット操作、係数の実機書込みには広げない。両手/両機の役割はCOM順で決めない。

## 確認項目と判定
1. 取得: 7値があるか。実際の受信頻度、警告、重複timestamp、device時刻差を見る。約80Hzは参考情報であり、未知Firmwareに一律の合格値を押し付けない。
2. 中立: 指を離した無荷重と指を添えた操作姿勢を分ける。初期/最終の平均・標準偏差を比較する。
3. 方向: 本人が定義した同じ方向で2回とも同符号の応答になるか。正方向でセンサ値が負になること自体は故障ではない。
4. 連動: 他chも変わるかを全7chで確認する。countの感度が未校正なので、数値の比だけを物理的な軸間干渉率にしない。
5. 除荷と弱入力: 力を抜いた区間に持続的な偏りが残らないか。弱い入力が中立の変動に埋もれないか。同時入力が再現されるか。

診断上の200 ms差カウンターは問題を探す目印であり、品質を認定する閾値ではない。
中立変動と入力差の比は各ch内で比較する。後から都合のよい閾値を選んで「校正合格」にしない。
合成試験のRMSEやnoise閾値を実機合格基準へ転用しない。

## 即時中止・再開
痛み、異常な変形、取付けの緩み、発熱、荷重経路の不明確さがあれば直ちに力を抜き、中止する。
初回のvectorが30秒程度で出ない、取得中のreadが1秒timeout、device時刻の後退、基準変更通知、非有限/不正vector、metadata変化等はCLIが停止する。
警告は記録して集計するが、警告がないことを取得品質正常と認定しない。異常が繰返す場合はoperatorも中止する。
Ctrl+Cは標準の中止手段。強制kill/電源断/ディスク障害ではsummaryが残らない場合があるため、events.jsonlも保持する。
再接続・再装着・tare後は別session。既存記録を上書きしない。場当たり的なc/tare連打はしない。

## CLIと成果物
field-test template NEW.jsonは未設定計画を生成、field-test preview PLAN.jsonは手順表示のみでportを開かない。
field-test capture PLAN.json --port COM番号 --out 新規フォルダ --yes-openで、operatorが明示した1portだけを開く。
OS列挙との照合、計画、出力先の検証後に開く。自動接続・再接続、info/tare/c/provision等の送信、EEPROM、Firmware書込みは行わない。
ただしserial openのDTR等で装置が再起動する可能性はある。接続前に指を離しておく。
既存のRust protocol parserとserial transportを再利用し、別言語にparserや校正logicを複製しない。

保存ファイル:
- events.jsonl: 計画、port、開始時刻、画面指示、受信行、7ch、診断、終了/中断。受信行はparse前にも保存。
- summary.json: 各区間の標本数、平均、標準偏差、最小/最大、受信頻度、警告/時刻異常数、判明した場合のdevice情報。
- review.md: 中立と各保持区間の応答を人が確認する表。
- notes.txt: 写真名、実際の指/方向、指示とのずれ、問題、再試験候補。
- BUILD.json: 使用したexeのhash/source revision。wrapperがsessionへコピーする。

通常vectorの品質はunknown。stable IDやFirmware情報が受信されなければnullのまま。紙ラベル/COMをstable IDへ変換しない。
時刻はhost受信・指示時刻であり、人間の正確な入力開始やend-to-end latencyを測定した値ではない。
ログ64MiB・100000受信行・固定scheduleの有限性を守る。全sampleのメモリ蓄積はせず、区間統計だけを保持する。
通常の中断では既存ログと中断理由を保存する。I/O失敗時は成功表示をしない。

## ソフトウェア検証
ハードウェアを使わず、新規11件を含むworkspace 112件、Clippy、formatを検証した。
実機でのport open、無送信動作、USB再起動、当該装置のセンサ品質の受入は翌日のoperator実施対象である。
この文書の時間は試験枠とスクリプトの予定長であり、正常完了を保証するものではない。

参照: serialportのport open/DTR仕様 https://docs.rs/serialport/latest/serialport/struct.SerialPortBuilder.html
