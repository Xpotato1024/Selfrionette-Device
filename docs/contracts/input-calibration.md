---
status: canonical
owner: selfrionette-core
canonical_for:
  - offline operator-adaptation fitting and signal processing
related:
  - docs/contracts/calibration-preview.md
  - docs/contracts/device-host-boundary.md
  - docs/contracts/device-persistent-settings.md
  - docs/operations/input-calibration-replay.md
---
# 7軸入力適合のオフライン契約

## 目的と境界
操作者が指示された方向・強さを入力した記録から、無次元入力の候補を作り、別の時系列で単純補正と干渉補償を比較する。
対象は firmware_v2_vector の記録だけである。元値は既に一律negation、RAM tare、スパイク抑制を経ている。
raw ADC、N単位の力、校正済みの物理量とは呼ばない。Firmwareのtare offsetをもう一度引かない。

これは**使用者・姿勢・セッションに依存する操作適合**であり、装置固有の物理校正やEEPROM設定ではない。
機械的干渉と人間の指の連動を記録だけから分離したとは主張しない。推定対象は指示された操作への応答である。
7chの番号を保持し、軸との対応と正方向の説明は明示入力する。親指等の物理位置や配線を推測しない。

物理校正・装置設定は既存契約、ロボットのXYZ/joint/速度gain/world・tool・task座標系は利用先が所有する。
管理GUIを必須の中継processにしない。新しい処理はpure Rust coreとファイル専用CLIから利用できるが、現行GUIの実機入力、Firmware、EEPROM、Simには適用しない。
既存の合成preview/保存schemaも変更しない。hardware_verifiedとcontrol_readyは常にfalse。

## 入力証拠と有限性
個体ID、Firmware、組立revision、利用者、姿勢、session epochをContextとして固定する。
各sampleはu32 timestamp_ms、同じepoch、7つの有限値、取得品質を持つ。品質usableは呼出側が根拠を持って与える注記であり、物理校正成功を意味しない。
単なるvector受信やwarnが見えないことからusableを生成してはならない。Unknown/Reset/Timeout/Spike/Clippedは拒否する。
取得失敗で保持された前値を、雑音の小さい正常sampleとして学習しない。

- 自然な操作姿勢での中立区間を最初に取得する。装置から指を離した無荷重tareとは別である。
- 1区間8～1024 samples、全校正記録16384 samples以下、112 trials以下。
- 各軸・各方向は2～8 trials。正方向は全軸必須。逆方向は未測定を許すが、1回だけの測定では採用しない。
- trialごとにID、対象軸、符号付き指示強度level、操作指示、入力前・保持・除荷後の区間を持つ。
- levelは[-1,1]の非ゼロ値。1は無理なく再現する操作範囲の基準であり、最大筋力でも既知の物理荷重でもない。
- 各capture内の間隔上限を明示する。区間間の指示待ちは許すが、全体の時刻逆行・重複は拒否する。
- u32のwrapは差分が正の半周期未満の場合だけ前進とする。epoch変更をwrapとして扱わない。

## 中立・応答の推定
中立の各ch中央値をbaselineとする。中立区間の値幅、前後のbaselineずれ、除荷戻りを明示した上限と比較する。
noise envelopeは全中立区間についてbaselineからの絶対偏差の最大値と、指定したnoise floorの大きい方である。
運転中の入力からbaselineを自動更新しない。持続した意図入力をゼロ点へ吸収しないためである。

trialの応答は保持中央値から入力前中央値を引き、指示levelで割る。対象chがnoise envelopeに対して小さすぎれば拒否する。
各方向の繰返し中央値からspanを得る。極性の不一致、保持中の大きな変動、繰返し応答の不一致は拒否する。
正負spanは独立に測定し、対称性を仮定しない。未測定の逆方向spanはNone/nullのまま残す。
閾値は全て明示設定し、実機推奨値のdefaultは設けない。

## 方式A: 単純補正
明示した軸→ch対応、極性、方向別spanを用いる。ch間の混合を解かずに入力を解釈する。
無次元化前の軸成分qは、対応chのbaseline差分に極性を掛けた値である。

## 方式B: 実測応答による干渉補償候補
正方向の応答を主chのspanで割り、各軸を列・元chを行とする7×7行列Aを作る。
逆方向も測定されている場合、独立のspanで正規化した方向が一致することを確認する。
方向が一致しない場合、方式Aの候補は残すが、方式Bは方向依存として拒否する。

~~~text
firmware_difference = A * q
q_A = explicit_channel_selection_and_polarity(firmware_difference)
q_B = inverse(A) * firmware_difference
normalized_i = q_i / positive_span_i    (q_i >= 0)
normalized_i = q_i / negative_span_i    (q_i < 0、測定済みの場合のみ)
~~~

固定7×7・部分pivot付き逆行列と残差検証を使用する。特異行列は拒否し、擬似逆で失われた自由度を補った扱いにしない。
条件数は無限大ノルム ||A||∞ ||A^-1||∞、noise gainは ||A^-1||∞。それぞれの指定上限を検証する。
条件が悪い場合は理由と候補不成立を返す。方式Bが要求されたときに方式Aへ黙ってfallbackしない。
逆行列が計算できることは、人が7軸を独立に操作できることの証明ではない。

## 共通の信号処理
推定したqに、実際のtimestamp差分を使う因果的な一次low-passを適用する。
alpha = 1 - exp(-dt/tau)。tau=0は明示的な無効化。小さいdt/tauではexp_m1を使う。
状態を更新する前にcontext・epoch・品質・時間・有限性を確認する。
最初の処理sampleは校正終了より後の新しい中立観測でなければならない。

各軸のactivation閾値onは、変換行列の絶対係数で伝播したnoise envelope × 指定倍率。
release閾値offはon × release_ratio。|q|>onで有効、|q|<=offで解除する。
範囲に対してdeadzoneが大きすぎる設定は拒否する。未知の逆方向を対称spanで埋めない。

有効時の出力は sign(q) * (|q|-off)/(span-off)。出力のみ[-1,1]へ制限し、制限前値とspan超過を別に残す。
非有効時の中立判定0は未測定逆方向のgainが分かったことを意味しない。未測定方向への有効入力はNone/nullとなる。
ヒステリシスの開始時には(on-off)/(span-off)相当の小さな不連続がある。軸別deadzoneは斜め方向の比率を変え得るため、実機の操作感と多軸入力で評価する。

遅延、停止までの応答、保持の安定性はfilter設定によって変わる。noiseの最小化だけを成功条件にしない。
通常入力からsign/span/matrix/閾値を自動学習・変更しない。quality、時間、context等のエラーは無効状態を保持し、前値や偽の0sampleを返さない。
再開には呼出側の明示操作が必要で、epoch等が変わった場合は新しい基準と証拠でfitし直す。

## 比較出力
同じheld-out replayをA/Bへ渡す。校正区間のtimestampの再利用は拒否する。
「held-out」は取得時系列を分けることの検証であり、コピーして時刻を書き換えたデータや利用者の申告の真偽までは証明しない。
入力意図のラベルは校正者が与えた代理正解である。思考や意図を直接測定したと主張しない。

全sampleのcommand RMSE、最大絶対誤差、中立ラベル区間の出力ピーク、評価数、同時入力数、未対応数、span超過数を出す。
未測定方向のsampleが1つでもあれば全体RMSE等をnullにし、都合のよいsampleだけで評価しない。
品質不明・時刻異常等はレポート生成自体を失敗させる。方式不成立だけはavailable=falseと理由を返す。
補正前、baseline差分、推定軸値、filter後、無次元値、制限前後の出力をtraceとして分離する。
fit閾値とtrialごとのID・指示・強度・各区間sample数も出力し、条件の由来を保持する。元の記録ファイルも併せて保存する。
方式の自動採用、プロファイルのinstall、GUIへの自動適用は行わない。

## 参照
- [Rust f64::exp_m1](https://doc.rust-lang.org/std/primitive.f64.html#method.exp_m1)
- [Serdeの未知field拒否](https://serde.rs/container-attrs.html)
