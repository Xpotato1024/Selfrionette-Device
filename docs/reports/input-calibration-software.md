# 7軸入力適合: software-only検証記録

## 対象と到達範囲
main 3435764を起点に、pure Rust coreの入力適合、file-only CLI、合成入力generatorを追加した。
既存のFirmware・通常protocol・GUI操作・EEPROM journal・合成preview保存形式は変更していない。
実機のphysical calibration、品質付きcapture、GUIでの実機校正、利用先への適用は未実施。

## ローカル検証
Windows x64 / MSVC、Rust stable cargo 1.94.1、Node v24.14.0で実行した。
全コマンドexit 0。ハードウェアI/Oは行っていない。

~~~text
cargo +stable fmt --check
cargo +stable clippy --locked --workspace --all-targets -- -D warnings
cargo +stable test --locked --workspace
git diff --check
~~~

既存77件に、新しいcore 17件とCLI 7件を加えた**101件がPASS**。skip/ignoreは0件。
正負span、明示ch対応、同時入力、noise、中立、filter時刻、停止応答、未測定逆方向、特異行列、条件数、noise増幅、
異常品質、epoch・利用者違い、重複/逆行時刻、u32 wrap、記録schema・容量、出力前の失敗を検証した。
新しいRust coreの外部依存は0件。JSON処理はCLI側で既存GUIと同じserde/serde_json版を利用する。
既存依存の版とWindows backend選択を維持し、追加したJSON依存のみlockへ反映した。

## 合成generatorからCLIまでの実行
同梱generatorの出力をUTF-8でファイルへ保存し、ビルドしたselfrionettectl calibrate-replayで解析した。
28校正trials、320 replay samples、うち128 samplesが複数軸同時入力。全て合成であり実機値ではない。
時定数40 msを指定した例で、指示の急変・除荷を含めたcommand RMSEは次の通りだった。

| 方式 | command RMSE | 最大絶対誤差 | 中立ラベル区間のピーク |
|---|---:|---:|---:|
| A: ch・極性・方向別span | 0.0690016104 | 0.5728070324 | 0.5450459318 |
| B: 応答行列補償＋方向別span | 0.0565960870 | 0.5456832459 | 0.5450459318 |

この例は線形混合を人工的に与えた機能検証であり、Selfrionetteの実機性能改善を証明する実験ではない。
中立ラベル直後の残留値はfilterの過渡応答も含む。安静noiseが小さいことだけで停止応答を合格にしない。
hardware_verified=false / control_ready=false、fit閾値・trial指示・元値・中間値・未対応状態が結果に保持されることを確認した。
生成した記録/結果はbuild output配下に置き、commit対象にしない。

## 未実施と後続条件
GUIの実機校正への接続、取得品質の実測検証、物理的な方向・ch対応の確定、ADC→N校正、EEPROMへの保存、Firmware書込み、ロボット操作は行っていない。
別の操作者・姿勢・セッションでの再現性、実機の遅延・停止応答、操作方向の比率、方式Bの有用性は別途実測で評価する。
CIの結果はPRに付く実行結果を正とし、このローカル検証記録から推測しない。
