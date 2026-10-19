---
status: supporting
owner: firmware
date: 2026-10-19
---

# Selfrionette Managed 1.1.0-preview.1 開発候補

これはIssue #24に対するソース変更記録であり、公開済みReleaseではありません。正式なDevice Manager 1.0.0、Managed Firmware 1.0.0、前段の40 Hz Draft PR #23を自動で置き換えません。

## 変更

- 品質付きProtocol 3の送信試行上限を既定80 Hzにし、比較・退避用に40/20 Hz buildを追加しました。
- 比較buildは本体版suffixを変え、同じsource SHAでも保存profileの適合を再判定できます。
- 長い処理・micros rollover・USB送信詰まりで過去のframeを一斉再送しません。queue skipでsequenceを進めず、新規ADC変換のないrawをfreshとして再送しません。
- actual firmware loopをfake hardwareで実行するsoftware試験と、全3環境のFlash/SRAM gateを追加しました。

Protocol 2の80 Hz互換経路、Protocol 3のwire shape・品質・ID/session/tare/settings、Original/Compatible凍結ソースは維持します。runtime rate切替やEEPROMへの個人profile保存は追加しません。

## 検証と利用条件

全3環境のstatic SRAMは1019 B、Flashは既定80 Hzで20564 B、40/20 Hzで20574 Bです。gateはSRAM 1024 B / Flash 24576 Bを維持します。[検証記録](../reports/2026-10-19-quality-stream-80hz.md)。

**最大80 Hzは送信試行の上限です。実際の7ch ADC変換・fresh取得・操作改善は未確認です。** 書込み、EEPROM保持、物理校正、−65536 count impulseの原因・修復もsoftware testからは証明していません。

候補画像の選択は[Firmware README](../../firmware/loadcell_7ch/README.md)、実機評価・20/40 Hzへの退避・既知正式版へのrollbackは[受入手順](../operations/quality-stream-rate-acceptance.md)に従います。正式配布にはclean SHAを注入したbuild、package/hash検証、実機受入と公開承認が必要です。
