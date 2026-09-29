---
status: canonical
owner: operations
last_verified: 2026-09-29
canonical_for:
  - git branch and pull request workflow
related:
  - AGENTS.md
---

# Git / PR Workflow

## Standard

bootstrap完了後は`main`へ直接作業しない。

1. latest `main`を確認する
2. clean stateを確認する
3. topic branchを作る
4. task scope内だけ変更する
5. relevant validationを行う
6. diff / scopeを確認する
7. commit
8. push
9. PR
10. remote head / PR metadata / CIを確認する

agentが作るbranchは原則`codex/` prefixを使う。

## Separate operations

commit、push、PR create / update、merge、Issue close、branch deleteを同一権限として扱わない。

merge / Issue close / branch deleteは明示許可が必要。

## Bootstrap exception

repositoryにbranch作成元commitが存在しない完全なempty stateでは、最初のbootstrap commitだけdefault branchへ作成してよい。

その後は通常workflowへ移行する。

## Diff discipline

- unrelated cleanupを混ぜない
- generated artifactをcommitしない
- machine-local configをcommitしない
- hardware captureは必要最小限のevidenceだけ保存する
- accepted ADRのdecision変更を通常implementation PRへ混ぜない
