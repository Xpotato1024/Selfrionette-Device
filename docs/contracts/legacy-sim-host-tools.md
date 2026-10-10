---
status: canonical
owner: architecture
last_verified: 2026-10-10
canonical_for:
  - legacy Sim monitor and measure host behavior
related:
  - docs/contracts/legacy-sim-serial-producer.md
  - docs/contracts/serial-transport.md
  - docs/operations/legacy-sim-host-tools.md
---

# 旧Sim monitor / measureのhost契約

`selfrionettectl legacy-monitor` / `legacy-measure`は[旧Sim 7ch producer](legacy-sim-serial-producer.md)専用である。
v2のidentity確認、tare、provisionとは別経路にする。robotのchannel→XYZ、符号、gain、runtimeはSimが所有する。

移行元はSim `1bda8c6d062214cd424e1b9867665ee5949d255b` の
[monitor](https://github.com/Xpotato1024/Xpotato-Sim/blob/1bda8c6d062214cd424e1b9867665ee5949d255b/scripts/hardware/selfrionette/monitor_selfrionette_serial.ps1)と
[measure](https://github.com/Xpotato1024/Xpotato-Sim/blob/1bda8c6d062214cd424e1b9867665ee5949d255b/scripts/hardware/selfrionette/measure_loadcell_channel_response.ps1)である。
引数/default・表示状態・集計の正本はDevice Rustに移す。SimはOS/argv互換転送だけを持つ。

## 共通条件

- `--port`は必須。旧COM5自動openは明示承認により廃止する。列挙・auto-open・再接続はしない。
- baudは115200、正のint32で明示変更可能。旧8N1、DTR/RTS=true、open後1秒の待機を保持する。
- lineはUTF-8、上限1024 bytes、CRを除きLFで確定。20ms以内のread sliceとpartial line保持で停止操作を回収する。
- stdoutへ逐次表示し、sample履歴を蓄積せず7chの合計と件数で平均を求める。
- 正常終了0、引数不正2、I/O failure 1、Ctrl+C 130。全経路でserial handleをdropし、raw consoleを復旧する。
- 初期待機中もCtrl+Cを処理し、検出後にCalibrate/SendTextを送信しない。
- `--powershell-args`は旧parameter名のcase/prefix、colon値、boolean switch、switchを除くpositionをDevice側で解釈する。
  空文字・quoted leading-dash operandを保持し、重複・曖昧名・不足値を拒否する。

## monitor

defaultはduration 0（無期限）、SendText空、Calibrate=false、DisplayLevel=vector、PausedDisplayLevel=status。
durationが正なら指定秒数、0以下なら無期限という旧挙動を保持する。

表示levelはstatus < warn < vectorの包含filterであり、未知prefixは旧実装同様statusとして表示する。
p/r/c/q（大文字も可）はpause/resume/旧`c`送信/quit。初期CalibrateはSendTextに優先し、表示をpauseする。
key `c`は旧実装同様送信のみで、自動pauseを追加しない。
`status,calibration_start/end`の通知とend時のresumeを保持する。

`[calibration complete]`は旧calibration round終了の表示であり、全channelの校正成功保証ではない。
旧producerはskipped/timeout/spread warningを出してもendを出し得る。paused filterでwarningが隠れる条件も
旧互換であるため、operatorは必要なwarning表示条件で確認する。これをv2 tare成功や永続設定成功の証拠にしない。

## measure

defaultはBaselineSeconds=3、PressSeconds=4、Sensor=1、Repeats=1、AllSensors=false。
Enterで無負荷baseline、各sensorを押すpressの開始を確認する。Sensorは1..7、表示channelは0..6。
AllSensorsは反復に優先し1..7を一巡する。反復が1以下なら単発という旧挙動を保持する。

exactly 9 fieldsのvectorのみを収集し、signed int64 timestampと7 finite floatを要求する。
不正vectorは収集せず診断表示する。status/warnはsensor recordにしない。
baseline/press秒数が0以下なら空windowとなる旧挙動を保持する。
件数0の平均0は旧互換summary値として表示し、件数0の診断を出す。測定sampleや有効なsensor値を捏造しない。

7chのbaseline平均、press平均、差分、差分絶対値を小数2桁・ties-to-evenで表示する。
abs deltaの降順で上位3chと最強channelを示し、同値はchannel順を保持する。
反復/sweepではSensor/Channel/AbsDelta/Deltaの最終表を出す。表の余白はOS固有Format-Tableへ依存しない。
この表だけでphysical sensor対応やrobot mappingを確定しない。

## software検証と採用順

fake transport/clock/inputでfilter、key、校正送信順、非ゼロ平均/差分、rounding、反復/sweep、
Enter、timeout、EOF、例外、Ctrl+Cとdropを検証する。CIはWindows/LinuxでRust format/clippy/testsを実行する。
software検証はDTR/RTSの実機挙動、校正成功、実荷重応答の受入を代替しない。

Device変更を先に採用し、その採用済みCLIが利用可能になってからSim互換wrapper変更を採用する。
DeviceのDraft branch binaryでsoftware検証しても、Device採用完了とは認定しない。
