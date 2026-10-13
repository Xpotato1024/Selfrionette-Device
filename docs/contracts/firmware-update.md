---
status: draft
owner: device-management
last_verified: 2026-10-13
related:
  - docs/decisions/0009-standalone-device-configuration.md
  - docs/contracts/device-persistent-settings.md
  - docs/operations/hardware-safety.md
---
# Firmware更新契約

## 目的と現在の実装範囲
Firmware更新はDevice Manager/CLIの必須の製品機能とする。通常入力のためにManagerを常駐させる理由にはしない。
本書は実装前のcontract。現時点でGUIに実機書込みボタン、serial/bootloaderへの接続、更新backendはない。未実装を成功するモックで埋めない。

Firmwareの更新、装置設定のEEPROM保存、今回のtare、PC上の証拠保存はそれぞれ別操作・別状態である。

## 公開する作業手順
横の「保守」からFirmware更新を選択し、縦の手順は次の順序にする。
1. 対象確認: stable ID、board/revision、稼働Firmware/protocol、接続所有者を確認。
2. 更新ファイル: source commitとbuild条件に結び付いたpackageを選択し、対応board・容量・hashを検査。
3. 設定の保護: 現在のID・設定・schemaを退避、migration可否と保持方針を確認。
4. 書込み: 対象と変更内容を再確認し、明示操作でのみ実行。
5. 再接続・確認: Firmware/protocol/ID/設定revisionを読戻す。

ファイル検査の通過、programmerのexit code 0、読戻し成功、使用前調整完了は別状態。更新後に自動でロボット操作へ復帰しない。
更新中の離脱・取消は段階ごとに許可する。erase/write中に強制終了するCancelを用意せず、停止可能な境界と復旧状態を表示する。

## 更新package
manifest schema、対応boardとclock、MCU、application address range、Firmware/protocol version、設定schemaの読込/書込範囲、migration policy、source commit、toolchain/build条件、image SHA-256を必須にする。
今回のboard候補は既存buildのsparkfun_promicro16。board実機/bootloaderが未確認なら推測で更新可能にしない。
Intel HEXはchecksum、address overlap、EOF、実application領域を検査し、bootloader領域・Fuse・EEPROM書込みを通常更新packageに含めない。
SHA-256の一致は破損検出であり、manifestの真正性を単独では証明しない。信頼する配布元・明示選択・署名の方針を別に固定する。
開発時のbuild-and-flashと通常利用者のpackage更新を分け、利用者にsource/toolchainの導入を必須にしない。

## 接続と対象の同定
利用先とDevice Managerは同時にserialを所有しない。利用先停止/解放をpreflightに含める。
通常modeからbootloaderへ移る際にCOM番号が変わり得る。USB topology、操作前の対象情報、他deviceの一覧、再列挙イベントを追跡する。
bootloaderがstable IDを返さない場合、その制約を補う対象確認方法を実機で検証する。候補が複数なら自動選択せず停止し、別個体へretryしない。
USB reset/1200 baud touchもhardware操作であり、ファイル検査や画面閲覧だけで実行しない。

## 永続設定と失敗
通常更新はidentityと設定を保持する設計にする。ただし書込み方式やEESAVE/bootloaderに依存するため、保持を無検証で保証しない。
backupはID・schema・hash・取得時点を記録する。別個体・不適合schema・古いrevisionへ自動復元しない。
新Firmwareで読めないschemaならmigrationが明示されない限りpreflight拒否。down-gradeも同じ。
書込み成功後にID/設定が一致しなければ「要復旧」とし、更新済み・使用可能という緑表示にしない。
bootloader修復、Fuse変更、個体IDの再発行は通常更新から分離した保守手順にする。

## backend境界
GUIとCLIはtypedなUpdatePlan/UpdateStateを共有する。自由なshell文字列をGUIから受け取らない。
許可したprogrammerと引数、有限timeout/出力量、対応targetと確定imageだけを実行する。任意command・任意memoryへのwriteは公開しない。
progressは実際の段階から作り、時間だけで100%へ到達させない。失敗・切断・復旧が必要な状態を最初から実装する。

## software-onlyから実機へ
最初に偽transport/programmerで対象変化、二台接続、失敗exit、読戻し不一致、backup不適合、途中停止、取消可能境界を試験する。
その後、明示許可された一台でboard/bootloader/Fuse、EEPROM保持、ポート再列挙、電源断復旧を受入。
最終受入はManagerを終了して電源を入れ直し、別PCのconsumerが設定・出力domain・品質を確認して直接利用できること。

## 一次資料
- AVRDUDEのerase/writeオプション: https://avrdudes.github.io/avrdude/8.3/avrdude_4.html
- AVRDUDEのeraseとEESAVE: https://avrdudes.github.io/avrdude/8.1/avrdude_9.html
- Pro Microのbootloader/port: https://learn.sparkfun.com/tutorials/pro-micro--fio-v3-hookup-guide/all
- MCU: https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-7766-8-bit-AVR-ATmega16U4-32U4_Datasheet.pdf
