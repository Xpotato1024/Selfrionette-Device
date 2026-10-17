---
status: canonical
owner: host
last_verified: 2026-10-16
---
# Managerを終了して正規化入力を利用する

装置の方向確認・保存と個人校正はGUIで行う。共通Rust serviceはManagerの常駐を要求しない。
他のconsumerへ渡す前にManagerの接続を切断し、serial handleを解放する。
利用側が明示portを所有し、稼働Firmware/ID/domain/設定世代と永久profileの一致を確認する。
robot mapping、速度gain、FK/IK、制御座標系はこのlibraryに含まれない。

## CLI

保存済みprofileの絶対pathを指定する。個体設定recordを含む元のstore配置を維持する。
操作者・姿勢・握り・methodは明示指定し、保存時の条件との不一致を拒否する。

```text
selfrionettectl input --port COM9 --profile C:\private-store\srn-<32hex>\<record>.srncal --operator my-name --posture seated --grip supported --method diagonal --frames 1200 --neutral-ready --tare-confirmed
```

`--tare-confirmed` は対象と無荷重状態を確認した場合だけ指定する。
新しい接続challengeは今回のdevice tareとneutralを失効させるため、再接続時は明示tareが必要になる。
`--neutral-ready` は指定した自然な使用姿勢でneutralを取得する準備があることを表す。
無荷重tareの後はCLIがEnterによる別のuser-ready確認を待つので、自然な使用姿勢・握りへ戻ってから押す。
Esc/Ctrl+Cで中止でき、確認待ちは15分で終了する。`--tare-confirmed`を伴う場合は対話端末が必要。
自動consumerは共通libraryを呼び、tareとneutralの二つのuser-ready操作を自身のUIで分離する。
現在のCLIは有限sample数を受け、Ctrl+Cで終了する。品質/identity/epochの異常は欠測を表示して停止し、偽zeroにしない。

stdoutはJSON Lines。device identity、Firmware/source、sequence、device time、tare epoch、settings generation、
取得品質flags、clamp前の正規化入力、範囲制限後のcommand、欠測、span超過と未測定方向を持つ。
countはNではなく、正規化入力は無次元。出力はrobot commandの許可ではない。

## Rust consumer

`selfrionette_serial::app::LiveService`に一つの`LineTransport`を渡し、
GUIと同じ`Command`でconnect/stream/tare/operator/load/session-neutral/applyを行う。
`execute`が返す`Reply.view.output`を利用する。取得窓・fitter・filter・保存の別実装を作らない。
consumerはuser-readyの取得開始、排他的port所有、poll継続と切断時の破棄を担当する。
callbackやframeの品質・世代不一致を握り潰さず、出力が失効したら利用を停止する。

Xpotato-Simには、このserviceを呼ぶInput Source adapter、profile選択と明示context確認、
取得品質/欠測の伝搬、serial所有権の引渡しを別変更で追加する必要がある。
このreleaseはそのadapterを統合していない。ここから先のrobot制御の動作確認も未実施。
