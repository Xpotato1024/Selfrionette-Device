---
status: canonical
owner: host
last_verified: 2026-11-10
canonical_for:
  - reproducible local bridge preparation without serial access
related:
  - docs/contracts/host-input-bridge.md
  - docs/operations/normalized-input-consumer.md
---

# 実験用bridgeを準備する

`tools/prepare_bridge.py` は、指定した正確なcommitのCLIをbuildし、起動時のrevisionと正常終了を確認します。USB接続、個体設定、校正、Firmware書込みは行いません。実機用profileや接続設定は別に用意します。

## 実行

Python 3.10以降、Git、Rust/CargoとOSのコンパイラが必要です。ソースを指定commitへ合わせ、変更がない状態で実行します。`--revision` は採用する40桁のcommit SHAを明示してください。

```text
python tools/prepare_bridge.py --revision <40桁のcommit_SHA> --output ../experiment-bridge
```

PowerShellでも同じ引数を使います。既に存在する出力先とソース内の出力先は拒否します。別のclean checkoutをbuildする場合は `--source <checkoutのpath>` を指定できます。

処理は次の順で進みます。

1. HEADと指定SHA、未commit/未追跡の変更、`Cargo.lock` を確認する。
2. 新しい一時build先で `cargo build --locked --release -p selfrionettectl` を実行する。revisionはscriptが環境変数へ設定する。
3. 生成したbinaryへ `bridge` の `status` 要求を1件だけ送り、stdinを閉じる。起動statusと要求statusの2件、revision、単調時計、未接続状態、正常終了を確認する。
4. build後にソースを再確認し、binaryを新しい出力先へコピーする。コピー後のSHA-256を照合し、manifestの一時fileをflush・fsync・read-backしてから、最後に `bridge-build.json` として排他的に公開する。

`configure` と `connect` は送りません。成功時も `ready: false`、`serial_opened: false`、`hardware_acceptance: false` です。検証失敗時は完成manifestを作成しません。途中で残った出力先を完成品として利用せず、原因を確認して新しい出力先でやり直してください。

## 実験条件への引渡し

出力先には `selfrionettectl`（Windowsは `selfrionettectl.exe`）と `bridge-build.json` が入ります。manifestにはsource commit/tree、`Cargo.lock` のSHA-256、Rust/Cargo版、binary名・サイズ・SHA-256、status検証結果が記録されます。

Xpotato-Sim側では、接続設定の `executable` にこのbinaryの絶対path、入力条件の `bridge_revision` にmanifestの同名値を指定します。別のcommitで作ったbinaryへ置き換えた場合は、条件のrevisionも明示的に変更して再確認します。Firmwareの `source_sha` は別項目です。bridgeのrevisionをFirmwareの値へコピーしません。

実機の個体setup recordと個人profileは元のstore配置を保ちます。接続後の無荷重tare、自然姿勢でのneutral、profile適用は[通常利用手順](normalized-input-consumer.md)と[bridge契約](../contracts/host-input-bridge.md)に従って個別に確認します。

この準備はbuildとprocess起動の確認です。実機の校正精度、ADCのfresh取得頻度、USB遅延、二台接続、長時間性能、操作感は実験端末と対象装置で確認します。binary hashやbuild stampは署名・物理受入の証明ではありません。
