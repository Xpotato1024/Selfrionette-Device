---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-16
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

The application release embeds `distribution/firmware/manifest.json`, `distribution/firmware/firmware.hex`, and a fixed AVRDUDE runtime. The manifest is JSON with exactly these fields:

| Field | Requirement |
|---|---|
| `schema` | `1` |
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

## Preflight and target selection

`prepare` reads the selected Windows port and requests protocol 3 `info3` before the legacy `info` fallback. It verifies supported board metadata when available and snapshots identity and persistent settings. An unidentified or legacy device requires a separate manual board confirmation. An initial install may proceed without an existing device ID or settings snapshot only after that confirmation; the reply states that previous identity/settings could not be compared.

The selected application port must be unique in the current topology. The update plan retains available USB serial and Windows location-path bindings. The displayed COM name is only a current transport address; it is not a durable device identity.

Only the explicit `apply` action begins the 1200-bps USB bootloader touch. The app waits for the application port to disappear and then enumerates bootloader ports. It binds the bootloader to the selected device by USB location path or serial when possible. A known location-path or serial mismatch is contradictory topology and cannot be overridden by manual selection. If at least one binding descriptor is absent, no known descriptor conflicts, and exactly one bootloader candidate exists, the UI must obtain a second explicit operator confirmation naming that candidate. Duplicate or ambiguous candidates, unknown topology, and mismatched bindings fail closed and require recovery; the app never silently reuses the old COM number.

## Flash operation

The updater runs only the packaged AVRDUDE v8.3 Windows x64 executable and configuration. It passes a fixed argument set directly to the executable without a shell, `PATH` lookup, user configuration, or caller-supplied programmer arguments. The command is restricted to AVR109 / ATmega32U4 and `flash:w:firmware.hex:i`; it does not issue EEPROM, fuse, or lock-bit operations. User AVRDUDE configuration is ignored, automatic chip erase is disabled, and the programmer's default verification remains enabled. The process, output, and timeout are bounded.

Before reset/programming the app writes an immutable pre-update backup and a schema 3 pending recovery journal under the application's configuration directory. The journal records `write_may_have_started=false` through reset, enumeration, bootloader selection, and pre-flash validation; it atomically changes to `true` before launching AVRDUDE. A schema 2 journal predates this durable stage and is treated as possibly written. If recovery sees `false`, it may clear the marker only after protocol 3 read-back exactly matches the original firmware version/source, device identity, protocol/settings schema, and settings snapshot. Legacy or unidentified initial-install state cannot prove an unchanged original and remains in recovery. If recovery sees `true`, it requires target firmware version/source, board metadata, identity, and available settings to pass; an unchanged old firmware is insufficient. The backup records the observed identity/settings and package hashes; unavailable fields remain explicitly unavailable. For a verified initial install, the app reports that preservation was not comparable.

Any failure after the reset/programming phase may have started leaves `needs_recovery` set. `recover` performs bounded read-back against the pending plan. It does not retry a flash or treat a reconnect as success. A mismatched identity, settings snapshot, firmware version/source, or board remains in recovery.

## Validation boundary

Package parsing, topology decisions, reset transitions, programmer arguments, journal handling, and read-back decisions are tested with scripted backends. Those tests do not open a real serial port, reset a board, or flash firmware. Physical acceptance must separately record board identity, source/application revision, port binding, expected read-back, observed behavior, and a stop procedure in accordance with [hardware safety](../operations/hardware-safety.md). A software build or scripted pass is not hardware acceptance.
