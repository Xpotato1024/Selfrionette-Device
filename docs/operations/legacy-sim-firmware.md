---
status: canonical
owner: operations
last_verified: 2026-10-09
canonical_for:
  - legacy Sim firmware ownership and build
related:
  - docs/architecture/repository-boundaries.md
  - docs/contracts/legacy-sim-serial-producer.md
  - docs/operations/hardware-safety.md
---

# Sim由来firmwareの管理とbuild

Selfrionette-Deviceは現行firmwareと旧Sim互換firmwareの両方を所有する。
現行の入口は[Protocol v2 firmware](../../firmware/loadcell_7ch/README.md)であり、
[旧2ターゲット](../../firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/README.md)は
過去のserial sample・実験条件の再現用である。旧コードを現行v2へ上書きしない。

| 用途 | project directory | PlatformIO environment |
|---|---|---|
| 現行Protocol v2 | `firmware/loadcell_7ch` | `pro_micro_7ch` |
| 旧Sim 7ch互換 | `firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_pro_micro` | `pro_micro_7ch` |
| 最初のreference import | `firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy` | `sparkfun_promicro16` |

repository rootからsoftware compileだけを実行する:

```sh
pio run -d firmware/loadcell_7ch -e pro_micro_7ch
pio run -d firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_pro_micro -e pro_micro_7ch
pio run -d firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy -e sparkfun_promicro16
```

CIは全3ターゲットをcompileし、現行v2には既存のstatic RAM 1024 bytes / Flash 24576 bytes
budgetを適用する。旧2ターゲットは凍結した比較用で、build時のFlash/SRAMを記録する。
旧版にはStringや動的allocationがあるため、compile時のstatic SRAMだけでruntime安全性を認定しない。
boardを変えたりresource超過を除くために旧codeを修正したりしない。

HX711.cpp/HX711.hは旧2ターゲット内で同じ版を保持する。各projectを独立buildできることと元sourceの
byte一致を優先し、この移行では共通化しない。現行v2のHX711実装は別版であり、旧版と置換しない。
[third-party notice](../../THIRD_PARTY_NOTICES.md)のMIT attributionを保持する。

## Sim側の利用と採用順

Device側の取り込み・build検証を先に採用し、その後Sim側のfirmware削除PRを採用する。
Device側はfiltered historyを残すmerge commit方式で採用する（squashすると履歴が落ちる）。
両方のPRがDraftの間は採用完了ではない。firmwareをcompileする利用者は本repositoryを別checkoutし、
Simの手順に記されたDevice commitへ固定する。Simの起動・offline parser/fixtureに本repositoryの
clone、submodule、build、serial接続を追加しない。

Simは受信parser、host receipt/health、Input Sourceとrobot control Mappingを所有する。
producer側のpin、sensor acquisition、tare/calibration、identity、EEPROM、firmware buildはDeviceが所有する。
旧producerの仕様は[legacy serial producer](../contracts/legacy-sim-serial-producer.md)、
現行は[Protocol v2](../contracts/firmware-protocol-v2.md)を参照する。
旧vectorの互換だけを現行device identityや管理command対応の保証へ読み替えない。

## 履歴と復元

[移行元](https://github.com/Xpotato1024/Xpotato-Sim/tree/552180b65d2055b63854cd3a25bc1d2e9308fb36/firmware)はSim commit `552180b65d2055b63854cd3a25bc1d2e9308fb36`の15ファイルである。
firmware配下だけをgit subtree splitで抽出し、squashせずDeviceのsubtreeへ取り込んだ。
元commit SHAはpathのroot変更に伴い変わるが、author/date/messageと4件の変更履歴を保持する。
import commit `c13bca48df2cc164d2fc396bb275f12e5e2d6799`のsubtree treeは元tree
`7b44904e9b76a434a9e9532a3ab6075047a60cae`と一致する。文書入口の更新は取り込み後の別commitで行う。

詳細なoriginal/split対応とcompile結果は[移行検証記録](../evidence/2026-10-09-firmware-import.md)を参照する。
元Sim sourceは指定commitから復元でき、import commitは変更前の15ファイルをそのまま保持する。

upload、serial open、EEPROM書込、実機確認は[hardware safety](hardware-safety.md)の別gateである。
本移行では実行しない。
