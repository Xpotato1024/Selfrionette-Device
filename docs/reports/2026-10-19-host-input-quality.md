# 品質付きHost入力の実装・software検証

Issue #25。基準mainは`ba37a3f44a03fa0c7aca8725f2d0240b3e158e26`。

## 変更

- 共通`LiveService`が一度に受けた最大16 frameを元の順序・時刻・品質・補正出力ごと返す。
  従来CLIが最後のframeだけを取り出す経路を全件引渡しへ変更した。
- 受信frame/s、fresh flag付きframe/s、usable率、補正件数、sequence gap、drain上限到達、
  相対backlogを有界に観測し、GUIの診断欄へ表示した。
- 同じ受信clockを注入できるようにし、遅れて読んだ古いdevice時刻が新しいhost時刻だけで
  有効になることを防ぐ。既存250 ms freshness上限を緩めない。
- serial readを1文字ずつから最大256-byte chunkにした。部分行・複数行を1024-byte以内に保持する。
  Firmware、EEPROM形式、個人補正アルゴリズム、GUIの50 ms描画周期は変更していない。

## 検証

Linux local / Rust 1.99.1。固定Cargo.lockを使用した。

- `cargo test --workspace --locked --offline`: **183 tests PASS**。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: PASS。
- `cargo fmt --all --check`、`git diff --check`: PASS。
- `node --test frontend/*.test.mjs`: frontend 10 test files PASS。
- clock/transport fixtureで20/40/80 Hz各2秒、41/81/161 frameを生成し、GUI相当の50 ms待ちを
  含めて同数の補正frameを順序通り取得。元coreの同じprofile/入力列と全command値が一致、gap=0。
- 32-frame burstの16件ずつの処理、不正sequence時の部分成功拒否、290 ms相対backlogの失効、
  sequence/device時刻wrap、部分ch品質不良、256件の観測上限、line境界を追加検証した。

## 未検証

実serial open、Firmware書込み、EEPROM変更、実操作者のtare/neutral/個人校正は未実施。
実二台のCPU負荷、ADC RATEと実効fresh変換/s、絶対的なUSB/OS遅延、Windows native GUIと配布物の
受入は未実施。software fixtureの成功はこれらの成功を意味しない。
GUI単位試験は新しい診断の欠測・表示値を検証するが、Windows上の見た目や操作受入の代替ではない。
