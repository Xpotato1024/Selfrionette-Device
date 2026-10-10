---
status: canonical
owner: architecture
last_verified: 2026-10-10
canonical_for:
  - host serial transport behavior
  - initial live selfrionettectl commands
related:
  - docs/decisions/0006-explicit-serial-transport.md
  - docs/contracts/host-device-core.md
  - docs/contracts/firmware-protocol-v2.md
  - docs/operations/hardware-safety.md
---

# Serial Transport Contract

## Scope

`selfrionette-serial`はOS serial portとProtocol v2 host coreのbridgeを所有する。

stable device identityそのもの、robot mapping、GUIは所有しない。

旧Simのraw `c` / SendTextとstreaming monitor/measureは独立した`legacy` moduleで扱う。
[旧Sim host tools契約](legacy-sim-host-tools.md)を正とし、以下のv2 bounded management queryとは混ぜない。

## Port enumeration

`available_port_names()`はOSが列挙したport名をsort / deduplicateして返す。

列挙結果:

- Selfrionetteであることを証明しない
- stable identityではない
- left/right roleを決定しない

`selfrionettectl list`はこの列挙だけを行い、portをopenしない。

## Open

initial live operationはoperatorが`--port`で明示したportだけをopenする。

- baud: 115200
- DTR stateをidentity / role判定へ使わない
- auto scan / auto openは行わない

## Line acquisition

- maximum line payload: 1024 bytes
- CRは除去
- LFでline確定
- invalid UTF-8はfail
- readは20 ms以下のslice timeoutを繰り返す
- management overall deadlineは5 seconds
- management line budgetは64

past sample、fake zero、last-known device infoでtimeoutを埋めない。

## Info query

`query_info()`:

1. input queueをdiscardする
2. `info\n`をwriteする
3. boundedにframeをparseする
4. `device` frame以外は分類してskipする
5. device frameのprotocol major / channel count compatibilityを確認する
6. compatible device infoを返す

malformed frameはsilent skipせずfail closed。

## Provision

`provision(id)`:

1. `query_info()`
2. already provisionedなら停止
3. input queue discard
4. canonical provision command write
5. boundedに`status,provision_ok`を待つ
6. management failure warningはfailure
7. 再`query_info()`
8. exact ID一致を要求

hostがwriteしただけでは成功と報告しない。

## CLI mutation gate

`selfrionettectl provision`は`--yes`を必須とする。

これはrepository agent permissionを置き換えない。AI agentが実deviceへprovisionを実行する場合は、AGENTS.md / hardware safetyに従い別途明示許可が必要である。

## Not yet exposed

- real serial tare
- automatic Selfrionette discovery by opening all ports
- reconnect / retry daemon
- background ownership service
- persistent host registry
