---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-14
related:
  - docs/contracts/firmware-protocol-v2.md
  - docs/contracts/serial-transport.md
  - docs/operations/hardware-safety.md
---
# GUIの実機検出と受信モニター

## 意味と範囲
通常起動は実機モード。OSのシリアルポート一覧を取得するが、列挙でportをopenしない。合成試験は開発者モードで明示選択した場合だけの別モードとし、合成ID/値/校正候補を実機へ転用しない。
OSのport候補、接続済み、7ch形式受信、個体情報応答、物理校正済みは別状態。VID/PIDやCOM順からSelfrionette個体ID・左右を推定しない。

## 接続
表示したportを人が選び、接続前確認を通した一台だけを115200 baudでopenする。現存するOS一覧との再照合を行う。DTR/RTSの開始によって機器が初期化され得ることを説明する。
自動接続・自動再接続・1200 baud touch・tare/c・provision・EEPROM・flash・robot送信は行わない。切断とアプリ終了でhandleを解放する。合成モードへの切替前に切断する。
旧Sim互換のvector形式とProtocol v2のvector形式を共通parserで受け付ける。接続dialogの「個体情報を照会」が有効な場合だけ、選択した一台にinfoを送る。対応基板候補ではチェック済み、その他/不明では未チェックを初期値にする。無送信の旧Firmware受信も維持する。診断からの明示照会もinfoだけを送る。応答なしを架空IDで補わない。

## 受信と異常
一回の受信処理は最大40ms/128行。行の上限は共通transportの1024 bytes。最新7値と直近8診断だけを保持し、全履歴を蓄積しない。途中の行は次のpollへ保持する。
受信時刻はhost monotonic clock。500msを超える旧値を現在値として表示しない。同じdevice timestampの反復では鮮度を更新しない。device timestampの後退、metadata変更、known frameの不正、I/O失敗では値を消して切断し、明示再接続を求める。起動/校正通知が受信中に現れた場合も旧基準の値を継続しない。
数値はFirmware処理後の受信値であり、raw ADC、N単位、GUIの無次元入力とは別物。受信できたことはセンサ品質や校正の証明ではない。warnが付いた値を正常な測定として認定しない。

## GUIと操作の競合
実機と合成の状態を分離し、実機モードで合成調整を実行しない。本書のv2モニターは受信確認と診断を担当する。品質付き設定・個人校正は[required-setup-calibration](../architecture/required-setup-calibration.md)の別経路で、v2値をrawや品質既知へ昇格しない。両経路の同時port所有をRustで拒否する。
Rustがconnection revisionを所有し、connect/read/info/disconnectは期待revisionを照合する。frontendは要求を一つずつ送り、受信中の利用者操作は一つの待機欄へ保持する。読取はボタンを無効化せず、切断は進行中読取の後に実行する。接続session変更時は待機要求を拒否する。通信不明時は現在値を消す。blocking I/OはTauriのUI threadの外で実行する。

## 検証
偽backendで列挙がopen/writeしないこと、対象照合、旧要求拒否、旧Firmwareの無送信受信、infoだけの送信、7ch有限値、切断/不正行/再起動/metadata変化、鮮度失効、診断上限を試験する。
native QAは実機モード起動とOS列挙を確認した後、明示的に合成モードへ切り替えて従来の全試験を実行する。実機にポートを開く試験は明示許可した対象で別途行い、software testを実機受入と呼ばない。

## 表示層の限定的な履歴
Rust受信側の最新7値/直近8診断の上限は変更しない。frontendの表示履歴だけは直近30秒、最大200表示点を保持する。これはサンプリング済みの表示用データであり、受信全件や物理品質の証拠として書き出さない。失効時は現在値だけを消し、過去30秒分の表示履歴を「更新停止」として時刻固定で保持する。再受信時は欠測区間の線を結ばず、新しいconnection sessionでは同じCOMでも履歴を破棄する。数値の更新ではDOMを作り直さず、列境界/小数点位置/未受信時の表配置を保つ。

通常/開発者の入口、USB候補の根拠、descriptor照合は[session契約](device-manager-session.md)を参照する。
