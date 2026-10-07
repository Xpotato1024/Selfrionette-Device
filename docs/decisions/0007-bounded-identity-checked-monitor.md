---
status: accepted
owner: architecture
date: 2026-10-07
related:
  - docs/contracts/sample-monitor.md
  - docs/decisions/0006-explicit-serial-transport.md
---

# ADR-0007: Identityを検証する有限sample monitorをhostに追加する

## Context

ADR-0006のexplicit-port transportとProtocol v2は実装済みである。
一方、sampleにはidentityを毎回付けないため、consumerは接続時metadataをsampleへ関連付ける必要がある。

実機なしで検証できる次の段階として、Device ManagerやSim adapterが利用できる小さなhost境界を用意する。
校正成功契約や物理単位は未確定なので、monitorの完了をsensor readyの証拠にはできない。

## Decision

既存の`DeviceSession`に、sessionを消費する`monitor`を追加する。
期待するstable IDと有限sample数を要求し、新しい`info` handshakeの一致後だけtyped frameをcallbackへ通知する。

既存のline/deadline制約をhandshakeとcaptureへそれぞれ適用する。
capture中にmetadataが変化した場合は停止し、再接続や過去metadataの再利用は行わない。

CLIは同じlibraryを使う短いdiagnostic captureを提供する。
publicな動作は[sample monitor contract](../contracts/sample-monitor.md)を正とする。

## Alternatives

- vectorの文字列表示だけを追加する: handshakeのidentityを保証できず、consumer間で確認処理が重複する。
- daemonやGUIを先に追加する: port ownership、role binding、長時間運転の要件が未確定なので今は追加しない。
- tare成功判定を追加する: current firmwareの`calibration_end`はchannel失敗後にも出得るため、今回の実装対象にできない。

## Consequences

- fake transportと共通wire fixtureでidentity/sample境界を検証できる。
- typed callbackからGUIやSim consumerへmetadataとframeを渡せる。
- sampleの数値、timestamp、診断tokenは既存protocolの意味を保つ。
- 短いdiagnostic captureだけを扱い、継続的control入力、健康判定、校正判定は別途必要である。
- 実serial open、firmware upload、EEPROM変更の許可はこのADRから導出しない。
