---
status: evidence
owner: apps/device-manager
date: 2026-10-13
related:
  - docs/operations/device-manager-distribution.md
---
# 発行元変更後のNSIS更新失敗

## 原因と再現
preview.1はpublisher=selfrionette、preview.2はpublisher=Xpotato.net。
Tauri CLI 2.12.2の標準NSISは製品名のUninstallStringから旧アンインストーラーを取得する一方、保存先はpublisherを含むMANUPRODUCTKEYから読む。
発行元変更後は後者が空になり、旧uninstall.exeへ空の末尾引数 `_?=` を渡していた。
LLM-01の既存インストールがないことを確認し、専用ディレクトリへ過去のローカル配布物preview.1/b59c3e21を導入してpreview.2/d5f1802eを起動。同じNSIS Errorを再現し、子プロセスの実command lineから空の引数を確認した。
同じ識別子を維持するだけではNSIS独自のpublisher別保存先を維持できず、前回の新規導入・単独削除の試験では更新経路を検証できていなかった。

## 修正
表示用のpublisherはXpotato.netを維持し、内部bookkeepingはBUNDLEID配下のInstallerへ固定。
既存の製品別ARPからInstallLocationとUninstallStringを読み、同一製品・非空・絶対パス・実ファイル一致を検証する。
引用符を除いた旧保存先を末尾の `_?=` に渡す。保存先復元も同じ検証済み値を使う。
不整合や欠落時は更新前に中止。読取り・取消ではレジストリ移行を行わず、データを推測削除しない。
採用したTauriテンプレートの版・原本hash・MIT licenseを保持し、CLI更新時にはテンプレートの見直しを要求する。

## 検証の区別
配布設定17件と、productionのNSIS読取関数を実行する隔離レジストリ/パス13ケースを確認した。
パス試験は日本語・空白・外側引用符・未登録・欠落・相対パス・製品不一致・実行引数混入・実ファイル不在を含む。無効な登録が削除操作へ到達しないことを確認する。
旧版の実バイナリを使うローカル試験と、現行setupに旧版レジストリ形式を与えるCI試験は区別する。CIでは取消、実GUI経由の事前アンインストール、同一版再導入、記録保持を検証し、UPGRADE-VALIDATION.jsonへ範囲と結果を保存する。
実インストール済み/ZIP展開後のアプリ起動・操作試験と最終アンインストールも維持する。

## 試験ツール側の修正
新しい更新試験では、旧版アンインストーラーの言語選択、半角カナのアンインストールボタン、非アクティブになった親ウィザードへの操作を扱う必要があった。
試験側は文字の互換正規化、言語選択のOK、所有するウィンドウの有効ボタンに対する通知を用いる。未知の画面や無期限の待機を成功扱いしない。
NSIS ErrorのOKはID=2の場合があり、ID=1と決め打ちしない。これらは試験側の対応であり、製品側へ画面自動操作を組み込まない。

## 残り
利用者本人のPCでの更新完了は別受入。壊れた登録情報を自動削除して修復したと主張しない。
未署名、記録形式の互換性、実機校正/Firmware機能の範囲は変更しない。
最終headのCIと実配布物のhashはPR #12の受入記録を参照する。
