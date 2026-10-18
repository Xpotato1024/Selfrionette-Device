# Device Manager 1.0.0の実装・検証記録

## 変更範囲

Issue #20。基準mainはef2ceecd175af6b2d95bd563b4d896f5837e56f4。Firmware三系統の配布識別、アプリ/本体Firmware/Protocol表示の分離、更新工程の進捗、正式タグの公開経路、研究室内共有ガイドを追加した。

OriginalとCompatibleの元ソースは無変更。各1.0.0は今回定義した配布識別版である。Managed Firmware版はアプリ版とは別のeditions.jsonから注入する。

旧系統のlegacy_7chという公開名に対し、Rustの自動snake_caseがlegacy7chを生成していた不整合を明示renameで修正した。旧journalの綴りは読取りaliasとして保持し、新規出力の名前を統一した。

## ローカルで確認したこと

Windows上、Rust 1.94.1 / Node.js 24.14.0で実施。

- Rust workspace: 176 tests passed。
- Native GUI/backend: 78 tests passed。三系統metadataの不一致拒否、両旧版への切替とManagedへの復帰、元設定の保持判定、進捗parser/寿命を含む。
- workspace/native clippyは警告をエラーとして成功。fmtとnative Debug buildが成功。
- frontendと配布用Node tests: 90 tests passed。正式/previewのRelease分類と遠隔asset不整合拒否を含む。
- production HTML/CSS/JS＋明示的なin-memory IPCで、9サイズ/DPI条件、数値表示の安定性、3系統選択、工程内100%と全体完了の区別、旧進捗拒否、書込み中の操作ロックを確認した。
- 進捗panelがスクロールで画面外になる問題をvisual QAで発見し、sticky表示へ修正。画面内に残る座標条件を回帰テストへ追加した。

途中に検出した系統名・fixture selectorの失敗を修正後に再実行した。先の成功件数だけで最終headのCI成功を代替しない。

## 配布検証

正式公開はtag対象sourceのWindows package buildと別runnerでの配布検証の成功後にのみ行う。実際のsource hash、Firmware画像/manifest、依存license、Windowsメタデータ、配布物の検証結果は生成されるBUILD-INFO/VALIDATION/UPGRADE-VALIDATIONとGitHub Actionsを正とする。

## 未実施と既知の制限

この作業では実serial接続・reset・Firmware書込み・EEPROM変更・実操作者の個人校正を行っていない。software fixture、実物からの入力、物理精度、安全性の認証を区別する。旧画像と先輩の実機画像が同一かは不明。既知の約−65536 countのimpulseについて、原因や修復は今回のUI/配布変更で証明していない。

Managedの品質付きstreamは現在20 Hz上限。1.0.0というソフトウェア版番号は、取得品質や力のN単位精度の保証ではない。コード署名は未導入。実ロボット制御とSim側の入力統合は今回の配布に含まない。

## CIで見つかった試験用投入の競合

初回PRのnative GUI試験で、fixture_inputの直接IPCと定期読取りが競合し、非待機の排他guardで試験投入が拒否された。実機操作のguardは変更していない。試験側だけに、未実行のbusy応答を受けた模擬センサ値投入の有限再試行を追加し、他の例外・実機操作・校正/保存の再試行は認めない。失敗回数の上限と即時伝播を単体検証した。書込み中の共通toolbarとmenuも無効化し、更新前のFirmware情報を現在情報と誤読しない見出しに変更した。

追加のローカルnative GUI試験は実アプリのIPCと共有Rust service、明示scripted transportで179区間を取得し、保存/再読込み/補正出力/品質異常の失効と復帰までPASS。実hardware acceptanceはNot Run。書込み失敗・確認待ちで不定進捗のアニメーションを残さないことも、画面試験で確認した。
