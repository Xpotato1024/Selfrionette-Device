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

## Windows native CIで見つかった模擬producer時計の補正

公開Host PR #28の`9d131d2ad48b5015a7fc4f792eac045a06d673ea`と、
その上のbridge PR #29の`dd828017e8ae328b4d4bc09b78653fc12d57a95f`で、
WindowsのRust native単体78 testsとdebug buildは通ったが、
`live-native-qa.mjs`の新sample待機が5秒で失敗した。
失敗したjobは[Host #28](https://github.com/Xpotato1024/Selfrionette-Device/actions/runs/39182650091/job/117838534849)と
[bridge #29](https://github.com/Xpotato1024/Selfrionette-Device/actions/runs/39182681824/job/117838642764)。

原因は開発者用`ScriptedTransport`の`device_ms = sequence * 50`だった。
このtransportは読取り時に新sampleを生成するため、GUI/IPCにかかった実時間を固定周期の
device時計から除くと、共通Hostがその差を蓄積したbacklogとして正しく検出する。
変更前の実transportと`LiveService`を85 ms間隔で読むsoftware再現では、相対backlogが
0→35→70→105→141→176→211→246 msと増え、9回目・経過682 msで250 ms上限により失効した。
これは当該再現1回の観測値であり、実deviceやWindowsの遅延測定ではない。

修正はこのdeveloper-only経路のproducer時計だけとし、生成時の実monotonic経過時間を使う。
最短50 msの生成間隔、元sequence/session、品質flag、補正処理、実Hostの250 ms上限は維持する。
新しい依存はなく、既存`MonotonicClock`をテストでも共有する。

補正後のLinux local / Rust 1.99.1、固定Cargo.lockでの検証:

- 実`scripted.rs`を`rustc --test`で直接コンパイルし、当該Host checkoutから
  `cargo build --locked --offline -p selfrionette-serial`で生成した共通libraryをリンク。
  追加2 tests PASS。native GUIの別実装やstatic fixtureを代用していない。
- 同じ時計で実transportと`LiveService`を動かし、0/85/170/255/600/685/1000 msの各時点に
  1 sampleずつ受信。sleepを使わず250 ms超の待ちを含め、相対backlogはすべて0、
  偽の失効と追いつき用の重複frameがないことを確認した。
- 実parserを通して50 ms未満で未生成、32-bit時計wrap、session変更時のsequence resetと
  producer時計の継続を確認した。
- 既存`host_backlog_cannot_refresh_old_samples_by_reading_them_late`を再実行しPASS。
  本当に290 ms古いsource時刻は引き続き失効する。
- 対象moduleの`clippy-driver --test … -D warnings`、`rustfmt --check`、`git diff --check`: PASS。

補正後のWindows実WebView/GUIと配布物は、公開した新HEADの既存CI gatesで確認する。
このlocal検証をWindows受入の完了とは扱わない。実serial・実機操作は行っていない。
変更はsoftware受入用の生成時計に限られ、実ADC時刻、個人校正、実験条件、解析式を変更しないため、
研究結果や物理性能に関する結論の追加・更新はない。
