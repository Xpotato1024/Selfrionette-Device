---
status: canonical
owner: firmware
last_verified: 2026-10-16
related:
  - docs/contracts/device-identity.md
  - docs/contracts/firmware-memory-budget.md
  - docs/decisions/0009-standalone-device-configuration.md
---
# 装置固有設定の保存・適用契約

## 状態と適用範囲
このcontractはfirmware protocol v3のdevice settings journalを定義する。合成previewのPC保存recordとは別物。
firmwareのsoftware implementationはboundedなC++ journalとEEPROM adapterで構成する。実機への接続・書込み、配線/正方向確認、物理極性の受入は別途のhardware validationである。

## 値の意味
補正前domain 1はHX711由来の符号付き24-bit ADC count。旧v2の一律negation後の値をdomain 1として再利用しない。
各chについて oriented_count = polarity * (adc_count - session_zero_count)。単位はcountでありNでも無次元操作量でもない。
polarityは既知方向を人が確認したchだけ±1を持つ。未確認chには暗黙の+1を割り当てない。
ch順0〜6を保持し、組立revisionは物理構成・確認記録を引く識別子であってhost roleではない。
ゼロ点・利用者span・ロボットmappingはこのrecordに保存しない。

## EEPROM配置（schema 1）
既存identity領域0..31は変更しない。slot A=32..95、slot B=96..159。各64 bytes、残りは予約。
C++ structのpadding/endianをformatにしない。複数byte数値は明示little-endian。

| Offset | Bytes | 内容 |
|---:|---:|---|
| 0 | 4 | magic SRNC |
| 4 | 1 | schema=1 |
| 5 | 1 | record length=64 |
| 6 | 1 | channel count=7 |
| 7 | 1 | sample domain=1 |
| 8 | 4 | generation（1以上、wrap禁止） |
| 12 | 4 | assembly revision key（0は未設定） |
| 16 | 16 | 対象device identity payload（all-zero禁止） |
| 32 | 1 | known mask（下位7bit。操作者が極性を確認したch） |
| 33 | 1 | negative mask（knownのsubset。bit=1は極性-1） |
| 34 | 27 | reserved=0 |
| 61 | 2 | CRC-16/CCITT-FALSE、bytes 0..60を対象、little-endian格納 |
| 63 | 1 | commit marker 0xA5。書込み開始時0、最後に0xA5 |

CRCは破損検出のみ。物理校正、真正性、正しい配線を証明しない。known maskも実測証拠の代替ではない。

## 読込み
- identity、schema、予約値、CRC、mask、generationを検証する。
- marker=0の明示未commit領域と、全byteが0xFFの消去済み領域だけを非activeとして扱う。他のmarkerやpayloadがある0xFF markerはCorruptとして拒否する。
- commit済みだが検証不合格のslotがあれば、他方が読めてもCorruptとして使用を止める。過去設定へ黙ってfallbackしない。
- 両方validなら大きいgenerationを選ぶ。同一generationはAmbiguousとして拒否する。
- 選択recordのknown maskが0x7FでなければPartial。全chを使用可能と扱わない。
- 設定がUsableでもsession tare/取得品質が有効という意味ではない。

## 変更transaction
1. expected generationと現在のrecordを照合する。古い要求、破損状態、generation上限は書込み前に拒否。
2. 対象ID・assembly・maskを検証する。保存内容が同一ならUnchangedとして書込み0回。
3. 現在activeではないslotのmarkerを0へ変更。
4. 新しいgenerationのpayloadとCRCを書く。
5. marker以外を読戻して一致確認。
6. markerを最後に0xA5へ変更。
7. 両slotを再読込みし、新recordの完全一致を確認して初めてSavedとする。

任意のupdateで失敗したら成功を返さない。旧active slotは変更しない。
実機adapterはEEPROM.update相当の有限処理を使用する。保存は明示操作のみで、毎sample/毎起動では実行しない。
firmwareはv3 settings write中にtareを開始させず、commit/readback後にgenerationを応答する。実変更ではsession tareを失効する。

## 受入
format golden vector、部分確認、ID不一致、未知schema、CRC破損、同世代競合、generation上限、same-value no-write、stale generation、全byte更新境界の中断から再起動、identity領域不変を試験する。
電源断試験はbyte store模型によるもの。実EEPROMのbrownout/byte破損、Fuse/BOD、耐久、物理極性の受入は別。

## 根拠
EEPROMの既存領域・容量・budgetはfirmware-memory-budget.mdとdevice-identity.mdが正本。
製造者資料: https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-7766-8-bit-AVR-ATmega16U4-32U4_Datasheet.pdf
