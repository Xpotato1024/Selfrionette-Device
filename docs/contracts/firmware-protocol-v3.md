---
status: canonical
owner: firmware
last_verified: 2026-10-16
---
# 品質付き opt-in protocol 3

v2 の `info` / `vector` / `tare` / `provision` は互換経路として残す。
v2 の値は加工済み・品質不明であり個人校正には使用できない。
115200 baud、ASCII、LF 区切り。数字は符号なし十進数（count は符号付き）、
request は 1..4294967295。command payload 上限87 bytes、frame 上限511 bytes。
現行firmwareの有効なcommand shapeは最大87 bytes（`settings3`の最大値）で、
より長い受信lineはboundedな`command_too_long` warningで拒否する。v2 commandは63 bytesまで。
不正・余分なfield・範囲外・未対応commandは副作用なしで拒否する。

## Command / reply

```text
info3,<request>
info3,<request>,<version>,<source40>,sparkfun_promicro16,atmega32u4,16000000,<srn-ID|unprovisioned>,<generation>,<assembly>,<known>,<negative>,<tare_epoch>,<tare_valid>
stream3,<request>,<session>,<0|1>
ok3,<request>,stream,<session>
settings3,<request>,<srn-ID>,<expected_generation>,<assembly>,<known>,<negative>
ok3,<request>,settings,<actual_generation>
tare3,<request>,<srn-ID>,<expected_generation>
ok3,<request>,tare,<changed_epoch>
error3,<request>,<token>
```

`info3` は上記機能への明示capability。source はimage内の40桁 lowercase Git SHA。
generation=0 / assembly=0 / known=0 は未設定。破損journalは info で正常設定と偽らず error。
settings は schema 1 journal のstage/readback/commit後だけ応答。同値は書込みなし。
hostは応答requestと対象IDを照合し、info再読でgeneration・全設定を確認する。
provision はv2の既存一回限りのtransactionで、明示確認後OS RNG生成IDを使う。

stream開始時のsessionはhostが選ぶ非ゼロchallenge。新しいchallengeはsequenceを0へ戻し、
session tareを失効する。同じchallengeへのstartはactive中なら冪等、stop後ならpauseからの
resumeであり、sequenceと成功済みtareを保持する。resume時はpause中のfresh/rejected状態を
捨て、resume処理後に取得した読み取りだけをfreshとして出力する。tare中のsame-challenge
resumeもtareをcancelしない。challenge変更は進行中tareをcancelし、成功済みtareも失効する。
hostはboundedなtare pause/resumeで同じchallengeを使い、再接続時は新しいchallengeを使う。
reset後はv3送信offとなり再handshakeが必要。hostはconnection epochとsequence逆行/停止も監視する。
v3送信中はvector送信を停止し、停止でv2へ戻る。sessionは永続identityではない。

v3 frame送信は最大20 Hz（50 ms以上の間隔）。出力backpressure中のframeはskipされ、
sequenceは出力queueへ入ったsampleだけで進む。センサfresh期限は50 msで、期限超過後の
既存rawには`held|timeout`を付け、新raw未取得ならraw/orientedは`?`とする。

## Sample

```text
sample3,<session>,<sequence>,<device_ms>,<tare_epoch>,<generation>,<tare_valid>,<raw0>.. <raw6>,<oriented0>.. <oriented6>,<flags0>.. <flags6>
```

実際のfield間に空白や `..` は含めず、合計28 fields。rawはwire CH順、
HX711の符号付き24-bit ADC（-8388608..8388607）、negation/tare/hold前。
欠測raw/orientedは`?`。orientedもwire順で、確認済み極性とsession tareを
`polarity * (raw-zero)` と一度だけ適用。hostのlogical permutationでは再反転しない。
全ch確認済み・tare有効でなければorientedは欠測。

flagsはbitmask: fresh=1, timeout=2, saturated=4, held=8, rejected=16。
signed 24-bit ADCの両endpoint `-8388608` / `8388607` はsaturated。
fresh単独だけを校正/通常処理へ渡す。saturated/rejectedのrawは診断用に保持し、
前値保持をfreshと称さない。品質は取得状態であり計量学的な精度保証ではない。

tare開始は旧zeroを直ちに失効。全7chでfresh・非飽和sampleを各8件、開始後2000 ms以内に
収集し、各chのmax-minが2000 count以下なら平均をzeroとしてepochを更新する。
2000-count条件は手続的なsoftware gateであり、計量学的精度の保証ではない。
成功時のみ`ok3,<request>,tare,<changed_epoch>`を返す。失敗時tare_valid=0でerrorを返す。
EEPROMにzero/session/sequenceを書かない。settings変更でもtareを失効する。
hostは失敗・切断・設定/epoch変化で進行中captureと適用を失効させる。

error tokenは固定ASCII識別子（最大22 bytes）、response lineは最大41 bytes。
current error token群は`malformed_command`, `stale_session`, `stale_generation`,
`device_mismatch`, `settings_corrupt`, `busy`, `invalid_settings`,
`generation_exhausted`, `settings_write_failed`, `settings_verify_failed`,
`tare_epoch_exhausted`, `tare_quality`, `tare_timeout`, `tare_cancelled`。

取得/commandはbounded、センサready確認後も無期限waitしない。
-65536 impulseの原因や修復を証明した仕様ではない。
