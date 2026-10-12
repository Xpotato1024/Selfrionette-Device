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
追加するapp commandは合成fixtureの開始・基準・候補・手動確認・preview・draft export・失効だけ。
serial、filesystem/shell plugin、daemon、自動接続、firmware mutationを追加しない。
publicな校正成功契約や既存physical/Sim境界を変更せず、実機verifiedを生成できないoffline段階にする。

Windows native buildにはMicrosoft C++ Build ToolsとWebView2が必要。
このmachineではBuild Tools不足のためnative build/起動は未検証で、完了扱いしない。
Nodeのみで起動するpresentation previewを別経路として用意する。校正logicの代替にはしない。
実機対応を追加する時点で、品質情報、校正session capture、保存/失効、明示操作の境界を再レビューする。
