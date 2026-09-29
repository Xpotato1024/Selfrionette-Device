---
status: canonical
owner: architecture
last_verified: 2026-09-29
canonical_for:
  - documentation lifecycle
  - source-of-truth rules
  - ADR handling
related:
  - docs/README.md
  - AGENTS.md
---

# Documentation Contract

## Purpose

Selfrionette-Deviceのcurrent specification、decision rationale、operator procedure、validation evidenceを混同しないためのdocumentation contractを定義する。

## Document classes

### Canonical

current architecture、contract、operator procedureの正本。

front matter:

- `status: canonical`
- `owner`
- `last_verified`
- `canonical_for`
- `related`

同一topicに複数canonical documentを作らない。

### Accepted ADR

設計判断の理由とdecision時点の前提を保存する。

- `docs/decisions/`へ置く
- Accepted後にdecisionを都合よく書き換えない
- 重大な判断変更は新ADRを作成し、旧ADRを`superseded`へ変更する
- ADRはcurrent behaviorの全文仕様ではない。current contractは`docs/contracts/`または`docs/architecture/`へ置く

### Supporting

current canonical documentを補助する説明、例、guide。単独で仕様決定に使わない。

### Historical / Evidence

過去の観測、実機validation、migration provenance、旧実装の記録。

current behaviorをhistorical documentから逆算しない。

## Directory responsibilities

- `docs/architecture/`: current ownership、component boundary、structure
- `docs/contracts/`: versioned/public/cross-layer semantics
- `docs/decisions/`: ADR
- `docs/operations/`: current operator procedure
- `docs/experiment-notes/`: hardware validation / calibration等の観測 evidence
- `docs/reports/`: inventory / audit / implementation report

## One topic, one canonical source

例えばserial protocolのproducer specificationを複数文書へ複製しない。

consumer repositoryであるXpotato-Simは互換version、fixture、consumer behaviorを所有できるが、Selfrionette-Deviceのproducer protocol本文を第二SoTとして複製しない。

## Update rules

次が変わる場合、同じchangeでcanonical docsを同期する。

- firmware protocol
- device identity semantics
- calibration ownership / persistence
- repository responsibility
- operator hardware procedure
- firmware update procedure
- public CLI / GUI behavior
- memory budget / resource gate

implementation chronologyだけが変わりcurrent semanticsが変わらない場合、無関係なcanonical docを更新しない。

## Language and encoding

Human-facing textは日本語を基本とする。code identifier、protocol literal、formal product name、CLI、pathは英語のままでよい。

Markdown / textはUTF-8 without BOMを基本とする。

## Linking

`docs/README.md`をSource of Truth Mapの入口とする。

canonical documentを追加・移動・supersedeした場合は、必要に応じて`docs/README.md`を更新する。
