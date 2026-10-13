---
status: proposed
owner: architecture
date: 2026-10-13
related:
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
  - docs/contracts/device-persistent-settings.md
  - docs/contracts/firmware-update.md
  - docs/architecture/device-manager-workflow.md
---
# ADR-0009: 装置固有補正をデバイスに保持し、管理GUIを通常利用の必須経路にしない

## 背景
個体の組立・配線に由来する極性と、利用者の操作範囲やロボットの運動方向が同じホストprofileに混在すると、GUIを終了した場合や別PCへ移した場合に同じ入力を得られない。
PR #9/#10の合成previewは安全な観測・状態管理の試作であり、実機の永続設定そのものではない。

## 採用する設計
- 同じboard系統は共通Firmwareを使用し、個体ID・組立revision・chごとの極性を別の不揮発設定として保持する。
- 装置固有補正はFirmwareで一度だけ適用する。Device Managerは構成、校正、診断、更新の操作窓口であり、通常入力の必須中継processではない。
- 利用先はversioned protocolと、設定・tare・取得品質の有効性を確認する。任意のserial値を無条件にロボット制御へ渡してよいという意味ではない。
- 個体設定はEEPROM、今回のtareはRAM、利用者の操作範囲はホストの利用者/session設定、ロボット写像・速度gainは利用先に属する。
- 形状の左右は組立情報、左右入力や左右腕の役割は利用先のbinding。個体IDへ役割を埋め込まない。
- ch順は保持する。正方向の定義、物理位置、感度軸・荷重経路の根拠がないものを符号だけで補わない。
- GUI/CLIは同じ管理use caseを利用する。常駐daemonは追加しない。利用先と管理側のserial同時所有は初期要件にしない。

## 検討した代替案
| 案 | 不採用または限定利用の理由 |
|---|---|
| Managerに保存して補正・中継 | PCと常駐processへ装置の真実が依存する。合成preview、研究上の試行だけに限定 |
| EEPROMに保存し利用先が補正 | Managerは不要だが、各consumerの補正実装差・二重補正を招く。物理補正はFirmwareに集約 |
| 左用・右用の専用Firmware | 組立設定をprogram差分へ混ぜ、交換や保守で誤書込みしやすい。board差と個体設定を分ける |
| 毎起動で極性を自動推定 | 正方向・荷重条件の根拠を得られず、使用中の入力から勝手に設定が変わるため不採用 |

## 互換性と実装順
既存v2のvectorは全ch negation/tare/filter後の値である。新しいADC基準の補正値へ意味だけを置き換えない。
新しい出力domainと設定revision・品質を識別できる別contractを追加してから、consumerを移行する。
現在のv2 Firmware、合成record形式、実機I/Oの許可境界はこのADRだけでは変更しない。

1. 保存contractと純粋なjournal実装・電源断模型の試験。
2. staging/readback/commitと新出力contract、Firmware内適用、host parserの実装。
3. 共通管理use caseのGUI/CLI接続、更新plan/preflight/実行backend。
4. 人が確認した個体・board・配線・荷重条件での実機受入。

## 完成の条件
Managerを起動していない別PCから直接接続し、再起動後も装置固有極性が一度だけ適用される。
破損・未確認設定、tare未確認、取得異常を正常入力と区別する。更新後にID・設定を読戻し、旧sessionを自動復元しない。
実機未実施のsoftware testは、この完成条件を満たしたという証拠にはしない。
