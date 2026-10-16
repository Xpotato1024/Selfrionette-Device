---
status: canonical
owner: selfrionettectl
canonical_for:
  - file-only calibration replay operation and JSON format
related:
  - docs/contracts/input-calibration.md
  - docs/operations/hardware-safety.md
---
# 入力適合のオフライン比較

## この段階でできること
記録JSONを読み、7軸の中立・方向・正負操作範囲と応答行列を評価し、方式A/Bの出力と指示意図との差を比較する。
serialポートは開かず、tare、Firmware、EEPROM、ロボットを操作しない。GUIを実機校正済みにする機能ではない。

## 合成例を実行する
リポジトリrootを作業ディレクトリとする。Rustのhost build環境とNodeが必要。
WindowsではC++ Build Toolsの開発者terminalから実行する。NodeだけのGUI previewとは別の検証である。
下記のsample/result JSONは生成物なのでcommitしない。

~~~sh
node examples/input-calibration/generate-synthetic.mjs > synthetic-input.json
cargo run --locked -p selfrionettectl -- calibrate-replay synthetic-input.json > comparison.json
~~~

Windows PowerShell 5.1の > はUTF-16出力になるため、上のredirectはcmd.exeで実行する。
またはNodeからexecFileSyncの結果をfs.writeFileSyncでUTF-8として保存する。
CLIは1つの入力ファイルを読むだけで、出力はstdout。既存ファイルへ自動保存しない。
--port等の実機引数は受け付けない。8 MiB以下の通常ファイルのみ対象とする。

例の係数・ノイズ・閾値・ユーザー情報は全て合成。Selfrionetteの実測値や推奨設定へ転用してはならない。

## JSON schema_version=1
未知field、未指定field、未知schema/domain、7ch以外の配列、非有限値は拒否する。
全fieldは同梱generatorが生成する。次がトップレベルの構成である。

| field | 内容 |
|---|---|
| schema_version | 1 |
| recording_id | その観測記録の識別子 |
| sample_domain | firmware_v2_vectorのみ |
| origin | syntheticまたはrecorded。呼出側の申告であり実機検証済み認定ではない |
| context | device_id / firmware / assembly / epoch / user / posture |
| axes | 7要素。それぞれchannel 0～6とpositive_direction。chの重複不可 |
| limits | fitの品質上限・noise floor・条件数等。全値を明示 |
| signal | time_constant_ms / noise_multiplier / release_ratio / max_deadzone_fraction |
| neutral | 操作姿勢での中立sample配列 |
| trials | 方向を指定した入力前・保持・除荷の繰返し |
| replay | sampleとintent[7]の組。校正終了後に別途記録する |

sampleのfieldは timestamp_ms:u32、epoch:u64、values[7]、quality。
qualityはusable / unknown / reset / timeout / spike / clippedのいずれかで、処理できるのはusableだけ。
trialのfieldは id、axis、level、instruction、neutral、loaded、released。
levelは[-1,1]内の非ゼロ値。正負の範囲が異なる場合にも、同じ値で捏造せず別々の記録を使う。
replayのintentは各成分[-1,1]。最初に中立を含め、単軸だけでなく斜め・複数指・停止も含める。
replayは1～8192 samples。その他の観測数上限と計算定義は契約を参照する。

## 結果の読み方
hardware_verified=false、control_ready=falseであることを最初に確認する。
方式Bのavailable=falseは逆行列の不成立等を意味し、方式Aの結果で埋めたものではない。
command_rmseがnullの場合は未測定方向や候補不成立を確認する。nullは誤差0ではない。
span_exceededは操作範囲超過であり、物理的なADC飽和とは異なる。
neutral_peak_commandは停止後のfilter応答も含む。小さい値だけを求めてfilterを強くしない。

CLIのexit 0はレポート生成成功であり、候補成立・実機校正成功・方式B採用を意味しない。
構造不正、入力品質・時間の不整合等ではexit 1、引数不正ではexit 2。失敗時には途中レポートをstdoutへ出さない。
比較に使ったcontext、操作指示、閾値と合成/実測の区別を維持する。結果だけを実験結果として切り離さない。

## 実機記録へ進む条件
まず既存Firmware出力の由来と取得品質を確認する。通常のv2 vectorだけではfilter前の値や十分なsample品質が得られない。
必要な診断情報がない記録を、単にquality=usableと書き換えて通さない。
無荷重の物理tare、操作姿勢の中立、既知方向、センサーの感度校正、使用者の快適な操作範囲を別々の証拠として扱う。

次の実装境界は品質付き有限captureと、実機での校正シーケンス・GUI操作への接続である。
実機確認では対象個体・配線・方向・port・許可された操作を確定する。EEPROMへの保存、Firmware書込みは別の明示承認を要する。
今回のCLI出力を現在の装置設定recordへ流し込む経路は作らない。
