---
status: proposed
owner: architecture
date: 2026-10-12
related:
  - docs/decisions/0002-separate-device-management-from-simulation-plugin.md
  - docs/contracts/calibration-preview.md
---
# ADR-0008案: Rust coreを共有するoffline Device ManagerをTauriで準備する

ADR-0002はGUIをDevice repositoryの責務とし、Tauriを第一候補にしている。
framework固定は未確定だったため、今回のローカル準備を別decisionとしてレビュー可能にする。

Tauri v2の薄いshellとframework非依存HTML/JavaScriptを使う。UIは同じRust coreを直接呼ぶ。
追加するapp commandは合成fixtureの開始・基準・候補・手動確認・preview・draft export・個体別保存/再読込・失効・状態読取に限定する。
serial、filesystem/shell plugin、daemon、自動接続、firmware mutationを追加しない。
publicな校正成功契約や既存physical/Sim境界を変更せず、実機verifiedを生成できないoffline段階にする。

Windows native buildにはMicrosoft C++ Build ToolsとWebView2が必要。
LLM-01では最小Build Toolsと既存SDKを利用し、Windows native Debug build/起動/Rust IPCと合成記録の保存/再読込を確認した。
合成経路の確認を実機physical calibrationやRelease配布の受入と混同せず、このADRはproposedを維持する。
Nodeのみで起動するpresentation previewを別経路として用意する。校正logicの代替にはしない。
実機対応を追加する時点で、品質情報、校正session capture、保存/失効、明示操作の境界を再レビューする。

## 後続の合成workflow再構成
private IPCは型付きdevice_commandへ変更し、個体別session、閲覧専用read_history、明示再開resumeを提供する。
校正の計算・失効規則はcore、use caseとviewはworkflow、Tauriは薄いshellへ分離する。
製品の操作体系は[device-manager-workflow.md](../architecture/device-manager-workflow.md)を正とする。このADRは引き続きproposedである。
