# ADR-config-read-without-exception: config の読みの規範から条件を無くし、担い手を lint からコンパイラへ移す

日付: 2026-08-18 ／ 状態: 承認

## 文脈

#1032 が「config の live-read は `engine.lock()` を経てはならない」を定めて以来、この規範は**どの読みを免除するか**を切り続けてきた。#1032 は場所（`commands/` の操作時の読み）で切り、#1076 は走るスレッドで切り直し（却下した案 A〜G は `ADR-config-read-exception-discriminator`）、#1122 が `clippy.toml` の `disallowed-methods` で機構化して、免除された地点は `#[expect(clippy::disallowed_methods, reason = …)]` が分類を記録して開けた。

#1123 は「その装置を置かないか」を問うた。**#1076 のサイクルでは一度も俎上に載せていない案**であり、同 ADR が「却下したのではなく評価していない」と明記していた。

評価の結果、免除されていた 4 か所（`resolve_opener` / `resolve_all_openers` / `ensure_icon_cache_loaded_if_enabled` / `config_watcher` の旧 config 読み）はいずれも engine の錠を config を読むためだけに取っており、`&AppState` を既に手に持っていた。**移せない技術的理由は 1 つも無かった。**

## 決定

**4 か所すべてを engine 錠の外へ出し、`Engine::config` を `#[cfg(test)] pub(crate)` で閉じる。** 規範を守るのは lint ではなくコンパイラになり、`clippy.toml` の当該群・カナリア登録・`#[expect]` はすべて不要になる。条項からは条件（免除の切り方）が消え、太字節は 25 から 13 へ縮んだ。

## 却下した代替案

- **案 1: 免除を残す（#1076 の判断を保つ）**: 却下。得るものが「4 か所が engine 錠を取り続けること」しか無い一方、費用は条項の 12 節（弁別子の宣言・動機と判定の分離・`on_event_loop` / tao / `app.listen` の非自明な 3 経路・`get_instant_commands` の穴の説明・hotkey の先例・読者に残る判定責務）と `clippy.toml` の 60 行と `#[expect]` 4 件である。**免除された 4 か所は規則の正しい適用結果であって、それ自体は欠陥ではない**——問題は規則が装置を必要としていたことだった。
- **案 2: 読み口を `state.config.read()` の直読みにする**（issue 本文が想定し、独立導出も「推奨・最小」とした形）: 却下。理由は 3 つ。(1) 生の guard はスコープ末まで生きるローカル束縛になり、**後から guard の後ろへ I/O を足せてしまう**——`is_dir()` が死んだ UNC で最大 21 秒塞ぐ（#524）この crate では、そこが唯一の実危険である。クロージャ形は guard の寿命を構文で閉じる。(2) 製品コードの read guard 取得点が 1 から 4 へ増える（移設前の 1 は実測）。(3) **src-tauri の config の読みはすべてクロージャ形であり**、この 4 か所だけ生 guard にすると唯一の書き方の例外になる——条件を消す変更で書き方の条件を作ることになる。
- **案 3: 4 か所の引数を `&AppHandle` へ替えて既存の `egui_shell::read_config` を使う**: 却下。3 か所は `&AppState` しか持たず、`&AppHandle` を通すと**到達しない `fallback` を 3 つ捏造することになる**。とくに `icon_cache_cap` の fallback は `Config::default()` を建てるので、走査パスの `exists()` と OS ロケールの読み（I/O）を、決して走らない経路のために書くことになる（同型の費用を `ADR-config-default-fallback-references` が `LazyLock<Config>` の却下理由として既に挙げている）。
- **案 4: 撤去条件が指定する `pub(crate)` へ落とす**: 却下——**指定どおりでは実装が止まる**。移設後 `Engine::config` の読み手は snotra-core 自身のテストだけになり、`pub(crate)` では lib ターゲットで `dead_code` が立って `-D warnings` の下で赤くなる。`#[cfg(test)]` を足すと消えることを実測した。**`#[cfg(test)]` のほうが強くもある**——製品ビルドにシンボルが存在しないので、可視性の違反ですらなく未定義メソッド（`E0599`）になる。同じ判断の先例は同 crate の `search.rs` の `sorted_prefix_len`（「製品から届く綴りを増やさないほうが、禁止を 1 つ足すより強い」）。

## 撤去条件そのものが持っていた欠陥

`clippy.toml` の当該群は自分の撤去手順を書いていた。**その手順には 4 つの欠陥があった**。手順は本サイクルで消えるので、記録の置き場はここしかない。

1. **`pub(crate)` へ落とすよう指定していた**（案 4）。同じファイルの別の群が `#[cfg(test)]` という正解を書いていたのに、撤去条件へ引き継がれていなかった。
2. **test fixture が列挙から漏れていた。** `G-clippy-disallowed.test.mjs` の緑 fixture が同じパスをリテラルで持つ。沈黙はしない（vitest が赤くなる）が、手順の完全性は失われていた。
3. **散文の掃除が一切列挙されていなかった。** 実際には 9 か所（条項本体・`CLAUDE.md` の icon 破棄の機序・`config_watcher.rs` の同じ機序の写し・`commands/instant.rs` の doc・`launcher_controller.rs` の 2 か所・`docs/architecture.md`・`docs/build-commands.md`・`G-clippy-disallowed.mjs` のヘッダ）を直す必要があった。**どの機構も見ていない**——`#[expect]` の reason 内に書かれた見出し参照はバックティックが無く、`G-heading-refs` の正規表現に掛からない。
4. **合図が駆動力を持たない。** 「最後の `#[expect]` が消えたら」は、消すのが撤去する当の変更なので自分では発火しない（`scaffold-removal-condition-self-reference` と同型）。**これは受容が妥当である**——撤去条件は「そのとき何をすべきか」を伝えるためのものであって、撤去を起こすためのものではない。

## 帰結

- **規範は機構より広いまま残る。** 塞がるのは `Engine::config` という綴りだけで、`engine.lock()` 越しに `config_handle().read()` を取り直す形は**今も通る**（実測）。ゆえに「engine 錠越しの config の読みは書けなくなった」とは言えない。
- **guard の中に I/O を書く形は構造では止まらない。** クロージャ形が保証するのは guard を外へ持ち出せないことだけである。`is_dir()` を読みの前に置く規律は文書契約のまま残り、宛先が engine 錠から config の `RwLock` へ変わっただけである。
- **口は 2 つになり、read guard を取る地点は 1 つのままである。** `AppState::read_config` が唯一の取得点で、`egui_shell::read_config` はその委譲になった（見るのは `AppState` 不在の面倒だけ）。
- **機構の乗り換えは測ってある。** 回帰の形を注入すると `cargo check` が `E0599` で落ちる。旧 lint と違い clippy を要さない（`clippy::` ツール lint ではなくなったため）。
