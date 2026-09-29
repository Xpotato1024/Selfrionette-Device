# Architecture Decision Records

ADRは重要な設計判断と、その時点の前提・代替案・結果を記録する。

## Lifecycle

- `proposed`: 未確定
- `accepted`: 採用済み
- `superseded`: 新ADRに置き換えられた
- `rejected`: 検討したが採用しなかった

Accepted ADRのdecisionを後から通常編集で変更しない。判断を変更するときは新ADRを追加し、旧ADRのstatusとsuperseding linkだけを更新する。

## Current ADRs

- [ADR-0001](0001-retain-pro-micro-cpp-firmware.md): Pro Microを継続し、初期firmwareはC++で実装する
- [ADR-0002](0002-separate-device-management-from-simulation-plugin.md): Device管理をXpotato-Simから分離する
