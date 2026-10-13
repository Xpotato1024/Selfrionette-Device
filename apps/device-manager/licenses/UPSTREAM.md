# 配布パッケージで追加する第三者ライセンス本文

Cargoから取得した一部の公開crateは、トップレベルにLICENSEファイルを含みません。本ディレクトリは、その依存パッケージの公開ライセンス宣言に対応する上流テキストを**変更せず**固定保存したものです。

| 利用パッケージ | 宣言 | 同梱する本文 | 取得元 | Git blob SHA |
|---|---|---|---|---|
| alloc-stdlib 0.3.0 | BSD-3-Clause | alloc-stdlib-BSD-3-Clause.txt | [Dropbox](https://github.com/dropbox/rust-alloc-no-stdlib/blob/master/LICENSE) | `cd496ba72eb37e83a44958358d1f89a8a28cbc15` |
| defmt-parser 1.0.0 | MIT OR Apache-2.0 | defmt-MIT.txt | [defmt (MIT側を採用)](https://github.com/knurling-rs/defmt/blob/main/LICENSE-MIT) | `e68a7ab37c8accca023db5f1505d48e4375d4324` |
| selectors 0.38.0 | MPL-2.0 | servo-MPL-2.0.txt | [Servo](https://github.com/servo/servo/blob/main/LICENSE) | `d8b475b6e17e390d702c9be4d3a735c7622b9735` |
| webview2-com / -sys 0.39.1、-macros 0.8.1 | MIT | webview2-rs-MIT.txt | [webview2-rs](https://github.com/wravery/webview2-rs/blob/main/LICENSE) | `de9a7e660e173136b351982a34f715af424ff812` |

配布時は `THIRD-PARTY-NOTICES.txt` にライセンス本文を入れ、`DEPENDENCIES.json` で元のcrate・バージョン・source archive・SHA-256を明示します。MPL宣言またはcrate内にライセンス本文がない依存については公開された元の`.crate`も同梱します。

新しいバージョンの依存が追加された場合に、以前のライセンス本文を自動で流用しないでください。宣言が変わった場合はビルドを拒否します。これは個別の著作権・再配布条件を全自動で判定する仕組みではありません。
