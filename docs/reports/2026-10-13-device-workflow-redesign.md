---
status: evidence
owner: apps/device-manager
date: 2026-10-13
related:
  - docs/architecture/device-manager-workflow.md
  - docs/contracts/calibration-preview.md
---
# Device Manager操作体系の再構成と検証

## 対象と変更
PR #9のhead 211cbf25a17280f1d328ec1d8eb04f40f78a6155を基点にした後続作業。PR #9のbranchとmainは変更しない。
GUIをデバイス・調整・診断/記録へ再構成し、個体別の調整状態、閲覧専用履歴、明示再開、型付きcommand、対象/epoch/revision照合を追加した。
Tauri+Rust coreを維持し、firmware・serial・ロボット制御・実機の永続設定を変更していない。

## ソフトウェア検証
実施環境: LLM-01 / Windows。Rust 1.94.1、Node.js 24.14.0、既存のMSVC/WebView2を使用した。

| 検証 | 結果 |
|---|---|
| Rust workspace test | 77件 PASS（core 36 / serial 27 / CLI 14） |
| GUI use caseのRust test | 9件 PASS |
| frontend/controller・表示専用adapter | 6件 PASS |
| workspace/application fmt・Clippy全target | PASS、Clippy警告ゼロ |
| Tauri Windows Debug build（bundleなし） | PASS |
| 実Tauri/WebView2/Rust IPC | 9群 PASS |
| UTF-8/BOM/改行・git diff --check | commit前に検査 |

native QAの最終runはapps/device-manager/src-tauri/target/native-qa/run-esLy7E/。
report.json、合成store、通常・調整・履歴・範囲超過・入力エラー・狭幅の画面を保存した。生成物はcommitしない。
通常・調整・狭幅の画面を目視確認し、重複した注意文、常設工程操作、カード装飾を削減した。
実機由来の情報や作成していないモデルのhotspotは表示していない。
入力選択欄は「次に比較する合成入力」と明示し、現在の結果と次の操作値を区別した後、Debug buildとnative 9群を再検証した。

## 回帰試験で確認したこと
| 対象 | 結果 |
|---|---|
| 保存結果の元7ch・補正値・未検証フラグ | 保持 |
| 履歴閲覧の成功・欠落・破損 | active session/承認は不変、失敗時の旧閲覧値は消去 |
| 明示再開・再開失敗 | 旧承認を失効、新基準からの再観測が必要 |
| 左右の表示切替 | 各個体のsession/epoch/承認を独立保持 |
| 旧対象token・要求再送 | 別個体/新状態を変更せず拒否 |
| 不正な変更値・品質不明7条件 | 旧承認・旧候補を使って保存できない |
| 表示範囲超過・未測定逆方向 | unclamped値を保持、確認拒否と再比較操作、未測定はnull |
| frontend同時操作・古い応答・状態取得不能 | 同時要求を抑止、古いviewを棄却、状態不明時は変更停止 |
| 新processでの閲覧 | 証拠だけを表示し、調整・readinessを復元しない |
| キーボード/入力エラー | Enter/Space/矢印、dialog cancel、関連づけたエラー、可視focus |
| 主操作 | 各工程と表示範囲超過の復帰経路で、可視の主操作は1個 |
| reflow | 3画面を320/640 CSS pxで確認、全ページ横はみ出しなし |
| 主要文字色 | サンプルしたbody/p/th/field errorは4.5:1以上 |

## CIと未実施の区別
既存のLinux/Windows workspace CIに、WindowsのGUI unit test/frontend test/Clippy/Debug buildを追加した。
この報告のローカルPASSはGitHub CIの完了を意味しない。CI結果はPRのchecksを確認する。
native画面QAはローカルでありCIに混同しない。試験processだけにloopback CDPと専用store/profileを設定し、自分のwindowを通常終了した。

未実施: 実機接続・配線/荷重方向照合・物理校正・firmware書込み・EEPROM、モデル導入、利用者による操作性評価、スクリーンリーダー、native 200%/Windows DPI、Release/bundle配布。
日常準備と個体校正の実機workflow、複数世代を選べる履歴ブラウザー、利用側への引渡し、保守機構は後続であり、今回の合成実装を完成品とは扱わない。

## Windows CIの一時保存先衝突の修正
PR #10のhead e7e614ab38f06855672cc46a6930e183b3687991で、[PR側のWindows job](https://github.com/Xpotato1024/Selfrionette-Device/actions/runs/38277768957/job/114907340425)がexplicit_resume_and_failed_resume_revoke_confirmationの失敗後に0xc0000409で終了した。同じheadのpush側jobは成功していた。

LLM-01で既存9テストを16スレッドで反復すると、115回目に同じテストのSaveがos error 3（保存先欠落）で失敗した。
一時的な保存先の診断出力を加えた反復では、26回目に別のテストがos error 5で失敗した。
この回はall_quality_failures_block_old_candidatesとcorrupt_history_clears_only_history_not_active_confirmationが、同じPID・同じSystemTimeの値から同一保存先を生成していたことを記録した。
時刻だけではテスト間の一意性を保証できず、一方のDropが他方の保存先を削除する干渉を確認した。診断用の出力は修正から除去している。

修正はworkflow.rsのcfg(test)内だけに限定した。PIDとatomic連番から候補名を作り、create_dirで親ディレクトリを排他的に確保する。
既存名との衝突は最大128回再試行し、その配下の未作成recordsを各テストの保存先とする。未保存時に保存先が存在しないという既存の検証も維持した。
Dropは自分が確保した親だけを削除する。通常の削除失敗は引き続きテスト失敗とし、既にpanic中の場合は二重panicで元の原因を隠さず、削除エラーを出力する。

追加回帰テストは、同一候補連番の衝突と既存記録の保護、16スレッドでの保存先・削除の独立性、後始末の実際の失敗中にも元のpanicを保持することの3件。
GUI use caseは既存9件を含む12件PASS。修正後はWindowsで16スレッドの12件を1,000回反復し、全12,000件PASSした。fmtとapplication Clippy全targetもPASS。
production部分と既存9テストの本文は修正前と一致することを検査した。校正意味論、通常の保存処理、UI、依存、workflow設定は変更していない。
調査・反復ログはdevice-pr10-windows-ciのローカル作業記録に保存し、生成ログはcommitしない。最終headのCIはPR #10のchecksで確認する。

0xc0000409は[Microsoftのfast-fail説明](https://learn.microsoft.com/en-us/cpp/intrinsics/fastfail?view=msvc-170)にも使われる終了コードであり、CIログに例外パラメータがないため、コードだけでスタック破損の詳細を確定しない。ローカルで確認した直接原因はテスト保存先の衝突である。
