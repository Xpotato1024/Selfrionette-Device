---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-17
canonical_for:
  - Device Manager firmware package validation, device targeting, flash safety, and recovery
related:
  - firmware-protocol-v3.md
  - ../operations/hardware-safety.md
  - ../operations/device-manager-distribution.md
---
# Device Manager firmware update contract

This contract defines the Windows Device Manager update path for the SparkFun Pro Micro 16 MHz / ATmega32U4 `sparkfun_promicro16` firmware. It does not define a general-purpose AVR programmer or an updater for other boards.

## Package and trust

The application release embeds `distribution/firmware/manifest.json`, `distribution/firmware/firmware.hex`, and a fixed AVRDUDE runtime. Modern schema 1 remains supported with the fields below. Bundled packages use schema 3 with explicit firmware naming/version fields; schema 1/2 remains a validated compatibility input.

| Field | Requirement |
|---|---|
| `schema` | `3`（配布名・版）、互換読取りは`1`/`2` |
| `firmware_name`, `distribution_version`, `supported_protocols` | Firmware配布名・配布版・対応通信。アプリ版とは別 |
| `release_version` | Exact Device Manager release version |
| `application` | `selfrionette-loadcell-7ch` |
| `board`, `mcu`, `clock_hz` | `sparkfun_promicro16`, `atmega32u4`, `16000000` |
| `protocol_schema`, `settings_schema` | `3`, `1` |
| `firmware_version` | Exact release version |
| `source_sha` | 40 lowercase hexadecimal characters from the source revision used for the firmware build |
| `image_file` | `firmware.hex` |
| `image_sha256`, `image_size` | Digest and byte count of the exact Intel HEX file |
| `flash_start`, `flash_end_exclusive` | `0`, `28672` (`0x7000`) |

For the embedded package, the app compares the manifest, image, and source hashes against values compiled into that same application build. This establishes package-to-app provenance; a source revision string alone does not establish trust. A user-selected package is reported as `user_selected_unverified`, even when its checksum is internally consistent. The app does not download a firmware package at runtime.

The Intel HEX reader checks record syntax, byte counts, checksums, record types, extended addresses, a single final EOF record, and address overlap. Every data byte must fall in application flash `[0x0000, 0x7000)`. EEPROM, fuses, lock bits, and the bootloader are outside the accepted image range.

## Firmware系統と出典

名前と版の正本は[識別契約](firmware-identification.md)とfirmware/editions.json。catalog schema 2はmodern / legacy_7ch / compatible_7chの固定3件を束縛し、manifest schema 3で配布名・配布版・本体Firmware版・対応Protocolを区別する。旧catalog schema 1の2件は明示互換として検証する。

Managedは従来のpackage位置、Originalはlegacy-7ch、Compatibleはcompatible-7ch配下に置く。各manifest・画像は別のcompile-time hashに束縛し、任意pathやruntime downloadは認めない。

両旧ソースは変更せず、legacy_7ch_package / compatible_7ch_packageのwrapperから固定toolchainでビルドする。旧画像の本体Firmware版は空、管理protocol/settingsは0/0（管理機能なし）。後付けの配布識別版を本体の応答として表示しない。先輩の実機画像との同一性は未確認のまま維持する。

同梱ZIPは3系統の画像、manifest、catalog、SOURCE-INFO、editions.json、ソースとlicense通知を含む。source tree hashはrelative path・NUL・元byte・NULを英字順で連結したSHA-256。sharedとeditions.jsonも個別に検証する。user-selected packageは内部整合しても未検証出典として扱う。

## Preflight and target selection

`prepare` is passive when metadata probing is disabled. Explicit setup/update probing sends only reserved `?3,<request>\n` then `?2\n` aliases, which both frozen sources ignore. No automatic `info3` / `info` fallback is permitted, regardless of destination family: the original source handles character `i` as physical tare. No reply stays unidentified; preview.1 without aliases requires initial-update guidance. It verifies supported board metadata when available and snapshots identity and persistent settings. An unidentified or legacy device requires a separate manual board confirmation. An initial install may proceed without an existing device ID or settings snapshot only after that confirmation; the reply states that previous identity/settings could not be compared.

The selected application port must be unique in the current topology. legacy_7ch requires an available USB serial or location binding; descriptorless legacy switching fails closed. The update plan retains available USB serial and Windows location-path bindings. The displayed COM name is only a current transport address; it is not a durable device identity.

Only the explicit `apply` action begins the 1200-bps USB bootloader touch. The app waits for the application port to disappear and then enumerates bootloader ports. It binds the bootloader to the selected device by USB location path or serial when possible. A known location-path or serial mismatch is contradictory topology and cannot be overridden by manual selection. If at least one binding descriptor is absent, no known descriptor conflicts, and exactly one bootloader candidate exists, the UI must obtain a second explicit operator confirmation naming that candidate. Duplicate or ambiguous candidates, unknown topology, and mismatched bindings fail closed and require recovery; the app never silently reuses the old COM number.

## Flash operation

The updater runs only the packaged AVRDUDE v8.3 Windows x64 executable and configuration. It passes a fixed argument set directly to the executable without a shell, `PATH` lookup, user configuration, or caller-supplied programmer arguments. The command is restricted to AVR109 / ATmega32U4 and `flash:w:firmware.hex:i` followed by explicit `flash:v:firmware.hex:i` in the same AVRDUDE process; it does not issue EEPROM, fuse, or lock-bit operations. User AVRDUDE configuration is ignored, automatic chip erase is disabled, and the programmer's default verification remains enabled. `-A` retains trailing erased bytes. The staged HEX covers every application flash byte `[0,0x7000)`, filling holes with `0xff`, so explicit verification compares the full application region before the bootloader session ends. It never writes or claims to compare the bootloader region. The process, output, and timeout are bounded.

Before reset/programming the app writes an immutable pre-update backup and a schema 3 pending recovery journal under the application's configuration directory. The journal records `write_may_have_started=false` through reset, enumeration, bootloader selection, and pre-flash validation; it atomically changes to `true` before launching AVRDUDE. A schema 2 journal predates this durable stage and is treated as possibly written. If recovery sees `false`, it may clear the marker only after protocol 3 read-back exactly matches the original firmware version/source, device identity, protocol/settings schema, and settings snapshot. Legacy or unidentified initial-install state cannot prove an unchanged original and remains in recovery. If recovery sees `true`, it requires target firmware version/source, board metadata, identity, and available settings to pass; an unchanged old firmware is insufficient. The backup records the observed identity/settings and package hashes; unavailable fields remain explicitly unavailable. For a verified initial install, the app reports that preservation was not comparable.

Any failure after the reset/programming phase may have started leaves `needs_recovery` set. `recover` performs bounded read-back against the pending plan. It does not retry a flash or treat a reconnect as success. A mismatched identity, settings snapshot, firmware version/source, or board remains in recovery.

## 保管版への切替と復旧

modern→legacyでは既知のID・設定をimmutable backupと接続先に結び付けたretained recordへ残す。legacyには個体設定・個人校正がなく、起動時の既存ゼロ点取得は保管ソースの動作である。設定保持を確認済みとは表示しない。新版への帰還時は同じbindingと退避ID・設定を照合し、成功するまでretained recordを残す。provisionやdefault再設定は更新に含めない。

旧版の成功条件は、対象にbindしたbootloaderでの全application flash verifyと、その後の同じUSB topologyでの有限7ch stream確認（12秒以内に増加timestampの3frame）。状態は`image_verified_legacy_identity_unavailable`、version/ID/settings_preservedはすべてunavailable。vectorは画像や版の証明ではない。設定保持は新版帰還まで未確認で、旧版を新版の補正入力へ利用できない。

schema 3 journalは`legacy_image_verified`を保持する（旧journalではfalse）。全域確認後・stream確認前の中断は、確認済み画像のdurable記録と再接続streamで復旧できる。全域確認が未完了なら、利用者が元の装置を手動bootloader状態に戻した後、固定AVRDUDEの**verify-only**操作で元画像全域を比較する。自動reset・再書込み・EEPROM操作はしない。画像・topology・streamが確認できなければpendingを残す。利用者の承認だけでは消さない。

## 接続所有と計画の失効

モニター、v3 service、updaterは同じRust runtimeの排他下で動く。更新画面を開く操作はmetadata列挙のみ。「受信を停止して更新へ」は明示操作としてsole ownerを解放する。対象候補・系統・package・基板確認の変更はbackendの未開始planを失効させる。applyはplan ID・明示確認・現在topology・ID・設定・画像hashを再照合し、preflight失敗で未開始planを消費する。更新中の操作を後から予約実行せず、operation epochが変わった要求を拒否する。

## Validation boundary

Package parsing, topology decisions, reset transitions, programmer arguments, journal handling, and read-back decisions are tested with scripted backends. Those tests do not open a real serial port, reset a board, or flash firmware. Physical acceptance must separately record board identity, source/application revision, port binding, expected read-back, observed behavior, and a stop procedure in accordance with [hardware safety](../operations/hardware-safety.md). A software build or scripted pass is not hardware acceptance.

## Native USBの情報確認

利用者が指定した対象の更新前検査・更新後確認では、CDC応答を受信するためDTRを有効にして開く。候補列挙ではportを開かない。再接続で機器が再起動し、旧版の起動時ゼロ点取得が発生する可能性は、明示的なtare命令とは別に扱う。未知機器へ`info`/`info3`をfallback送信しない。

## 書込み進捗と終了制御

進捗はupdaterとは別の小さなMutexで保持する読取り専用Snapshot。operation_id、sequence、対象、段階、工程内percent、activeを公開する。UIは選択planと新しいsequenceだけを採用し、古い試行の進捗や完了通知を表示しない。

対象確認、reset、再列挙、書込み、照合、再接続、完了/要復旧を分ける。割合はAVRDUDEのWriting/Reading出力が解析できた工程だけを表示し、未対応出力や不明な工程では不定進捗にする。時間経過から割合を捏造しない。Writing 100%やReading 100%は工程の進捗であり、更新完了は従来の全域verifyと更新後照合の結果だけで決まる。

書込み中はGUIの再書込み・選択変更・ページ切替を止め、nativeウィンドウの通常終了も拒否して説明する。電源断・OS強制終了を防げるとは主張しない。失敗時は復旧journalを保持し、進捗だけで成功や安全な取消を認定しない。
