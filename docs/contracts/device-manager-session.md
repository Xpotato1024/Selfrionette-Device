---
status: canonical
owner: apps/device-manager
last_verified: 2026-10-14
canonical_for:
  - Device Manager developer gate and native menu
  - Candidate discovery and selected-port validation
---
# 通常操作・開発者機能・接続先の設計

## 通常起動と開発者モード
通常起動は実機の管理画面だけを提供する。合成試験・入力元選択は通常toolbarへ置かない。
Windows標準タイトルバーとTauriのnative menuを使用する。デバイス/表示/ヘルプを常設し、開発者モード時だけ開発メニューを追加する。
ヘルプの明示確認、または --developer-mode で現在processに限り有効にする。--synthetic は --developer-mode と同時指定した場合だけ許可する。未知/重複引数は拒否する。
有効化だけでは実機から切り替えない。合成切替は別操作。実機接続中はRustが切替を拒否する。GUIの非表示だけに依存せず、合成IPCは開発者モードかつ合成画面でのみ許可する。
終了/再起動で開発者モードを保持しない。無効化は合成画面を閉じるがPC記録は削除しない。署名・Firmware・EEPROMの権限を付与するモードではない。

## COM候補と接続
OS列挙からUSB種別、VID/PID、製品/メーカー、USB serialを保持する。候補/その他/不明/bootloaderを区別し、候補は対応基板に過ぎずSelfrionette確定ではない。許可表は出典付き、COM番号をhard-codeしない。
接続先dialogで候補一覧を表示し、その他も明示表示できる。選択中の一覧は自動更新・並べ替えしない。手動更新でdescriptorが変われば選択を破棄する。
接続直前に選択時descriptorと再列挙結果を比較する。bootloaderは受信接続不可。シリアル番号を持たない同型機の物理交換を完全識別できるとはしない。
選択した一台だけ開く。初期化され得ることをその場で説明する。info照会はdialog内の明示されたoptionだけで実行し、その他のポートでは既定off。旧Firmwareは無送信受信を残す。
USB候補/7ch形式受信/v2応答/個体ID/物理校正は別状態。COM順やVID/PIDから左右役割を割り当てない。個体の恒久登録/二台同時監視/自動再接続は今回行わない。

## 受信と表示
バックグラウンドreadは操作部を無効化しない。読取中の操作は有限の待機欄で保持し、同じconnection sessionのままのときだけ最新revisionで実行する。切断要求は以後のpollを止め、進行中read終了後に実行する。要求を黙って落とさない。
ch行選択と履歴対象は一つの選択状態で管理する。現在値失効は同じセルの—とし、履歴は更新停止を示して保持する。同じ接続の再開では欠測区間を結ばず、新接続では消す。
履歴は30秒/200表示点以内、自動/固定スケールを提供する。受信全件や校正証拠とはしない。dialogのボタン列/対象/主操作/フォーカスを統一する。

## 受入
通常起動の合成IPC拒否、明示GUI/CLI有効化、接続中切替拒否、モード再起動reset、候補2+その他1、同VID別用途、descriptor変更、bootloader、unknown fallback、info無応答、読取中切断、二重操作、stale表示/履歴欠測を模擬backendで試験する。
実機受入は後日。大学の装置・serial・EEPROMへはこの作業でアクセスしない。

## USB候補表の出典と限界
SparkFun Pro Micro: VID 1b4f、通常PID 9204/9206、bootloader PID 9203/9205。Arduino Leonardo/Micro: VID 2341/2a03、通常8036/8037、bootloader0036/0037（Micro 2341:8237/0237も含む）。VID/PIDの一致は同型基板の候補判定であり、Selfrionette固有の識別子ではない。
- https://github.com/sparkfun/Arduino_Boards/blob/main/sparkfun/avr/boards.txt
- https://github.com/arduino/ArduinoCore-avr/blob/master/boards.txt
- https://v2.tauri.app/learn/window-menu/

USB serialは取得した文字列を保持するが、一意性を仮定しない。実機COM3/8/9のdescriptorは未採取。同一descriptorの同型機への差し替えや、応答を偽装する機器の認証は対象外。過去個体の恒久記憶・左右割当は未実装。現在は一台監視のまま、列挙では他ポートを開かない。
