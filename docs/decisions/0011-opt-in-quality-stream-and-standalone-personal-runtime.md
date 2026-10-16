---
status: proposed
owner: architecture
date: 2026-10-16
related:
  - docs/decisions/0003-versioned-ascii-protocol-v2.md
  - docs/contracts/firmware-protocol-v3.md
  - docs/architecture/required-setup-calibration.md
---
# ADR-0011: v2を維持し、品質付きstreamと個人runtimeを明示選択で追加する

## 背景

v2 vectorはnegation/tare/spike抑制後で、timeout時に前値を保持し得る。
品質・tare結果・settings generationを持たないため、rawを必要とする個体確認や
個人校正へ品質既知として取り込めない。既存consumerの値の意味を変更すると互換性を破る。

## 判断

v2の既存command/frameは残し、明示的なinfo3/stream3で品質付きprotocolへ切り替える。
wire順のrawと装置内で一度だけ極性/tareを適用したorientedを区別し、logical permutationはhostが所有する。
request/session/sequence/tare epoch/settings generationを照合し、品質不明や状態変化は通常入力を失効させる。
同一sessionの一時停止・再開と、新しいconnection challengeを区別する。

個体の最小設定は既存EEPROM journalへ明示保存し、identity配置を維持する。
ラベル・方向確認の根拠・論理対応はPCにidentity/revisionを結び付けて保存する。
個人span/noise/filter・採用method・観測/held-out根拠は永久profile、今回のneutralとreadyはvolatileとする。
共通Rust serviceと既存fitter/runtimeをGUI/CLIで共有し、Managerの常駐やCLI subprocessを要求しない。

更新は固定の配布programmerと検証済みpackageだけを使い、明示確認後のreset/書込みと、
USB対象同定・ID/設定/実行Firmware読戻しを分離する。exit 0だけではreadyにしない。

## 帰結と限界

新機能には対応Firmwareが必要。旧Firmware初回導入は実boardの独立確認を伴う別routeとする。
取得品質と手順閾値の合格は物理精度の認証ではない。既知荷重のN係数やlive board受入は別に行う。
robot mapping/kinematics/controlは引き続きXpotato-Simが所有する。
この記録は実機操作やrelease公開の権限を追加しない。
