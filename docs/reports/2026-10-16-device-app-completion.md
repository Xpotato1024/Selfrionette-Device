---
status: evidence
owner: device-management
date: 2026-10-16
related:
  - docs/architecture/required-setup-calibration.md
  - docs/contracts/firmware-protocol-v3.md
  - docs/contracts/firmware-update.md
  - docs/releases/device-manager-v0.2.0-preview.1.md
---

# Device Manager 0.2.0-preview.1 実装・検証記録

対象は`codex/device-app-completion-20261016`、開始commitは`9994bbbc927bcd87ff33a312df6167c72da07a6b`。
以下は未commitの実装を含むsoftware検証の記録である。commit/push/merge/tag/Release公開は行っていない。
`0.1.0-preview.5`はDraftであり、公開済みとは扱わない。

## 実装した利用操作

| 操作 | 実装と境界 |
| --- | --- |
| 個体確認・登録 | 明示port選択、対応v3情報照合、OS乱数による非zero 128-bit IDの明示登録と読戻し。既存IDを再登録しない |
| センサー取付設定 | 全7chのraw/装置oriented/取得flags表示、実際の部位・方向・wire→logical対応、既知方向観測による極性候補、独立確認、装置保存とgeneration読戻し。COM順や暫定finger配置をdefaultにしない |
| tare | 無荷重確認後の全ch取引と結果/epoch照合。失敗はreadinessを失効させる。個人neutralとは分離 |
| 個人校正 | 操作者/姿勢/握り、今回のneutral、方向ごとのuser-ready開始、neutral/ramp/hold/releaseの反復、skip/再試行/中止。共通fitterによる候補A/B比較と別記録の正逆/弱入力/同時入力/除荷検証 |
| 保存と再利用 | 実際のfit/span/noise/filter・method・版・装置設定/identity/source・取得/held-out根拠をimmutable保存、読戻し検査と互換性照合。ready/今回のneutralを永続復元しない |
| 現在入力 | 装置極性を二重適用せずlogical順へ投影し、今回のneutralから正規化する。欠測、取得品質、span超過、未対応方向を保持する。GUIと明示port/profile指定CLIは同じRust service/runtimeを使う |
| Firmware更新 | 同梱/ローカルpackageとHEX検査、固定runtime hash、保護記録、明示書込み確認、bootloader対象照合、要復旧journalと再接続読戻し。初回導入とID登録を分離 |
| 配布 | 固定app IDとXpotato.net表示を維持したsetup/portable、単独Firmware package、CLI/updater runtimeと通知を生成するcode。clean sourceからbuild-onceし、独立runnerのbyte/起動/GUI証拠を通してtag prereleaseを公開する |

旧v2は既存consumer向けに維持する。v2 processed値をrawとして使わず、取得品質不明のため個人校正へ昇格させない。
通常の装置設定・個人校正・profile再利用にJSON編集やCLI操作を要求しない。
手順閾値は調整可能な実験用設定であり、物理精度の合格値ではない。

## 実行証拠と未達gate

| 検証 | 結果 |
| --- | --- |
| Rust workspace fmt / 全target clippy（warnings拒否） | PASS |
| Rust workspace全target tests | 171/171 PASS。doc testは2 target、各0件 |
| Tauri native fmt / 全target clippy / Debug build | PASS |
| Tauri native tests | 53/53 PASS。fake transport/programmerのみ |
| frontend / CI scope / 配布条件の実行可能なNode tests | 64/64 PASS。下記3件は環境制約で除外し、製品のCI gateには残した |
| 配布testの子process 3件 | ZIP round-trip / Windows metadata / stage子processが`EPERM`。成功扱いしない |
| stage/remote assetの直接fixture検証 | 5件PASS：9 assetsのstage、remote一致、remote size不一致拒否、別exeのGUI証拠拒否、payload検証なし拒否 |
| Firmware memory report parser | 3/3 PASS |
| Firmware native contract | protocol/acquisition/tare/outputの8 groups PASS、settings 6 groupsと132 interruption/reboot cases PASS |
| AVR compile-only contract / Firmware build | 2 objectsとPlatformIO build PASS。resource値は下記 |

独立reviewで見つかったFirmware 2件、host 4件、updater/notice 3件は修正し、各修正の独立確認を行った。
testで判明した初回replay時のneutral判定の抜けも修正し、training値の再利用拒否を保った。
reviewをtest実行の代用にはしていない。

Windows native Debug executableをcompileし、最終review修正前のbuildを`--developer-mode --scripted-hardware`で起動した。
最終修正後のDebug buildも成功したが、描画startup IPCの完走を再確認できたという意味ではない。
これはOS serial portを開かず、実際のnative IPCと共通serviceへ接続する専用transportである。
ただし、この環境ではWebView2のCDP listenerが開かず、描画UI自動操作は30秒timeoutで停止した。
**描画UIの操作完走とスクリーンショットは未取得であり、PASSではない。**
失敗reportは`apps/device-manager/src-tauri/target/native-qa/live/run-OfuD88/REPORT.json`にある。
CIの`live-native-qa.mjs`は実際のexe hashと版を記録し、個体設定・校正・保存runtimeの3画面を要求する。
公開gateは、この同じexeで成功したreportと画面がない場合に停止する。

この実装作業ではdirty sourceからの配布を許可していない。setup/portable/Firmware ZIPのRelease build、
同梱AVRDUDE実行、installerの導入/削除、独立runner CI、公開は未実施である。
制限環境でNodeの子process起動が`EPERM`となる配布fixture試験は、実行成功とは扱わない。

## Firmware image

開発用image: `target/firmware/firmware.hex`（57,859 bytes）。
SHA-256: `bff4a2cd0d385490aca0bd5a71d35407a6f8f94985e693725292e6c2438c897d`。
Flashは20,564/28,672 bytes、static SRAMは1,013/2,560 bytes。
既存gateの24,576/1,024 bytesを満たす。static SRAM gateまでの余裕は11 bytesである。
imageには開始HEADをsourceとして注入しており、未commit firmware全体のrevision証明にはならない。
`target/firmware/BUILD-PROVENANCE.txt`はこの制約を明記する。配布imageはcleanな最終commitから再生成する。

## 実機・外部consumerの境界

実serial open、reset、flash、EEPROM変更、robot動作は一度も行っていない。
既知荷重によるN係数、実センサーの精度/安全性、実boardのUSB/bootloader/readback、
実操作者による手順受入は未実施。観測された`-65536` impulseの原因確定や修復を主張しない。
Xpotato-Sim adapter、robot mapping/kinematics/controlは含まない。

## Workspace hygiene

task作成の`.pio`、空の`%SystemDrive%`、`node_modules`、Tauri生成schema directoryの削除が自動承認reviewに`blocked by policy`で拒否された。
別の削除手段で迂回していない。clone外のtask tempも、利用者のclone外cleanup禁止に従い保持した。
そのためworkspace hygieneはFAILであり、全受入完了を主張しない。
`target`/`dist`内のbuild/test証拠は利用者が意図した残存出力である。
最終的なsource一覧・残存path manifestと検証集計は`target/implementation-evidence/`へ出力する。
残存非成果物は6 root（clone内4、clone外2）。完全な子path一覧は`remaining-non-deliverables.json`を参照する。
意図した残存物は変更source一式と、利用者が保持を許可した`target/`および`apps/device-manager/src-tauri/target/`のbuild/test証拠である。

## 非blockerの保守的な拒否

PC設定recordで同じrevisionの異なる内容が複数ある場合、より新しいrevisionが別にあっても、
directory列挙順によって読込みを保守的に拒否し得る。通常のimmutable保存はこの重複を作らず、
古いmappingへ復帰することもない。非materialなavailability候補として別記し、今回の受入範囲を広げていない。

## Primary統合確認

実装担当のsandboxとは別に、同じ作業コピーでWindows workspace171件、native53件、frontendおよび配布28件を省略なしで実行し、fmt/Clippyを含む一括検証はexit 0。前述のEPERMは担当環境の履歴であり、この再実行ではZIP・metadata・stageの3件もPASSした。

Primaryのnative GUI試験では、実際のTauri IPCで個体登録・全7ch設定・逆向き補正・tare・113手順の個人校正・held-out検証・profile保存まで到達した。現在入力への適用を含む固定版の一括完走と配布runner検証は別途確認し、途中成功を全体PASSにしない。
