# Protocol 3送信上限の40 Hz化（software-only）

Issue #22。Managed Firmware 1.0.0の品質付きProtocol 3は50 ms周期（20 Hz）、v2互換は12.5 ms周期（80 Hz）だった。次期Managed Firmware 1.0.1でv3送信試行を25 ms周期（最大40 Hz）に変更する。wire形式、7chのraw/oriented/flags、tare、session、sequence、EEPROM、Original/Compatible保管ソースは変更しない。

## 速度設計の根拠

- 現行sample3は28 fields、最大219 bytes。40 frames/sでは最大8760 bytes/s。もし物理8N1 UARTを115200 bpsで通す場合、理論容量11520 bytes/sの約76%に相当する。
- **Pro Micro（ATmega32U4）のUSB Serialはnative USB CDCであり、115200という設定値が物理USB上限という意味ではない。** 上記は保守的な送信文字量の比較である。50 Hzなら10950 bytes/s、80 Hzなら17520 bytes/s。USB CDCで80 Hz不可能と結論する根拠にはならない。
- HX711の変換周期はRATE端子の接続で通常10/80 SPS。研究室の実基板の配線は未確認。10 SPSなら送信40 Hzでも40個/sの新ADCデータは得られない。
- updateSensorsによるready確認は送信と独立。新rawがないchはheld、50 ms以上古ければheld|timeout、saturated/rejectedは校正・指令で不採用。品質判定を緩めない。
- 256-byte有界出力queueの空き不足時はframeをskipし、sequenceはenqueue成功時のみ進む。長い停止後に未送信frameを一斉に取り戻さない。

## 試験

C++ native fixtureで40 Hzの期限、32-bit micros wrap、1秒間の40試行、10回の模擬backpressure skipで30 frameがsequence付与されること、fresh/held/timeout/rejected、最大長の送信文字量を検証する。既存Protocol v2、v3 parser、tare、保存設定、SRAM/Flash gateを維持する。

## 実機受入（未実施）

対象入力装置の個体ID、Firmware SHA・版とUSB portを明示した許可の下で、全7chのADC形式/RATE接続を確認する。20 Hz版と40 Hz版で、実効frames/s、各ch新規fresh/sと全7ch同時fresh/s、held/timeout/rejected/saturated割合、device/host時刻、周期p50/p95/p99、sequence/epoch、欠測・停止・再接続を同条件比較する。GUIは現在約50 msごとの有界readを行うため、画面fpsと通信の受信fpsを混同しない。SimのInput Sourceアダプタ（#631）での鮮度・遅延・RTF測定は別タスクである。

個人プロファイルはFirmware版とsource SHAに束縛される。旧版で保存したプロファイルを無検証で再利用しない。ビルド成功は実機で40件/sのfreshデータが取得できた証拠ではなく、50/80 Hzは実測後に判断する。
