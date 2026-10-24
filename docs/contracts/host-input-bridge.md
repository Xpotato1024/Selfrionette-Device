---
status: canonical
owner: host
last_verified: 2026-10-19
canonical_for:
  - versioned local normalized-input bridge
related:
  - docs/operations/normalized-input-consumer.md
  - docs/contracts/quality-input-observability.md
  - docs/architecture/required-setup-calibration.md
---

# 正規化入力のローカルbridge

`selfrionettectl bridge`は、保存済みsetup/profileを持つ**明示された一台**を、同じコンピューターの
consumer processへ渡す。stdin/stdoutのversioned JSON Linesを使い、serial所有、品質、tare、
個人neutral、profile再検証、補正は共通`LiveService`が実行する。
[Xpotato-Sim #631](https://github.com/Xpotato1024/Xpotato-Sim/issues/631)のInput Source adapterとの境界であり、
常駐daemon、別の校正stack、robot mappingや速度gainを追加しない。

## 所有と準備

一つのbridge processが一つの明示portとstable device IDを所有する。左右roleはconsumerがIDに束縛する。
二台を扱うconsumerは二つのprocessを起動し、片方の欠測を他方やneutralで埋めない。
接続前にManagerのserial handleを解放し、handoffを明示確認する。

process起動、`configure`、`status`はserialを開かない。
`connect`、無荷重`tare`、自然な使用姿勢での`neutral`、`apply`は別々の明示確認である。
確認を含む一括自動準備commandはない。GUI側の姿勢指示を読んだ人が各操作を実行する。
provisioning、device settings、Firmware書込み、profile作成・更新はこのbridge APIに含まれない。
neutral取得の証跡は既存serviceの有界capture archiveへ保存する。

profileは元の`<store>/<stable-device-id>/<record>.srncal`配置を維持し、絶対pathで指定する。
そのstoreには現在の個体setup recordも必要となる。profileを単独で移動しない。
profileの再計算・held-out再検証・ID/firmware/source/setup/operator/methodの照合はserviceに委譲する。
恒久profileの内容と今回のvolatile tare/neutral/readyを区別する。

## build provenance

binaryにはbuild時の`SELFRIONETTE_BRIDGE_REVISION`を埋め込む。値は正確なsource commitの
40桁小文字hexとし、clean checkoutから生成する。通常の開発buildで未指定なら`unknown`を返し、
`connect`の前に拒否する。実行時の`--revision`上書きはない。

```sh
SELFRIONETTE_BRIDGE_REVISION="$(git rev-parse HEAD)" cargo build --locked --release -p selfrionettectl
```

consumerは信頼するbuildのexact revisionをprofile設定とともに固定し、起動status以降の全eventで
一致を確認する。build変数はsource追跡の情報であり、署名検証や実機受入の証拠ではない。
bridge revisionとdevice Firmwareの`source_sha`は別のrevisionである。

## Command v1

各行のschemaは`selfrionette-bridge-command/v1`。
`request_id`は1から`2^53-1`までの整数で、process内で単調増加させる。
未知field、重複field、不正型、未知action、重複・逆行request IDは拒否して終了する。
consumerは一度に一つの要求だけを未応答にする。

共通fieldは`schema`、`request_id`、`action`。各actionの追加fieldと条件は次の通り。

| action | 追加field | 条件と結果 |
|---|---|---|
| `configure` | `config` | 切断中のみ。明示port/ID/profile path/context/methodを検証し`configured`へ |
| `connect` | `confirmed: true` | `configured`/`stopped`から明示portを開く。safe `?3` probeとstable IDを確認し`connected`へ |
| `tare` | `confirmed: true` | `connected`/`awaiting_neutral`で新接続challengeを確立し、無荷重tareを行う。profile再検証後、producerを止めて`awaiting_neutral`へ |
| `neutral` | `confirmed: true` | `awaiting_neutral`で取得barrier後の16 fresh sampleを取得し`capturing_neutral`へ。完了時に`neutral_ready` statusを送る |
| `apply` | `confirmed: true` | `neutral_ready`で正確なmethodのprofileを適用し`applied`へ |
| `start` | なし | `applied`で最初のfresh補正出力を確認し`streaming`へ。以降の受信frameを公開する |
| `stop` / `disconnect` | なし | stream停止、volatile状態破棄、serial handle解放後に`stopped`へ |
| `status` | なし | serial I/Oを行わず現在のstateを返す |

`confirmed`の省略はfalse。文字列`"true"`やnullを真として扱わない。
`config`は`port`、`expected_device_id`、`profile_path`、`operator`、`posture`、`grip`、
`method`（`diagonal`または`coupled`）を持つ。
[configureの完全な例](../../examples/host-bridge/configure-v1.json)は合成例であり実機操作を許可する設定ではない。

phaseは`unconfigured`、`configured`、`connected`、`awaiting_neutral`、`capturing_neutral`、
`neutral_ready`、`applied`、`streaming`、`stopped`、`failed`。
`apply`直後はprocessorがまだ最初の入力を検証していないため、`ready: false`になり得る。
consumerは`applied`の間、最大10 Hz程度の`status`で`ready: true`を待ってからStartを許可する。
neutral完了後も同じchallengeのstreamを継続して排出し、freshnessを保つ。
Startでchallengeを作り直すとneutralが失効するため、Startは公開開始だけを行う。
Stop後は接続・tare・neutral・applyをやり直し、過去の準備を再利用しない。

## Event v1

全行のschemaは`selfrionette-bridge-event/v1`。
共通fieldは`event`、`schema`、`bridge_revision`、`host_clock`、`host_emitted_us`。
`host_clock`は`bridge_process_elapsed_us`で、全eventのemission時刻は**同じwriterの書込み直前**に付ける。
単位は整数microseconds。同じmicrosecondのeventは等しい時刻を持てる。

### Status / error

`event: "status"`は`request_id`、`phase`、`ready`、`session_neutral_valid`、`source`、`profile`、
`operator`、`diagnostics`を持つ。起動statusのrequest IDは0。要求へのstatusは要求IDをechoする。
neutral取得完了の自発statusも元neutral要求のIDをechoし、新しい入力sampleには数えない。
source/profile/operatorはsampleと同じ型で、まだ利用できないものはnull。
readyはApplied/Streamingでfreshな補正出力が存在する場合だけtrueとなる。
[statusの例](../../examples/host-bridge/status-v1.json)を参照する。

`event: "error"`は最後に受理した`request_id`、最大2048 UTF-8 bytesの`reason`、
`ready: false`、`fatal: true`を持つ。consumerは即時に入力を失効させる。
JSONが壊れている場合、要求IDを取り出せるとは限らない。

### Sample

`event: "sample"`は一つの新規v3受信frameと、そのframeに対して共通processorが計算した出力を表す。
[完全な合成fixture](../../examples/host-bridge/sample-event-v1.json)をconsumer双方の型確認に使用する。

| field | 意味 |
|---|---|
| `source` | `device_id`、`firmware_version`、`source_sha`、`protocol: 3` |
| `profile` | `record` basename、`revision`、`assembly_revision`、`config_revision`、検証した`method` |
| `operator` | 明示された`operator`、`posture`、`grip` |
| `domain` | `normalized_input`。無次元でありNやrobot速度ではない |
| `session` / `sequence` / `device_ms` | 元device challenge、frame sequence、device millisecond時刻 |
| `settings_generation` / `tare_epoch` | 元frameの設定世代とtare epoch |
| `quality_flags` | 元frameの7ch flags。未取得flagやepochをneutralの証拠へ変換しない |
| `host_received_us` | 元frameをserviceが受理したbridge単調時刻 |
| `host_emitted_us` / `sample_age_ms` | writer直前時刻と、受理後経過＋相対backlogを切り上げたage |
| `relative_backlog_ms` | device相対時刻とhost受信の変化から観測した滞留。絶対USB latencyではない |
| `normalized` / `command` | 個人profileで補正した7要素のnumber/null配列。`normalized`はclamp前、`command`はDevice側deadzone/filter/range制限後 |
| `span_exceeded` / `unsupported_direction` / `active` | 対応する補正出力の7要素boolean配列。出力失効時はnull |
| `ready` / `missing` | 現在の出力可否。失効をゼロ入力へ置き換えない |
| `hardware_acceptance` | このsoftware bridgeではfalse。受信成功から物理受入を推定しない |

空poll、status、consumerのcontrol tickからsampleを再生成しない。
consumerが同じsampleを複数control tickで参照するときは、元identityと元receipt時刻を保ってageを増やす。
IPC writer到達時に250 msを超えた出力はready=false/missing=trueとし、normalized/commandを
`[null, null, null, null, null, null, null]`、補正診断配列をnullへ変更する。
device品質・epoch・順序・stream freshnessの異常はfatal errorと切断へ進み、部分成功を再利用しない。

## Process間の時刻対応

Python等のconsumerとRustのprocess開始時刻は異なる。
受信時刻から単純に最小offsetを作る方法は、最初の固定IPC滞留を新鮮な入力と誤認するため使わない。

configure後の即時`status`要求について、consumerの送信単調時刻を`L_send`、
対応statusのwriter時刻を`B_emit`とし、同じmicrosecond単位で`C = L_send - B_emit`を固定する。
そのstatusは要求処理後にしか出力されないので、`C + B_received`は実際のreceiptを過去側へ見積もる。
consumer時刻`L_now`でのageは、`max(0, L_now - (C + B_received)) / 1000 + relative_backlog_ms`と
eventの`sample_age_ms`の双方を下回らない値とする。
ms切り上げの端数で推定した取得時刻は過去側へ揺れる場合がある。
順序検査には元sequence/device時刻/`host_received_us`を保持して使い、
保守的なageから逆算した時刻を元receiptの順序として扱わない。
consumerは要求と返信の対応と有界RTTを確認する。初期受入基準は200 ms以下とし、超過時は利用しない。
tareやneutralの長い要求で時計を再校正しない。processを作り直したら対応を破棄する。

この対応はローカルIPCで隠れる遅延を保守側へ扱うためのもの。
ADC取得から最初のhost受信までの絶対遅延は測定していない。
受信frame/s、fresh flag frame/s、ADC conversion/s、相対backlogと絶対latencyの区別は
[可観測性契約](quality-input-observability.md)を正とする。

## 有界I/Oと終了

JSONLはLFを含め64 KiBまで。stdin queueは最大8 command、stdout queueは最大32 event、
一回のservice tickは最大16 frame/20 ms。JSON出力のstdoutと診断stderrを混在させない。
blocking pipe read/writeはそれぞれ一つのworkerに限定し、serial ownerはqueue投入で待たない。
queue満杯、writer断、stdin EOF、不正要求、Ctrl+Cで入力を停止し、serviceを切断してportを解放する。
既存serial acknowledgementの3秒上限は維持する。
終了時のstdout drain待ちは最大200 msで、読み手が停止したpipe workerを無期限joinしない。
queue満杯時はerror行自体を渡せない場合があるので、consumerはEOF、非zero exit、age超過も失効として扱う。

## 検証境界

strict command/状態遷移、clock stamp、実保存profileとscripted v3による80 Hz配信、quality失効、
Stop/reconnect、pipeの有界queue/行上限、binaryのstatus/EOF/errorをsoftwareで検証する。
実port、実board、実操作者のtare/neutral、二台運転、Windows配布binary、robot運動は別の受入である。
bridgeが起動したことを実機入力経路の受入完了と扱わない。
