---
status: canonical
owner: device-management
last_verified: 2026-10-16
canonical_for:
  - required per-device setup and per-operator calibration workflow
  - setup and operator-profile binding foundation
  - staged implementation and end-to-end acceptance roadmap
related:
  - docs/decisions/0010-required-device-setup-and-personal-calibration.md
  - docs/contracts/input-calibration.md
  - docs/contracts/device-persistent-settings.md
  - docs/contracts/firmware-update.md
  - docs/architecture/device-manager-workflow.md
---

# 必須の装置設定と必須の個人校正

## 製品要件と今回の到達点

正常入力の前提は、**個体ごとの装置設定**と**操作者ごとの個人校正**の両方である。
個人校正は追加改善ではなく、操作者・姿勢・握り方に適した入力を得るための必須機能とする。
既存の合成preview、v2受信、JSON生成、schema検証、compile成功だけでは、この要件を満たさない。

本書は二つのworkflowと実装順の正本。既存GUIの現在の操作・配置は
[Device Manager workflow](device-manager-workflow.md)、数値処理は
[入力適合契約](../contracts/input-calibration.md)、EEPROMの既存byte形式は
[装置設定契約](../contracts/device-persistent-settings.md)を参照する。

今回の実装はRust `setup` / `setup_persistence`とファイル専用CLI `setup-inspect`まで。
装置の全mapping検証、raw専用の純粋な投影、個人profileのbinding照合、volatileなneutralの失効条件、
boundedなbinding manifest保存・再読込みを追加した。既存`input_calibration::fit`を呼ぶ`fit_v2`も提供する。
新しい数値fit/filter/正規化アルゴリズムは作らない。

**未実装**: Firmwareの新しいinfo/取得品質/tare/settings操作、実機設定の適用、更新backend、
実個人校正GUI、永久parameterの保存・再利用、新domainのfit、Sim consumerの結線。
profile-binding manifestはparameter recordを識別するmetadataであり、spanやfilter係数そのものを格納しない。
既存v2のvector形式、既存synthetic `.srnp`、EEPROM journal/identity形式は変更していない。
field-testの実装・記録にも変更を加えない。PR16の完了を後続実機工程の前提とする。

## 必須workflow 1: 個体の装置設定

1. 実機が返した128-bit `srn-` IDと対象board、組立revision、設定revision、settings generationを確認する。
   COM番号、接続順、例示ID、外見の左右からidentityや利用roleを作らない。
2. 元のwire CH0–6を保持して、実際のセンサ位置・感度軸・荷重経路・論理軸への対応を確認する。
   「CH0–2=親指、3–5=人差指、6=小指」はtip未装着時の利用者の暫定仮説であり、global defaultにしない。
3. 一回に一つの実際の指/部位と一つの方向を指示し、全chの応答を観測する。
   図の組立姿勢・原本revisionと実配線が一致する根拠を必要とする。未知の部位や方向を推測して矢印を描かない。
4. 配線対応・実装向き・raw基準の極性を候補として比較し、実測前後を表示して明示適用する。
   未確認の極性は`None`。+1や0で埋めない。単なる符号反転で荷重経路や軸入替えを解決したことにしない。
5. 設定readbackと再接続後の同一性を確認する。取付変更では**全chの向きと対応**を再検証する。
   assembly/config revisionとgenerationを更新し、旧個人profileを要再検証、旧session neutralを無効にする。

既知基準荷重によるcount→N係数は後続でよい。個体の配線・向きの検証と個人校正を待たせる理由にしない。
実際のreference load、荷重経路、範囲、温度等の根拠なしにN係数を作らない。
host snapshotの論理mapping/config revisionは現在のEEPROM schema 1に存在しない。
今回のmanifestをそのままEEPROMへ書き込むadapterは提供しない。Firmware結線前にversioned contractを定める。

## 必須workflow 2: 個人校正と毎sessionのneutral

装置設定、exact Firmware/domain、操作者、姿勢、握り方を固定して開始する。
一つの装置を別人・別姿勢で使う場合は別profileとし、左右/robot roleと混同しない。

1. 姿勢・握り・支持方法を利用者の言葉と図で確認し、準備ができた利用者が「開始」を押してから取得する。
   UI表示や固定timerだけで開始しない。取得の時間・sample数・品質・停止条件を有限にする。
2. 自然な使用姿勢で、意図的な操作をしない個人neutralを取得する。無荷重ADC tareとは別物。
3. 実際の指/方向を一つずつ、痛みのない快適な正方向・負方向の範囲で複数回測定し、毎回除荷を記録する。
   7chを7本の独立した指とみなさず、指示対象と全ch応答を残す。未測定の負側spanは`None`のままにする。
4. noise、drift、繰返し、戻り、方向の一貫性を評価し、deadzone/noise/filter候補を提示する。
   既存engineのdiagonalを基準に、条件数/noise増幅を満たす場合だけcoupled compensationを比較できる。
   合成fixtureの閾値を実測の推奨値にしない。
5. **観測された値と提案された補正**を別表示し、正方向・逆方向・clamp前の値・欠測・span超過を確認する。
6. fitに使っていないheld-outの指示・同時入力・neutralへの戻りで検証する。学習記録を検証記録として再使用しない。
7. exact profile revision、device/assembly/config/generation、Firmware/source/domain、操作者/姿勢/握り、
   count / N / 無次元という単位と変換順を示し、利用者が明示適用する。保存と有効化は別操作にする。
8. 永久parameter（正負span、noise、filter、採用method/根拠）を保存し、次回は互換性検証後に再利用する。
   session neutralやready flagを保存して復元しない。再接続・reset・tare後は新しいneutralを必須にする。
   互換性が保たれる限り、tareだけで永久spanまで捨てない。

各stepで再試行・skip・abortを提供する。skipは欠測を保って未完了にし、成功へ進めない。
abort/切断/timeoutは進行中の確認・適用を無効にし、観測済み証拠は由来付きの履歴として残せる。
再試行の成功で以前の失敗を消したように見せない。品質不明、弱信号、向き不一致、戻り不良、
過大noise、欠測、干渉、context不一致などの**失敗理由と次の行動**を近傍に表示する。
未実装の実機校正buttonや、必ず成功するlive mockは公開しない。

## domainとcore境界

| Domain | 意味 / 許可される処理 |
|---|---|
| `raw_adc` | negation/tare/hold前の符号付き24-bit整数count。今回の`project_raw`だけが受け付ける |
| `firmware_v2_vector` | 既に一律negation、RAM tare、spike抑制を受け、timeoutで前値を保持し得る7ch。rawやusable品質とみなさない |
| `device_oriented_counts` | 個体の方向/対応を一度適用したcount。raw投影の再入力は禁止。今後のstreamではtareの有無/epochもcontractで固定する |
| `newtons` | 根拠のあるreference load係数が必要。今回の変換・係数生成対象外 |
| `normalized_input` | 個人の正負comfort span等に基づく無次元入力。robot gain/座標写像は含まない |

`project_raw`はrawの符号/論理軸投影のみで、tare/filter/個人neutralを引かない。
戻り値`OrientedCounts`は別型で、値は論理軸順。wire順の元値は書き換えない。
これは将来Firmware出力のbyte仕様を決めるAPIではない。将来の装置内適用では既存契約のsession zeroと
wire順の保持を含めて新frameを定義し、logical projectionはconsumerが明示的に扱う。
v2、N、無次元、投影済みcountをrawとして渡すと`DomainMismatch`。
数値だけを抜いてdomain tagを偽る呼出側の誤用を物理的に検出する機能ではない。

`DeviceSetup::validate`は未知欄を含む部分観測の構造を検証する。
投影/profile bindingには全7chのpermutation、位置/方向、±1、mapping確認、usable注記が必要。
重複軸、範囲外軸、同じ部位/方向の重複、ゼロrevision、空/長過ぎるmetadata、非有限sampleを拒否する。
identityは既存`DeviceId` parserを使い、自動採番やfixture IDへのfallbackはしない。

`ProfileBinding`はfull setup snapshot、profile revision、parameter record識別子、exact Firmware version/source commit、
sample domain、操作者/姿勢/握り、証拠由来を保持する。Firmwareの実行image digest/readbackは後続で追加が必要。
`check_compatibility`は同じrevisionを誤って再利用したmapping編集も検出する。
設定が異なれば`RevalidationRequired`、Firmware/domain/操作者不一致はそれぞれ明示error。
source commitは40桁lowercase hexでall-zero不可。buildの真正性や実機稼働imageをこれだけで証明しない。

`fit_v2(current, recording, limits)`はbindingと記録context/明示軸を照合してから既存engineに委譲する。
v2以外は`UnsupportedFitDomain`。raw補正済み値をv2と称して二度処理しない。
既存recordingにはsource commit/握り/全provenanceを直接検証する欄がないため、呼出側が正しい取得contextを
結び付ける必要がある。このhelperの成功はlive利用許可を返さず、`FittedInput`のverified/readyもfalseのまま。
記録schemaの拡張と永久parameterの再構成は後続工程で、現在のv2/preview schemaを暗黙に移行しない。

`SessionNeutral`は永続化しない。context/profile snapshotと`SessionEpoch { connection, tare }`を保持する。
取得所有者が再接続/reset時にconnection、tare時にtareを更新し、失効したneutralを再使用しないことが前提。
新しい中立値は有限・usable注記が必要で、syntheticまたはunknown由来のprofileからlive用neutralは作れない。
`SessionNeutral`自体は安定性判定や運転許可ではなく、取得済み観測の寿命を検証する純粋な値である。
実入力処理への結線、品質根拠、neutral windowの受入基準は未実装。

## ファイル境界と実行方法

```text
selfrionettectl setup-inspect <context.srnc> [profile-binding.srnb]
```

ファイルだけを読み、列挙/serial open/tare/EEPROM/flashは実行しない。
構造不正・不適合はexit 1、引数不正はexit 2、構造検査成功はexit 0とJSONを返す。
部分setupは理由付きでinspectできるが、binding比較には完全な確認済みmappingが必要。
`profile_binding_compatible`はmetadata比較結果だけ。parameter recordの存在/内容は読み込まない。
`parameters_loaded=false`、`fresh_personal_neutral_required=true`、`personal_calibration_required=true`、
`device_setup_required=true`、`hardware_verified=false`、`control_ready=false`を常に返す。
最後のrequired値は、ファイル検査だけでは実機の必須工程を完了扱いにしないという意味である。

`setup_persistence::{encode_context,decode_context,encode_binding,decode_binding,load_context,load_binding,save_binding_new}`
が共通codec/API。既存persistenceのhex metadata方式と32,768-byte上限を再利用し、外部依存を増やさない。
metadataは各128 UTF-8 bytes以内、前後空白/control文字を拒否する。
読込みはregular fileとサイズを確認し、上限+1 byteまで読み、必ず構造/意味を再検証する。
破損時に過去値へfallbackしない。`.srnp`や古い品質unknown captureからbindingを生成する移行APIはない。

schema 1の行順は次の通り。文字列はUTF-8 byteのhex、欠測は`?`、boolは0/1。
不明schema、追加/欠落行、wire順違い、非数値・不正flag・不正ID・不正極性を拒否する。

```text
context: srn-setup-context,1
binding: srn-profile-binding,1
共通setup:
  srn-setup,1
  device,<srn-ID>,<assembly u32>,<config u32>,<generation u32>,<mapping validated bool>
  evidence,<synthetic|recorded>,<quality>,<reference hex>
  wire,<0..6 in order>,<logical axis or ?>,<location hex or ?>,<direction hex or ?>,<polarity or ?>
共通tail:
  firmware,<version hex>,<source commit>,<sample domain>
  operator,<operator hex>,<posture hex>,<grip hex>
binding追加:
  profile,<revision u32>,<parameter-record identifier hex>
  evidence,<synthetic|recorded>,<quality>,<reference hex>
```

qualityは`usable|unknown|reset|timeout|spike|clipped`。originをFirmware名から推定しない。
setupとprofileのoriginは一致し、binding作成時のqualityはusable必須。ただしcallerによる注記であり、
品質不明の古い記録をusableへ書き換える根拠ではない。syntheticはroundtrip後もsyntheticのまま。
保存済みfull setupはprofile作成時のimmutable snapshotであり、現在の装置設定の第二SoTではない。

`save_binding_new`は既存private directoryの絶対pathへ新規作成のみ。既存fileを置換しない。
全内容を検証してからcreate_new/write/syncし、失敗時は自分が新規作成したfileの除去を試みる。
同時readerは書込み途中を不正recordとして拒否する。atomic replace/複数file transactionとは称さない。
cleanup失敗時の残骸は再読込みで拒否され、明示的な管理が必要。trustedな個人用directoryを前提とし、
敵対的な親directory差替えや共有storeの競合解決は今回のpublic contractに含めない。
CLIにsave/apply操作は追加していない。

## 実装順と受入gate

| 順番 | 実装・受入 | 現在 |
|---|---|---|
| 0 | PR16を完了。unknown観測とusable証拠を分離 | 別branchの責務。今回未変更 |
| 1 | 本書/ADR、domain/setup/profile binding、file-only検査、決定的試験 | 今回のslice |
| 2 | Firmware acquisition/info/settingsとhost parserの共通契約・実装 | 後続 |
| 3 | application device management / updaterをGUI/CLIの共通use caseへ接続 | 後続 |
| 4 | 実機の個人校正GUI、永久parameter保存、consumer引渡し | 後続・必須 |

FirmwareはPro Micro / ATmega32U4、C++/AVRを継続し、normal pathでheapを増やさずboundedにする。
実装順2では、EEPROMに保持する128-bit IDとimage内に固定するversion/source/build identityを分離する。
imageに個体IDを埋めず、identity領域0..31を維持する。raw+quality sampleにはwire CH、raw integer、
sequence/timestamp、boot/tare epoch、settings generation、domain、fresh/timeout/held/spike/clipping等を明示する。
現在のv2から欠けた品質bitを推測しない。正常/診断frame、有限capture、欠測とtimeoutをversioned contractで分ける。

tareはrequest ID・対象ID・期待世代・開始/進行/成功/失敗/readbackを持つ明示transactionにする。
boot autotareや`calibration_end`の文字列だけで成功と判定しない。
成功時のみ新tare epochを確定し、失敗/切断時も旧session neutral/readyを無効にする。
取得停止・再開境界と停止方法を決め、EEPROMへsession zeroを高頻度保存しない。
settingsは既存journalのstaging/readback/commitを再利用し、全mappingとの表現差を先に解決する。
identity領域・現schemaの予約byteを流用せず、変更が必要なら別途versioned設計/互換受入を行う。
compile時のFlash/SRAMを記録し、実EEPROMの電源断・board/bootloader受入はsoftware試験と分ける。

順番3のupdaterは[更新契約](../contracts/firmware-update.md)に従い、board/容量/HEX address/checksum、
source/build条件、image hash、settings schema/migration、信頼するpackageを検査し、保護対象を退避する。
明示書込み後にID・Firmware/build・設定をreadbackし、不一致は要復旧。programmer exit 0を更新受入としない。
対象portの変化や複数device、失敗/取消/電源断復旧、EEPROM保持をsoftware modelの後に実機で検証する。
GUIとCLIは同じtyped use caseを呼び、GUIからCLI subprocessや任意shellをbusiness APIにしない。

通常入力時はManagerを終了できる。個体補正は装置内で一度、個人profileの互換性検証とneutral取得・
入力正規化はconsumer側の共有Rust libraryで行い、Managerの中継を要求しない。
serialは管理側とconsumerで明示的に所有権を引き渡し、二重openを前提にしない。
Xpotato-SimはInput Source consumerとしてversion/domain/品質/ID/profile適合を確認し、
その先のrobot mapping/速度gain/座標系を所有する。daemon、第二stack、RPC/DBを追加しない。

完成gateは、利用者が実際の指/方向を理解して準備・計測・再試行・skip/abort・比較・held-out検証・
保存・明示適用までGUIだけで完遂できること、別操作者/取付変更/切断/tareで正しく止まること、
Managerを終了したconsumerが互換profileと新neutralで正常入力を使えること。
Firmware/backendの完成だけでGUI end-to-end受入や利用者要求を完了としない。
