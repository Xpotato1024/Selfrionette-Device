# 実験用bridge準備のソフトウェア検証

2026-11-10。実験で使用するCLIのsourceとbinaryの取り違えを防ぐため、固定revisionのbuildとserialなしの起動確認を一つの手順にした。runtime、Firmware、校正処理、bridgeの通信契約は変更していない。

## 変更

- `tools/prepare_bridge.py` が指定SHAとclean checkoutを確認し、新しい一時targetへrelease buildする。
- 起動status、要求status、正確なrevision、単調時計、未接続・未準備状態、EOFでの正常終了を確認する。
- sourceを再確認し、コピー後のbinary hashと `Cargo.lock` のhashを `bridge-build.json` へ保存する。既存出力は上書きしない。
- 誤ったrevision、dirty source、既存出力、異常なstatusを拒否する回帰を追加し、既存Host CIで実行する。manifestの最終flushが失敗しても完成markerを公開しない。

## 実行結果

Linux x86_64、Rust/Cargo 1.99.1、固定 `Cargo.lock` で確認した。

| 検証 | 結果 |
|---|---|
| 既存Rust workspace全件 | 192 passed、0 failed |
| 準備scriptの回帰 | 6 passed、0 failed |
| clean checkoutからの実release build | 成功 |
| 生成binaryの起動・status・EOF | status 2件、sample 0件、ready false、exit 0 |
| binaryコピー後のhash照合 | 一致 |
| 差分・UTF-8/BOM/置換文字検査 | 成功 |

実buildの対象sourceはmain `fb21bccc8739bb9ca4647ccd7c0d1762d5c76733`、treeは `e6705b9ae6272803d44530551a56cde5a363580d`。準備scriptはこのsourceとは別の作業checkoutから呼び出した。記録はこのLinux buildを表し、別OSや別sourceから生成したbinaryの同一性を主張しない。

| 項目 | 値 |
|---|---|
| `Cargo.lock` SHA-256 | `c12331e16dc93089feeeebe81ceb415935b9481bfda0d3c613bf92d1e6e09347` |
| binary bytes | 2724504 |
| binary SHA-256 | `a8a93c080c7e03fb52ada99bd5404886ac01ee710a4d45cf9620d3297a8cb3f4` |

## 残る実機確認

serial open、Firmware書込み、EEPROM変更、物理tare、操作者の個人校正、二台接続、実ADCのfresh取得頻度、実験端末での長時間運転は行っていない。`hardware_acceptance` はfalseのまま。実験時は[bridge準備手順](../operations/bridge-build.md)と[レート比較・実機受入](../operations/quality-stream-rate-acceptance.md)を使い、ソフトウェア起動と物理的成立を区別する。
