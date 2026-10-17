---
status: verified
owner: apps/device-manager
last_verified: 2026-10-16
---
# AVRDUDE Windows runtime provenance

The Windows x64 updater runtime is the unmodified MSVC archive published for AVRDUDE v8.3.

- Release: <https://github.com/avrdudes/avrdude/releases/tag/v8.3>
- Runtime archive: <https://github.com/avrdudes/avrdude/releases/download/v8.3/avrdude-v8.3-windows-x64.zip>
- Archive SHA-256: `8630c7d8bb9682a1685aaa9ad42550ae1131c9d13528f0d20975368ba615e9b0`
- Upstream source commit: `f139b390a6821039f5e4a2693f179803a85117b0`
- License: GNU General Public License, version 2; the build downloads the exact upstream `COPYING` file from that immutable source commit and bundles it with the notices.

The release packager rejects a missing or different archive, extracts only `avrdude.exe` and `avrdude.conf`, and records their SHA-256 digests. The runtime binary and configuration are copied unchanged into the application resources. The app verifies both hashes before an update.

## 対応ソースの同梱

バイナリと同じcommit `f139b390a6821039f5e4a2693f179803a85117b0`のソースZIPを`distribution/firmware-tools/AVRDUDE-SOURCE.zip`へ同梱する。取得・インストール後にSHA-256 `aa255d1750e168226d31dcb44be723c4023503b3a0805447032567da0febe455`を照合する。COPYINGは原本をそのまま保存し、見出し検査だけが先頭空白を許容する。
