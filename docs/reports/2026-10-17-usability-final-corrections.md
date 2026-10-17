# preview.2 bounded final corrections — software evidence

状態: **PARTIAL — required CI / GUI / distribution acceptance pending**。未commitの同じworktreeで修正した。公開・hardware認定ではない。

## 対象と出典

- Base HEAD: `9dbc372b446caa265c6789f2fd8e8ec9d182e33e` + uncommitted worktree。buildへ渡したsource SHAはbase HEADであり、dirty worktree全体をそのcommitと同一とは扱わない。最終入力hashは`target/final-corrections/source-snapshot.json`を参照する。
- `change_class=semantic`、対象はWindows Device Managerのmonitor/setup/update、同梱2系統、UI/docs/tests。任意のcustom transportや新規hardwareは対象外。
- 新版: `firmware/loadcell_7ch` + `firmware/shared`。
- **旧版（初期保管ソース）**: `firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/loadcell_7ch_legacy`。`isLeader=true`、元の7ch・driver・pin・codeを維持し、`firmware/legacy_7ch_package/platformio.ini`の別projectでcompileする。先輩のprivate source由来の保管版で、実際の配備revisionは不明。
- 後の整理候補`loadcell_7ch_pro_micro`は参照専用。両凍結treeを含む15ファイルのraw Git blobがHEADと一致した（`frozen-archives.json`）。

## 修正結果

- 初期保管版の文字`i`はtareなので、GUIの未知／旧版モニターは接続・再開とも受信専用にした。明示Infoは`?2`、setup/updateの明示probeは`?3,<request>`と`?2`だけを送る。宛先が新版でも更新元にlegacyがあり得る。
- 新版は同じv2/v3 frameを返す安全なaliasを追加した。無応答時に`info` / `info3`へ自動fallbackしない。alias非対応preview.1は自動probe対象外で、初回Firmware更新へ案内する。既存の明示CLI contractは維持した。
- 既知IDは再照合し、不一致・無応答を未知扱いへ落とさない。pause/resumeはbackendのhandle所有・candidateを使い、最後に所有した経路へ戻す。以前のneutral・ready・outputは復元しない。
- catalog、schema 2 manifestのsource_path／tree hash、SOURCE-INFO、recovery record、UI、Release templateを初期保管版へ揃えた。legacy postflash/recoveryは全application image比較と有限passive 7ch受信で、ID・版・設定保持はunavailable/unknownのまま。
- updaterで有効な選択を保持し、切断候補は再接続待ちのplaceholderにする。消えた候補でpreflight/writeを有効化しない。
- 800/1100/1600px、125/150%相当のlayout、monitor→populated updater、plan/confirmation gate、legacy選択、modern pause/resume、最後の接続先、再起動preference、empty/disconnectedのCI試験を拡張した。target appのbuild設定・devtools・CSPはこの修正で変更していない。

## 実行した検証

コマンドはrepository rootで実行。Native command promptは既存VS2022のVsDevCmdを使い、`CARGO_TARGET_DIR=<repo>/target/native`、`TEMP/TMP/CODEX_TASK_TEMP_ROOT=<repo>/target/final-corrections`をprocess内だけで設定した。global設定は変更していない。

| コマンド | 結果 |
|---|---|
| `cargo +stable test --locked --workspace` | **172 passed**, 0 failed |
| `cargo +stable test --locked --manifest-path apps/device-manager/src-tauri/Cargo.toml --lib` | **74 passed**, 0 failed |
| `cargo +stable clippy --locked --workspace --all-targets -- -D warnings` | PASS |
| `cargo +stable clippy --locked --manifest-path apps/device-manager/src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS |
| `cargo +stable build --locked --manifest-path apps/device-manager/src-tauri/Cargo.toml` | native Debug build PASS、起動なし |
| `node --test --test-isolation=none apps/device-manager/frontend/*.test.mjs apps/device-manager/tools/distribution.test.mjs apps/device-manager/tools/firmware-packages.test.mjs tools/ci-scope.test.mjs` | **81 passed / 85 total**, 4 failed, 0 skipped |
| `python -m unittest tests/test_check_firmware_size.py` | **3 passed** |
| `node tools/test-firmware-command-safety.mjs --emit-cpp <repo>/target/final-corrections/command-safety.cpp` → `cl /EHsc /std:c++14 /utf-8 ...` → generated exe | PASS: 両凍結ソースから取り出した実command blockを実行。`?2/?3`無変更、元の`i`校正のpositive control、新版alias dispatchと既存info応答 |
| `cl /EHsc /std:c++14 /utf-8 firmware/tests/firmware_protocol_v3.cpp ...` → generated exe | PASS: parser/quality/tare/resume/orientationと追加alias境界 |
| changed JS 9ファイルの`node --check`、`git diff --check` | PASS |

実行wrapper／詳細log: `target/final-corrections/rust-checks.cmd`、`native-verify.cmd`、`rust-tests.log`、`native-verify.log`、`node-tests-final.log`、`python-tests.log`。

### Firmware compile（serialなし）

`PLATFORMIO_CORE_DIR=<repo>/target/final-corrections/pio-core`。既存の固定toolchain packageをこの明示artifact directoryへcopyして使用した。global coreはread-onlyで、最初のbuildはglobal `platforms.lock` write拒否で失敗した。local coreへ変更後、同じpackage pinでcompileした。modernでは`SELFRIONETTE_SOURCE_SHA=9dbc372b446caa265c6789f2fd8e8ec9d182e33e`を明示した。

| コマンド | Flash / static SRAM | 結果 |
|---|---|---|
| `python tools/check_firmware_size.py --project-dir firmware/loadcell_7ch --environment pro_micro_7ch --max-static-ram 1024 --max-flash 24576` | **20568 / 1019 bytes** | PASS、SRAM gateの余裕5 bytes |
| `pio run -d firmware/legacy_7ch_package -e legacy_7ch` | **11168 / 773 bytes** | PASS、元の初期保管ソース |

各build/workspace/cacheは`target/final-corrections`配下へ指定。HEX/ELFとcompile logを同配下に保持した。firmware compileはruntime stack余裕や実機動作を証明しない。

## 未達・未実施

- Nodeの4失敗: ZIP round trip、Windows metadata module isolation、release staging、inherited PATH child。`spawnSync powershell.exe/node.exe EPERM`のため子プロセスを実行できない。assertion/skipやapp build/CSPを変更して通していない。
- ローカルGUI/Edge起動は以前のautomatic permission reviewで拒否されたため再試行せず、別toolによる迂回もしていない。
- 描画9条件・interactive fixture UI・real native scripted QA・installer/portable/release validation・exact-head GitHub CIは **Not Run**。拡張コードやmock単体試験だけで必須CIをPASSとは扱わない。
- independent fresh read-only review: reviewed dirty code snapshotで**material P1/P2なし**。reviewerはtest/hardwareを実行していない。runtimeの実効model/effort metadataは非公開で、指定設定を観測済みとしない。optional backlog: catalogのmodern metadata検査をRust backendと同じ強度へ揃えるhardening（現在の生成metadataに不一致なし）。
- hardware、serial open、reset、flash、EEPROM、secrets、global config、git mutation、公開・releaseはすべてNot Run。

## Workspace hygiene

cleanupは利用者指示に従って未実施。policy-denied remnantsは削除していない。新規の検証成果物は意図した`target/final-corrections/`、native buildは既存`target/native/`に保持する。OS temporary rootは作っていない。完全な変更path／artifact directory一覧は`target/final-corrections/remaining-paths.json`を参照する。

親directoryにbaseline後の`../final-review-task.md`を観測したが、この修正agentが作成したpathではなく所有未確認のため触れていない。最終hygiene判定と残存pathは同じdirectoryの`final-status.json`に記録する。必須CI未達なので作業全体の完了を主張しない。
