---
status: proposed
owner: architecture
date: 2026-10-16
related:
  - docs/decisions/0009-standalone-device-configuration.md
  - docs/architecture/required-setup-calibration.md
  - docs/contracts/input-calibration.md
---

# ADR-0010: 個体設定と個人校正を両方必須とし、domain/contextを固定して段階導入する

## 背景

個体ごとの逆向き取付・配線・荷重経路と、人ごとの姿勢・握り・快適な正負操作範囲は責務が異なる。
符号だけの補正、合成fixture、品質不明のv2受信、GUIなしのbackend完成では必要な操作入力を保証できない。
ADR-0009案のManager常駐不要方針を維持しながら、個人校正を任意改善として先送りしない決定が必要である。

## 決定案

- 個体setupと操作者/posture/gripごとの個人校正を、正常入力に必要な二つのworkflowにする。
- raw ADC、旧v2 processed vector、device-oriented count、N、normalized inputを区別する。
  unknown極性を+1にせず、取付変更では全mapping/方向を再検証する。未装着tip由来の暫定指対応を既定値にしない。
- 個人profileをexact ID、assembly/config revision、settings generation、Firmware/source/domain、操作者条件に固定する。
  設定変更時は要再検証、tare/reconnect時は新neutral。互換な永久spanとvolatile neutralを分ける。
- 既存Rust calibration/input_calibration/persistenceを再利用し、最初はpure coreとfile-only CLIへ結線する。
  schema検査・synthetic生成を物理品質や制御可能性の証明にしない。今回のsliceはparameter保存/実適用を含まない。
- PR16完了 → 共通契約 → Firmware取得/info/settings → application管理/updater → 実個人校正GUIの順で実装する。
  GUIは一step一指/方向、user-ready開始、欠測を保つretry/skip/abort、観測/候補比較、理由、明示適用を必要とする。
- Pro Micro C++/AVRとRust/Tauriを維持する。通常入力はManagerなしでconsumerが共有libraryを利用する。
  既存v2/EEPROM形式、identity領域はこのsliceで変更しない。daemon/第二stackを設けない。

## 代替案と帰結

単一のsign/span設定では、配線入替え・未知の荷重経路・個人差とtare寿命を表現できない。
Manager常駐の補正中継は装置設定とconsumerを管理GUIへ依存させるため採用しない。
基準荷重によるN校正を全工程の前提にする案も、まず必要な方向確認と個人操作適合を遅らせるため採用しない。

実装の詳細・schema・後続gateは[必須setup/個人校正の正本](../architecture/required-setup-calibration.md)に集約する。
このADR案は実機操作を許可せず、Accepted扱いにも実機受入済みにもしない。
