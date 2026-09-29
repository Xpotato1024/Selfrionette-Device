# AGENTS.md

Last updated: 2026-09-29

## 0. Purpose

このファイルは`Selfrionette-Device`で作業するAI agent / Codex向けrepository-local instructionである。

このrepositoryはSelfrionette実デバイス側のsource of truthであり、firmware、device protocol、identity、physical calibration、diagnostics、host-side device managementを扱う。robot simulation / robot control mapping / experiment physicsは`Xpotato-Sim`の責務であり、このrepositoryへ取り込まない。

## 1. Read first

作業前に必要な範囲だけ確認する。

1. `AGENTS.md`
2. 対象Issue / PR
3. `docs/README.md`
4. 関連ADR
5. 関連canonical architecture / contract / operations document
6. 関連implementation / tests
7. hardwareを扱う場合は`docs/operations/hardware-safety.md`

過去chat、旧Selfrionette repository、Xpotato-Sim内のlegacy firmwareをcurrent specificationとして扱わない。migration sourceとして参照する場合も、現行contractとの差分を確認する。

## 2. Source of truth

- documentation map: `docs/README.md`
- rationale / accepted decisions: `docs/decisions/`
- current architecture / ownership: `docs/architecture/`
- public / cross-layer contracts: `docs/contracts/`
- operator procedures: `docs/operations/`
- firmware source: `firmware/`
- host libraries / CLI: `crates/`
- human-facing applications: `apps/`
- hardware definitions / references: `hardware/`
- repository tools: `tools/`
- tests: `tests/`

同じ事実を複数箇所で独立SoTとして管理しない。

Accepted ADRは後から都合よく書き換えない。decision変更は新ADRでsupersedeする。

## 3. Architecture boundaries

### Device repository boundary

このrepositoryが所有する。

- Pro Micro / ATmega32U4 firmware
- load-cell acquisition
- device identity
- protocol version / firmware version
- physical tare / calibration
- persistent device settings
- sensor health / diagnostics
- firmware build / upload workflow
- host-side discovery / configuration / diagnostics
- Device Manager / CLI

### Xpotato-Sim boundary

このrepositoryで所有しない。

- MuJoCo physics
- robot model / FK / IK
- robot control mapping
- endpoint / joint command policy
- world / tool / task control frame
- experiment task definition
- simulation evaluation

Xpotato-Simから見たSelfrionetteはGamepad / Keyboardと同格のInput Source Pluginである。Device Manager、firmware updater、calibration wizardをsimulation pluginへ押し込まない。

## 4. Firmware invariants

初期firmwareはADR-0001に従う。

- MCU / board: Pro Micro / ATmega32U4
- language: C++
- initial toolchain: PlatformIO + AVR / Arduino ecosystem
- Rust firmwareはcurrent implementation targetではない
- RTOSまたは独自async executorを安易に追加しない

ATmega32U4 resourceは有限である。

- Flash: 32 KB total
- SRAM: 2.5 KB
- EEPROM: 1 KB

実際のapplication使用可能量はbuild / bootloader構成から測定する。総容量をそのままbudgetとみなさない。

### Memory discipline

- normal pathでheap allocationを避ける
- hot pathでArduino `String`等の動的所有文字列を使わない
- buffer / queue / diagnostic historyはboundedにする
- large local array、recursion、無制限log蓄積を避ける
- sample historyをfirmware内へ必要以上に保持しない
- EEPROMへ高頻度writeしない
- build時のFlash / SRAM usageをvalidation evidenceとして記録する
- resource不足を隠すために診断やfailure handlingを削らない

## 5. Device / protocol invariants

- COM番号、USB接続順、OS device pathをdevice identityとして扱わない
- stable device identityと`left` / `right`等のroleを分離する
- protocolはversioned contractとして管理する
- malformed / incompatible / incomplete stateはfail closedを基本とする
- timeoutやdiagnostic queueを無制限にしない
- calibration値とrobot control gainを混同しない
- physical calibrationはDevice側、robot mappingはXpotato-Sim側
- raw evidenceとnormalized / calibrated値を混同しない

## 6. Implementation discipline

`YAGNI / KISS / small feedback loop`を基本とする。

- current acceptanceを満たす最小correct implementationを選ぶ
- 将来必要かもしれないdaemon、plugin framework、RPC、binary protocolを先回り実装しない
- wrapper / adapter / registryは実在する責務境界が必要な場合だけ追加する
- firmwareではresource costを含めてabstractionの価値を判断する
- host側とfirmware側で同じlogicを二重実装しない
- failureをsilent fallbackや偽zero sampleへ変換しない
- stableでtestableなbehavior変更は可能ならregression testを先に置く

## 7. Autonomy and hardware boundary

説明、調査、レビュー、設計ではread-onlyを基本とし、依頼されていない変更を行わない。

実装依頼ではtask scope内のcode、tests、canonical docs、software-only validationを変更してよい。

次は明示許可を必要とする。

- merge / Issue close / branch delete
- hardware access
- serial port open
- firmware upload / flashing
- device EEPROM / persistent calibration mutation
- destructive migration / large delete
- public contractの重大な破壊的変更
- secrets / credentials / deployment

hardware task以外では、serial port openやfirmware uploadを行わない。compile成功をhardware validationと呼ばない。

## 8. Validation

changed / materially affected riskへ比例した検証を選ぶ。

Firmwareでは必要に応じて:

- PlatformIO compile
- compiler / linker resource usage
- protocol parser fixture
- bounded state machine test
- static / host-side unit test
- `git diff --check`

Host toolingでは必要に応じて:

- Rust format / clippy / test
- protocol golden fixtures
- CLI deterministic tests
- GUI typecheck / build

Hardware validationは別カテゴリとして記録する。

- target device
- port / identity
- firmware revision
- command
- expected behavior
- observed behavior
- stop procedure
- rollback
- persistent state mutationの有無

test削除、skip、assertion弱体化だけで変更を通さない。

## 9. Documentation contract

`docs/contracts/documentation-contract.md`を正とする。

- `docs/README.md`はSource of Truth Map
- 1 topic = 1 canonical document
- current specificationとhistorical evidenceを分離する
- accepted ADRをcurrent contract代わりにしない
- behavior / ownership / protocol / operator procedure変更時は関連canonical docを同じ変更で同期する
- Human-facing本文は日本語を基本とし、identifier / path / protocol literal / formal nameは必要に応じ英語を使う
- UTF-8 without BOMを基本とする

## 10. Git / GitHub

- bootstrap後は`main`へ直接commit / pushしない
- agent branchは原則`codex/` prefix
- branch / commit / push / PR / mergeを別操作として扱う
- PR前にbase、branch、actual diff、working treeを確認する
- 報告前にlocal / remote / PR headを一致させる
- PR本文とactual diff、validation、scopeを一致させる
- `mergeable: true`だけでmerge readinessを判断しない
- 明示許可なしにmergeしない
- unrelated cleanupを混ぜない

## 11. Repository hygiene

- generated binary、build output、cache、local config、serial captureの無制限dumpをcommitしない
- secretsやmachine-local pathをcommitしない
- hardware evidenceを保存する場合は最小化し、再現条件とprovenanceを残す
- firmware binaryをrelease artifactとして扱う場合はsource revisionとbuild configurationを追跡可能にする

## 12. Definition of done

完了前に確認する。

- acceptance criteriaを満たした
- current architecture / contractと矛盾しない
- required docsを同期した
- changed scopeのvalidationがPASSした
- firmware変更ならresource usageを確認した
- hardware未実施なら未実施と明記した
- unrelated / generated artifactを含まない
- unresolved blockerを成功扱いしていない

完了後に「念のため」の追加抽象化、追加framework、追加全面検証を開始しない。
