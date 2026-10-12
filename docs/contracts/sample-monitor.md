---
status: canonical
owner: architecture
last_verified: 2026-10-07
canonical_for:
  - identity-checked host sample capture
  - monitor consumer callback boundary
related:
  - docs/decisions/0007-bounded-identity-checked-monitor.md
  - docs/contracts/serial-transport.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/contracts/device-host-boundary.md
---

# Sample Monitor Contract

## Scope

`selfrionette-serial::DeviceSession::monitor`は期待identityを確認した短いdiagnostic captureを所有する。
calibration validity、sensor health、robot control readyを判定しない。

## CLI

```text
selfrionettectl monitor --port <port> --id <device-id> --samples <1..64>
```

- operator指定の1 portだけをopenする。auto-open、auto-reconnectは行わない。
- port、canonical ID、sample数、全flagをopen前に検証する。
- `--id`は期待stable IDであり、provision操作やrole指定ではない。
- 標準出力はidentityを添えたhuman-facing diagnostic表示。安定したmachine protocolではない。
- 引数不正はexit 2、session/output失敗はexit 1、要求sample数を取得した場合のみexit 0。
- 出力済みの一部sampleはfailureでも残り得る。consumerはexit statusを確認する。

## Library lifecycle

```text
DeviceSession<T>::monitor(expected_id, sample_count, on_frame) -> Result<DeviceInfo, MonitorError>
on_frame: (&DeviceInfo, &ProtocolFrame) -> std::io::Result<()>
```

1. sample数が1以上かつsessionのmax_lines以下であることを確認する。不正時はtransportへ触れない。
2. 既存`query_info()`でinput queueを破棄し、`info\n`を送信する。
3. compatibleかつprovisionedなDeviceInfoのIDをexpected_idと照合する。
4. 一致後、Device frameをcallbackへ一度通知する。
5. queueを再破棄せず、後続frameを同じDeviceInfoとともに通知する。
6. Vectorだけをsample数として数える。要求数で終了し、DeviceInfoを返す。

sessionを値で消費する。成功・失敗後に同じsessionを再使用しない。
実SerialPortTransportはsession終了でdropされる。注入transportの外部共有resourceの寿命はその実装が所有する。

## Bounds

[serial transport](serial-transport.md)のline上限とSessionLimitsを使う。
既定値はhandshakeとcaptureの各段階で最大64 lines / 5 seconds。
Device / Status / Warning / Unknownもline budgetを消費する。
同じcaptureのdeadlineはframe受信で更新しない。

callbackは同期実行し、速やかに戻る必要がある。deadlineはcallbackやOS schedulingを強制中断しない。
CLIのstdoutへの書込みもOS pipe等により待ち得るためhard real-time保証はない。
sample履歴や診断queueはlibrary内へ蓄積しない。

## Failure behavior

次の場合は以後のframe通知を停止してerrorを返す。

- unprovisionedまたは期待ID不一致
- 不正protocol frame、非互換protocol/channel数
- capture中のDeviceInfo変化（ID、firmware version、protocol major、channel数）
- line budget超過、deadline超過、transport失敗
- callback/output失敗

同一metadataの追加Device frameは診断として通知する。
Status / Warning / Unknownは独立したtyped frameとして通知し、sampleや偽zeroへ変換しない。
warningから校正成否を推測せず、`calibration_end`も校正成功としない。
`info`以外のcommandは送信しない。

## Xpotato-Sim / GUI consumer boundary

consumerはcallbackのDeviceInfoとVectorFrameを同一sessionの組として扱う。
frame内の7ch値は既存firmware sampleそのままであり、新たにN等の物理単位やgainを付与しない。
timestamp_msはdevice boot基準のu32 millisであり、wrapを含めそのまま渡す。
host monotonic clockとの差を直接取らない。

Sim側が別途所有する事項:

- 複数接続のduplicate ID拒否、left/right等のrole binding
- host受信時刻とsample freshness、reset/epoch検出
- 健康/校正状態が必要なoperationの追加gate
- channelからrobot入力へのmappingと研究条件
- 長時間streaming、port ownership、GUI lifecycle

このdiagnostic monitorの成功を、上記のoperationへ進める許可やvalidityとして使わない。
共有fixtureは`tests/fixtures/protocol-v2-monitor.txt`。合成データであり、hardware evidenceではない。
