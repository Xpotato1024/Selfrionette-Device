# PR #19 CI回帰修正 — ローカルsoftware検証

対象: `3d147162e77495c391411908a1bb22285b52b159`の既存clean worktreeからの未commit差分。`change_class=semantic`。入力範囲は既存GUI/IPC、v3品質stream、共有LiveServiceと既存fitter。primaryのreview・push・GitHub CI待ちであり、CI成功や配布受入は未確認。

## 修正

- Desktop回帰はfixtureの`connected`直接変更をやめ、実画面の候補選択→receive-only接続確認→IPC replyを通す。接続中のsource切替拒否、read中に一度だけ実行する切断を保持・強化した。未接続時のidle scan/readは追加していない。
- LiveServiceは同一contextの通常の不良sampleでruntime/output/applied context/session neutralだけを失効し、完了・独立検証済みheld-outとfitを保持する。良いsampleだけでは復旧せず、新neutral完了→明示applyを要求する。修正前の回帰テストで証拠消失を再現した。
- evidence再取得は開始時に合格状態と対象phase以降を失効する。不良sampleで部分captureを中断・archiveし、held-outの全4phaseとneutralの反復失敗が部分証拠を認証しないことを確認した。新training/fit、setup/operator/Firmware/ID変更、generation/tare/session変更、fatal protocol、切断の失効を維持する。
- GUIは現在のservice検証結果・選択method・neutral receiptに応じた次操作を示し、不可能なneutral/save/applyを無効化する。native E2Eのbadquality stepを残し、証拠保持・自動復帰なし・reloadなしの明示復旧をassertする。

## 検証

既存`../validate-software.cmd`と同じcommand-local VS2022/stable Rust環境を使用。global toolの追加・設定変更なし。cache/build/test出力は許可されたtarget配下へ保持。

| 検証 | 結果 |
|---|---|
| `cargo +stable test --locked --workspace` | 176 passed、0 failed、0 ignored |
| workspace `fmt --all --check` / `clippy --locked --workspace --all-targets -- -D warnings` | PASS |
| Tauri `cargo +stable test --locked --manifest-path apps/device-manager/src-tauri/Cargo.toml --lib` | 74 passed、0 failed、0 ignored |
| Tauri `fmt --check` / `clippy --all-targets -- -D warnings` | PASS |
| Tauri `cargo +stable build --locked --manifest-path apps/device-manager/src-tauri/Cargo.toml` | PASS、起動なし |
| Node frontend全suite + `tools/ci-scope.test.mjs`（`--test-isolation=none`） | 51 passed、0 failed、0 skipped |
| 同上 + distribution/firmware-packages全suite | 86 passed / 90 total、4 failed、0 skipped |
| 変更したdesktop/native QA scriptの`node --check`、`git diff --check` | PASS |

Nodeの4失敗はZIP round trip、Windows metadata module isolation、release staging、inherited Path child。既存の子process実行が`spawnSync powershell.exe/node.exe EPERM`で拒否される環境制限に対応し、assertionやskipを変更していない。通常のNode test isolationも`spawn EPERM`のため、pure suiteはCLIの`--test-isolation=none`で実行した。詳細: `target/pr19-ci-recovery/node-validation.log`。Rust最終結果: `workspace-final.log`、`native-final.log`（同directory）。

## 変更file

- `crates/selfrionette-serial/src/app.rs`
- `apps/device-manager/frontend/application-ui.mjs`
- `apps/device-manager/frontend/application-ui.test.mjs`
- `apps/device-manager/frontend/hardware-controller.test.mjs`
- `apps/device-manager/tools/desktop-ui-test.mjs`
- `apps/device-manager/tools/live-native-qa.mjs`
- `docs/contracts/input-calibration.md`
- `docs/contracts/device-manager-session.md`
- `docs/reports/2026-10-17-pr19-ci-recovery.md`

## 未実施と保持物

描画desktop UI・native WebView E2E・exact-head GitHub CIはNot Run。以前のローカルGUI/Edge起動拒否の境界を保持し、再起動や別経路での迂回はしていない。独立reviewはprimaryが担当する。hardware/serial/reset/flash、credentials、commit/push/merge/release、rebase、追加agentは実施していない。

Workspace hygiene: PASS（利用者指定のtarget/cache保持、cleanupなし）。Remaining task-created non-deliverable paths: 0。Temporary root: not created。意図した残存pathは上記9変更fileと、許可された`target/`内のbuild/test/cache/log、`apps/device-manager/src-tauri/target/`と既存`apps/device-manager/src-tauri/gen/`の生成物。既存untracked/ignored・親directoryのfileは変更していない。

## Primaryの統合確認

修正担当とは別の通常実行環境で、最終ソースのworkspace176件、Tauri74件、frontend44件、配布39件、CI scope7件を実行し、すべてPASS、失敗・skipなし。workspaceとTauriのfmt/Clippy（warnings拒否）もPASSした。上記EPERMは担当環境の履歴であり、この再実行では子processを含む配布試験も省略していない。ローカルGUIは起動していない。

独立read-onlyレビューは変更8ファイルのコード・契約にP0/P1/P2指摘なし。回帰の追加確認後、最終コードでPrimaryの一括検証を再実行した。本記録とRelease本文の追記はその後の文書変更のみで、コードは変更していない。GitHubの描画・native通し試験・配布検証はpush後に別途確認する。公開済みタグと配布物は変更しない。
