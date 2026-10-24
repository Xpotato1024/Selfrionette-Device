---
status: canonical
owner: host
last_verified: 2026-10-19
canonical_for:
  - quality stream receive observations and bounded sample delivery
related:
  - docs/contracts/firmware-protocol-v3.md
  - docs/contracts/serial-transport.md
  - docs/operations/normalized-input-consumer.md
---

# 品質付き入力の受信・遅延・配信

Issue #25。取得、個人補正、CLI/GUIへの引渡しは共通Rust `LiveService`を利用する。
Firmwareの版から送信レートやADC変換回数を推定しない。取得threadや常駐daemonを追加せず、
既存の有界drainとconsumerへの引渡しを整える。

## 1回の受信とconsumerへの引渡し

`StreamTick`は最大16行、最大20 ms、1回のreadは最大10 msを維持する。
transportは最大256 bytesずつ受信し、部分行と続く行を1024-byteの有界bufferに保持する。
read syscallを1文字ごとに発行しない。最大行長、CR/LF、UTF-8、不正行の拒否は維持する。

各sampleは順番に既存の品質判定と`InputProcessor`を一度だけ通る。`Reply.sample_events`に
その操作で処理した全sampleと補正出力を順番に返す。上限は16件、次の`execute`で空にする。
これは未消費の全履歴を溜めるqueueではない。`Snapshot`と無受信tickは古いsampleを再配信しない。
操作途中でfatal errorが起きた場合、部分的な成功batchを返さず、readinessと出力を失効する。

`SampleEvent`は元session/sequence/device時刻/設定世代/tare epoch/raw/oriented/flags、
同一host monotonic起点の受信microseconds、相対backlog、当該sampleのready/補正出力を保持する。
CLIは全eventを順番に出力する。GUIは`Reply.view`の最新値を描画する。
snapshotの最新値だけを取得し、中間の補正出力も全件consumerへ渡したと報告しない。

## 受信指標

`LiveSnapshot.diagnostics`は次を区別する。

| 指標 | 意味 |
|---|---|
| received_frames / normalized_frames | 受理したProtocol 3 frame / 補正出力が得られたframe |
| receive_frame_hz | host受信時刻によるframe/s |
| observed_source_frame_hz | 元sequence差とdevice時刻差から観測したframe/s。Firmwareの送信試行回数ではない |
| fresh_frame_hz[7] | 各chのflagがfresh単独だったframe/s。ADC変換の実測SPSではない |
| normalized_frame_hz / usable_ratio | 補正frame/s / 全7chの品質と値がusableだったframeの比率 |
| sequence_gaps | 前方sequence差に含まれる欠落数。Firmwareのenqueue前skipはsequenceを進めないため含まれない |
| stream_ticks / last_tick_lines / last_tick_samples | poll回数と直近pollで処理した行/frame数 |
| drain_budget_exhaustions | 行数または時間budgetに達した回数。OS内の未読byte数ではない |
| relative_backlog_ms | 最小観測clock offsetから増えた遅れ |
| transport_latency_ms | 絶対的な輸送遅延。現在は測定できずnull |

rateは直近2秒以内、最大256件の有界記録で計算する。2件未満、時間差なし、
2秒以上更新なし等ではrateをnullにする。rate用の観測区間はstream開始・captureの
pause/resume境界で新しくする。未知値をゼロと表示しない。GUIの表示回数は含めない。

## 時刻と鮮度

device_ms/sequenceは32-bit wrapに対応し、重複・逆行を拒否する。host clockは単調時計とし、
testsでは同じclockをtransport/serviceへ注入できる。

host受信経過とdevice時刻経過の差について、最小観測offsetを基準に追加の滞留を測る。
250 msを超える相対backlogは入力を失効させる。遅れて読んだ古いsampleを、新しいhost受信時刻
だけで現在入力とみなさない。既存の250 ms freshness上限を延長しない。

最初の観測から一様に存在する通信遅延はこの方法では分からない。`sample_age_ms`は
受信後のhost経過時間と相対backlogの和であり、装置からの絶対遅延を保証する値ではない。
別processのconsumerはhost単調時計の原点を共有していると仮定せず、IPCの送出時刻・受信時刻と
確認応答を用いて対応づけ、IPC滞留分も鮮度へ加える。

## ソフトウェア検証

決定的clock/transportで20/40/80 Hz各2秒、41/81/161 frameを生成する。GUI相当の50 ms待ちが
あっても共通serviceから同数の補正出力を得て、既存coreへ同じsample列を直接渡した結果と
一致することを確認する。sequence gapは各0件。別試験で32-frame burstを16件ずつ引き渡し、
不正seqの部分成功拒否、290 ms backlogの失効、clock/sequence wrap、部分ch品質不良、
bounded記録、split/multiple/overlong/不正UTF-8 lineを確認する。

これらはsoftware-onlyの結果である。実ADC RATE、USB/OS遅延、二台のCPU負荷、
実操作者の入力性能、物理的freshレートはIssue #26で測定する。
