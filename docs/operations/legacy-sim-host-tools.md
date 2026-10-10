---
status: canonical
owner: operations
last_verified: 2026-10-10
canonical_for:
  - legacy Sim monitor and measure operation
related:
  - docs/contracts/legacy-sim-host-tools.md
  - docs/operations/hardware-safety.md
---

# 旧Sim monitor / measureの操作

能力・引数/defaultの正本は[host契約](../contracts/legacy-sim-host-tools.md)である。
採用済みDevice checkoutでbuildする。自動download、firmware書込、Deviceへの操作はbuildに含まない。

```text
cargo build --locked -p selfrionettectl
```

`target/debug/selfrionettectl`（Windowsは`.exe`）の次のhelpはportを開かない。

```text
selfrionettectl legacy-monitor --help
selfrionettectl legacy-measure --help
```

以下は実機操作例であり、本移行のsoftware検証では実行しない。
[hardware safety](hardware-safety.md)に従って対象device、firmware target、port、無負荷/参照荷重、
停止手順、state変更を確認し、operatorが明示承認した場合だけ使う。

```text
selfrionettectl legacy-monitor --port COM9 --display-level warn
selfrionettectl legacy-measure --port COM9 --sensor 4 --repeats 3
```

monitorのCalibrate/key cは測定基準を変更する旧v1要求である。自動testはfakeへ送るだけにし、
実校正・EEPROM・v2 tare/provisionを実行しない。停止はq/Ctrl+C（measureはCtrl+C）、serial/consoleは回収する。
monitorとmeasureを同じportへ同時接続しない。

## Sim互換入口の採用

Device変更を採用し、そのrevisionでbuildしたnative CLIの絶対pathを、呼出processの`SELFRIONETTECTL`へ明示する。
通常Sim起動やoffline fixtureへDevice依存を持ち込まない。Simのcompatibility PRはこの順序を満たすまでmergeしない。
旧PowerShell形式はCLIの`--powershell-args`で解釈し、Sim wrapperにserial・集計・defaultを複製しない。
Port省略は失敗する。COM番号や接続順でdevice identityを推測しない。
