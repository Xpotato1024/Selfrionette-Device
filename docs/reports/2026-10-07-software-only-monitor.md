# 2026-10-07 実機なしでのhost monitor実装報告

## 結果

PR #5で導入済みのexplicit serial transportに、期待stable IDの検証後だけsampleを通知する有限monitorを追加した。
既存Protocol v2をそのまま使い、firmware、EEPROM schema、研究値、robot mappingは変更していない。

- library: `DeviceSession::monitor`、typed frame callback、sessionを消費するlifecycle
- CLI: explicit port、期待ID、有限sample数を必須化
- failure: metadata変化、不正frame、非互換、timeout、budget超過、output失敗で停止
- docs: ADR-0007とcanonical sample monitor contract
- fixture: 合成Protocol v2 streamをRust contract testで消費

## 調査の根拠

- mainの基点: `986fa046beedd72a6a0b0224487f3242a61c71c3`
- PR #1〜#5はmerge済み。PR #5のserial基盤は再実装していない。
- repository `AGENTS.md`、SoT map、ADR-0006、protocol/identity/serial/host境界、hardware安全規則を確認した。
- 適用可能なmachine-local編集skillと許可されたCodex memoriesを確認した。
- 当該memoriesにはDevice固有の履歴は見当たらず、現行仕様はrepositoryとmerge済みPRを正とした。
- 保護されたsessionsは読んでいない。
- 既知checkout領域に現行Device checkoutを特定できなかったため、タスク専用clone/topic branchで作業した。

## Software-only validation

LLM-01 Windows、Rust 1.94.1:

- `cargo fmt --check`: PASS
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS
- `cargo test --locked --workspace`: 40 passed、0 failed
- 新規monitor contract test: 14件
- 新規CLI argument test: 3件

既存GitHub ActionsのLinux/Windows matrixで同じ検証を行う。
共有fixtureのみの変更でもhost CIが起動するようpath filterを追加した。
実CIの最終結果はPR checkを正とする。

## Sim側に必要な連携

[sample monitor contract](../contracts/sample-monitor.md)をconsumer境界の正とする。

- DeviceInfoとVectorFrameを同一sessionとして扱う。
- 7ch値へ新しい単位・gainを付与しない。
- device u32 millisとhost clockを直接減算しない。
- warning/statusを独立した診断として扱い、`calibration_end`から成功を推測しない。
- duplicate ID、role binding、freshness/reset判定、健康/校正gate、robot mappingはSim/consumer側で別途扱う。
- CLIのhuman-facing表示をSimのwire protocolとしてparseしない。

Sim repositoryは変更していない。長時間streamingやGUIは今回の有限diagnostic captureをそのままcontrol-readyとして使わず、必要契約を確定して進める。

## 残事項と承認境界

今回のsoftware-only実装に実機承認のblockerはない。
次のoperationは未実施であり、個別承認の対象になる。

| 対象operation | 必要なexact target情報 | 理由 |
|---|---|---|
| CLI monitorの実機受入 | physical device、expected srn ID、OS port、command/sample数 | serial openと`info`送信を伴う |
| firmware upload | physical device、board revision、port、`sparkfun_promicro16`、source revision | flashingとresetを伴う |
| provisioning | physical device、port、新ID、command | EEPROMへpersistent writeする |
| tare/calibration | physical device、ID、port、reference load、手順 | 測定基準を変える。成功契約も未確定 |

target identity/portが未指定なので、実機受入の実行依頼はまだ具体化できない。
tare成功契約、gain/persistent calibration、GUI、複数deviceのrole管理は今後の作業。
merge/deploy、credentials、新規権限、network/security設定、hardware操作は実行していない。
