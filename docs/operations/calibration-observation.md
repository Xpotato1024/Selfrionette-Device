---
status: draft
owner: operations
last_verified: 2026-10-12
---
# 左右校正の後続観測手順（未実施）

実機操作の承認とexact target確定後に実施するための案。今回のtaskではUSB/serialを開かず、実測値を保存しない。

1. 無駆動でrobotから切り離す。一台ずつphysical device、stable ID、組立/firmware revision、host roleを確認する。
2. wirech0〜6と本人の実番号の対応を確認する。左右の番号は保持する。OS portや接続順から推測しない。
3. 無荷重でbootする。current firmwareのboot autotare、setup/status/warning、基準姿勢とepochを記録する。
4. 全chの基準分布とnoise/漂移を有限時間記録する。状態が不明・不安定なら先へ進まない。
5. 本人が指示できる既知方向を定義し、左右とも同じ方向・姿勢で比較する。穏やかな入力、保持、除荷を全ch記録する。
6. 対応する入力を複数回繰り返す。符号候補、除荷戻り、他chの応答を確認する。方向未知の観測だけでは絶対極性を断定しない。
7. 無理のない正方向の操作範囲をユーザー/姿勢/sessionごとに測定する。逆方向を試せない場合は未測定とする。
8. 元firmware sample、baseline、符号、span、補正値、source/provenanceをpreviewする。手動変更後は再確認する。
9. 品質・由来・個体対応が揃い、別途定義する実機verified条件を満たした場合だけ実profileとして保存する。schema validだけで運用許可にしない。

reset、tare、reconnect、spike、timeout、clipping、除荷で戻らない区間は判定から除外し、基準からやり直す。
calibration_endだけで全ch成功としない。現行streamにはfilter前原本や十分な全品質情報がないため、
必要な情報が取れなければinconclusiveのまま保持する。
軸/荷重経路が変わっている場合はsignだけで解決せず、本人の構造確認と別mapping設計へ戻る。
N/forceの物理校正には既知の物理荷重・単位・scale・妥当性の別契約が必要。操作spanと混同しない。
