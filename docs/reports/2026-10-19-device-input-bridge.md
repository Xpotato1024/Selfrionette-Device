# ローカル正規化入力bridgeのsoftware検証

Xpotato-Sim #631のDevice側bridge。基準はHost #25の`ef05f07`。
一台の明示ID・portを一つのCLI processへ束縛し、既存`LiveService`をversioned JSON Linesで呼び出す。
sourceの正本は[bridge契約](../contracts/host-input-bridge.md)。

## 実装と判断

- configure/statusではserialを開かず、connect/tare/neutral/applyを別々の明示確認にした。
  無荷重tare後はproducerを止めて自然姿勢の確認を待ち、取得barrier後の16 fresh sampleを使う。
- setup/profile/held-out再検証、極性の扱い、neutral、filter、deadzoneは共通serviceのまま。
  loaded profileのmethod表示は実際に検証したmethodを返すようにした。
- 元frameのsequence/device time/receipt、profile/source/operator/domain/epoch、品質と補正出力を保持する。
  空pollとcontrol/statusはsampleを再生成しない。
- stdin 64 KiB/8 command、stdout 32 eventと二つのpipe workerで、blocked consumerがserial ownerを
  無期限停止させない構成とした。上限、EOF、不正要求、品質失効は切断へ進む。
- 全eventをwriter直前の同じ単調時計でstampする。sampleはqueue滞留もageへ含め、250 ms超を欠測へ変える。
  request送信とstatus writer時刻を対応させる保守的なprocess間clock mappingを契約にした。
- bridge exact revisionはbuild変数由来。未指定buildの実live connectを拒否する。

## 検証

Linux local / Rust 1.99.1 / 固定Cargo.lock。すべてsoftware-only。

- `cargo test --workspace --locked --offline`: **192 tests PASS**。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: PASS。
- `cargo fmt --all --check`、`git diff --check`: PASS。
- 実際に生成・保存したsetup/profileをscripted Protocol 3 transportでloadし、
  configure→connect→tare→neutral→apply→ready→startから80 frameを順番通り配信。
  command値、元sequence、設定/tare epoch、provenance、sample JSON fixtureのfield集合を照合した。
- coupled profileを保存・再読込みして、公開metadataのmethodがdiagonalへ戻らないことを確認した。
- unknown build revision/未確認connect/wrong device ID、duplicate/unknown/malformed command、
  bad quality、writer時点のage超過、Stop後の再準備必須、line/queue上限を検証した。
- CLI subprocessではserial未設定のstatus request echo、単調emission時刻、EOF終了、
  64 KiB超・不正要求のtyped errorと非zero exitを検証した。

## 未実施・残る受入

実serial open、Firmware書込み、EEPROM変更、実操作者のtare/neutral/個人校正、二台接続とrobot運動は未実施。
Windows配布binary・OS固有pipe/serial挙動の実機受入は未実施。
ADC取得から最初のhost受信までの絶対latencyはこのclock対応では測定できない。
Xpotato-Sim側のInput Source/TrialRunner/Workbenchは別リポジトリの変更であり、この報告だけで統合受入を宣言しない。
