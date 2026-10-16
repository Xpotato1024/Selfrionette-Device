---
status: historical
owner: selfrionettectl
date: 2026-10-16
related:
  - docs/operations/field-test-30-60min.md
---

# 2026-10-16 初回の受信観測

ユーザーが実施したcaptureの履歴。以下はユーザー提供ZIPから確認済みとして引き継がれた事実であり、この修正を行ったagentはhardwareアクセス・serial open・再取得を行っていない。raw logや利用者固有pathは公開しない。

| 項目 | 記録 |
|---|---|
| ZIP SHA-256 | `8f08c7e628a1af09a3faaeb605e0459e19df3a137584286db8abaf2989f47c0c` |
| PC source revision | `379524545607e377bed68dcf64eb4b594e183ee0` |
| outcome | `completed_observation` |
| vector総数（旧`vectors`） | 62875 |
| 集計対象（scored） | 47109 |
| 集計対象外（unscored） | 15766 |
| warnings | 3604 |
| duplicate timestamps / device gaps >200 ms / unknown lines / statuses | すべて0 |
| Firmware revision / stable device ID | 取得できず不明 |

spike警告のch別内訳はCH0=137、CH1=1163、CH2=956、CH3=0、CH4=8、CH5=1195、CH6=145（合計3604）。指先部品は未装着。ユーザー申告ではCH0–2=親指、CH3–5=人差し指、CH6=小指だが、対応は暫定であり、物理的方向・極性・校正値を確定しない。操作軸ラベルとwire chは別物である。

約−65536 countのimpulseは追加調査を要する。digital bugの証明ではなく、このPRによる修正済み事象でもない。受信値はFirmware処理済みcountであり生ADCやNではない。Firmware/個体情報が不足するため、完全なFirmware診断はできない。

旧capture実装では、前区間のblocking readが次の`/prepare`へ跨ぐと、実際の`operator_wait`/準備完了より前のvectorを次trialへ集計し得た。そのvectorと準備完了後のvectorを同じ区間で結ぶことで、待機時間が`observed_line_rate_hz`を汚染し得る。上記のscored数は旧実装の分類を記録したものであり、修正後の境界規則による再分類・品質保証ではない。旧capture・summaryの内容は変更しない。

今回の修正は以後のcue/read境界・入力処理・集計表示を対象とする。通常vectorの品質は引き続きunknown。完了は受信観測の完了であり、physical calibration、7軸独立性、control readiness、品質認証、実機制御許可を意味しない。
