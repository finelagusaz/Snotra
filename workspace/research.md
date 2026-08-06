# research — #900 `ctx.set_visuals` の禁止を clippy の disallowed-methods で機構化する

## issue の要約

#751 の修正（`ADR-visuals-application-target`）で「`ctx.set_visuals` を使ってはならない」という規範が
1 本増えた。現在それを止めているのは `src-tauri/CLAUDE.md` と `view.rs` のコメントだけである。
`src-tauri/clippy.toml` の `disallowed-methods` で機構化する（このリポジトリ初の `clippy.toml`）。

**射程は「禁止」だけである。** #751 が同時に新設したもう 1 つの不変条件——「適用は visuals を読む
最初の操作より前」という**順序**——は `disallowed-methods` では守れない。ゆえに
`src-tauri/CLAUDE.md`「テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）」の
**「この順序に検知手段は無い」は本 issue 実装後も正しいままであり、書き換えてはならない**（issue 明記）。

## 関連ファイル・モジュール・シンボル

| パス | 役割 |
|---|---|
| `src-tauri/clippy.toml` | **新規作成**。`disallowed-methods` の宣言 |
| `src-tauri/CLAUDE.md` | 「テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）」節へ 1 文追加 |
| `src-tauri/src/egui_shell/view.rs` | 適用点（`let visuals = ui.visuals_mut();`・`:502`）。**フォールトインジェクションの注入先**。コメントは正本なので変更しない |
| `snotra-settings/src/app.rs:52` | `ctx.set_visuals` の**正当な**使用（起動時 1 回の静的テーマ）。巻き込んではならない |
| `snotra-settings/src/style.rs:81` | `ctx.all_styles_mut` の**正当な**使用（TextStyle の size 書き換え）。同上 |
| `.github/workflows/ci.yml:126` | `cargo clippy --workspace --all-targets -- -D warnings`（既存・変更不要） |
| `.claude/hooks/post-edit.mjs` の `selectChecks` | `.rs` 編集で clippy が自動発火（既存・変更不要） |

## 実測（2026-08-06・すべてこのセッションで自分で測った）

### 測定 1 — global style を書く「足」の完全な列挙（egui 0.35.0 `context.rs`）

issue は `set_visuals` 1 件を提案するが、`safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに
検算する」（#858）に従い母集団を数え直した。**#751 の欠陥は `set_visuals` 固有ではない**——root `Ui` が
pass 冒頭で `ctx.global_style()` を `Arc<Style>` snapshot する以上、`Context` 経由の style 書き込みは
**すべて**当該 pass に届かない。ソースを読んで確認した writer は次の 7 つで、いずれも `options_mut` を
通って `opt.dark_style` / `opt.light_style` を書く。

| メソッド | `context.rs` | 実装 |
|---|---|---|
| `set_visuals` | 2212 | `style_mut_of(theme(), \|s\| s.visuals = v)` |
| `set_visuals_of` | 2199 | `style_mut_of(theme, ...)` |
| `style_mut_of` | 2169 | `options_mut` → `Arc::make_mut(dark/light_style)` |
| `set_style_of` | 2182 | `options_mut` → `dark/light_style = style` |
| `global_style_mut` | 2121 | `options_mut` → `Arc::make_mut(opt.style_mut())` |
| `set_global_style` | 2132 | `options_mut` → `*opt.style_mut() = style` |
| `all_styles_mut` | 2145 | `options_mut` → dark/light 両方を `make_mut` |

**`ADR-visuals-application-target` の列挙を流用してはならない**——あちらは global style の *reader* を
数えて 0 件を出した。ここで要るのは *writer* の母集団であり、別物である。

> **この 7 件は母集団として不完全だった**（独立導出レビューが指摘・自分で再照合して確定）。
> **clippy は呼び出し点の def_id しか見ず callee（別 crate）の中身はリントしない**ので、
> 「内部で他の書き込み口を呼ぶ `Context` のメソッド」も同じ抜け道になる。名前から選ぶ限り到達できない
> 3 件が在る: `style_ui`（`:3564-3568` → `set_style_of`）・`settings_ui`（`:3187-3200` →
> `options_mut(\|o\| *o = options)`）・`set_debug_on_hover`（`:3072-3074` → `all_styles_mut`）。
>
> **ただし、この 3 件はいずれも最終的な禁止集合に入らなかった。** 縮小レンズと codex が独立に
> 「`style_ui` / `settings_ui` は**ウィジェットを描く API** であり、呼ぶ人は inspector を出したいので
> あって #751 の誤りを犯していない。禁止すれば偽陽性になり `#[allow]` を訓練する」と結論した。
> **母集団としてはこの 7 件が正しく、採否の理由は `plan.md`「禁止集合 — 7 メソッド（確定）」を正本とする。**

### 測定 2 — src-tauri での現在の使用数（`grep -rn "\.<name>(" <crate>/src --include=*.rs`）

| メソッド | src-tauri | snotra-settings | snotra-egui-runtime |
|---|---|---|---|
| 上表の 7 メソッド全部 | **0** | `set_visuals` 1 / `all_styles_mut` 1 | 0 |
| `set_theme` / `options_mut` / `style_mut` | 0 | 0 | 0 |

src-tauri の `global_style` 3 件は**すべてコメント内**（`view.rs:23` / `:482` / `:1365`）。
ゆえに 7 メソッドを禁止しても既存コードの書き換えは 1 行も要らない。

### 測定 3 — フォールトインジェクション（7 足すべてが赤になるか）

`src-tauri/clippy.toml` へ 7 パスを置き、`view.rs` の `ui.visuals_mut()` 直前へ 7 呼び出しを注入して
`cargo clippy --workspace --all-targets --message-format short -- -D warnings` を実行（PostToolUse hook が自動実行）。

```
src-tauri\src\egui_shell\view.rs:506:15: error: use of a disallowed method `egui::Context::set_visuals`
src-tauri\src\egui_shell\view.rs:507:15: error: use of a disallowed method `egui::Context::set_visuals_of`
src-tauri\src\egui_shell\view.rs:508:15: error: use of a disallowed method `egui::Context::set_global_style`
src-tauri\src\egui_shell\view.rs:509:15: error: use of a disallowed method `egui::Context::global_style_mut`
src-tauri\src\egui_shell\view.rs:510:15: error: use of a disallowed method `egui::Context::all_styles_mut`
src-tauri\src\egui_shell\view.rs:511:15: error: use of a disallowed method `egui::Context::set_style_of`
src-tauri\src\egui_shell\view.rs:512:15: error: use of a disallowed method `egui::Context::style_mut_of`
error: could not compile `snotra` (bin "snotra") due to 7 previous errors
```

exit 101。**7 足すべてが捕まり、7 つの path 表記すべてが解決された**（誤記があれば無言で無視されうるので、
これは path 文字列自体の一次証拠でもある）。**bin と bin-test の両ターゲットで落ちた**（`--all-targets`）。

### 測定 4 — 負の方向（`snotra-settings` が巻き込まれないこと）

`cargo clippy -p snotra-settings --all-targets --message-format short -- -D warnings`
→ **EXIT=0 / `disallowed` を含む行 0 件**（出力はファイルへ落として exit code をパイプ越しに読まない形で測定）。
settings 側には `set_visuals` と `all_styles_mut` の**実使用が 2 件ある**にもかかわらず 0 件なので、
`clippy.toml` が package スコープであることの強い証拠になっている（変異を足す必要すらなかった）。

測定後、`view.rs` は `git checkout --`、`clippy.toml` は `rm` で撤去し、作業ツリーを clean に戻した
（`git status --short` が空・`TEMP MEASUREMENT` の grep が 0 件）。

### 測定 5 — 面積 cap（issue コメントの警告は陳腐化している）

`npm run governance:check` → 全検査 passed。**常時ロード 13596/15500 字・rules 10479/12000 字**。

issue コメントの「rules 9199/9200・残り 1 文字」は ±100 字 ratchet 時代の値で、その後 cap は実測の
約 1.3 倍（火災報知器・`ADR-doc-promise-over-area-ratchet`）へ置き換わっている。**rules の余裕は 1521 字**。
さらに `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`（`governance-check.mjs:824`）は**ルート 2 文書のみ**で、
`src-tauri/CLAUDE.md` は**どちらの面にも算入されない**。ゆえに作業項目 2・3 に面積上の制約は無い。

### 測定 6 — 環境

`clippy 0.1.94 (4a4ef493e3 2026-03-02)` / `rustc 1.94.0`。`src-tauri/Cargo.toml` は
`egui.workspace = true`（`= "0.35.0"` ピン）を**直接依存**に持つので、`egui::Context::*` のパスが解決する。

## 再利用できる既存パターン

- **CI・hook の再利用**: `disallowed_methods` は warn 既定で、CI（`ci.yml:126`）と PostToolUse hook の
  clippy がともに `-D warnings` を付けているため、**配線は 1 行も要らない**。
- **ワークスペース lints との棲み分け**: ルート `[workspace.lints.rustdoc]` は全 member 共通の deny
  （`G-workspace-lints` が opt-in を検査）。今回は**逆に crate ごとに分けたい**（settings は正当に使う）ので、
  workspace lints ではなく package スコープの `clippy.toml` が正しい道具である。
- **フォールトインジェクションの位置づけ**: 意図的に規則違反となる操作を行って拒否を確認する類は
  ガードの**行使**であり、`safety-nets.md`「稼働中のガードを弱めない」の対象外（#482 で明示）。

## 技術的制約・受容する残余

1. **`src-tauri/clippy.toml` に PostToolUse 検査は割り当てられない。** `selectChecks`
   （`post-edit.mjs:125`）の `config.toml` パターンは `(^|\/)(tauri\.conf\.json|config\.toml)$` で
   `clippy.toml` に一致せず、`CARGO_MANIFEST` も `Cargo.toml` 限定。**編集しても何も走らない＝沈黙は
   「合格」ではなく「何も走らなかった」**（`CLAUDE.md`「フック」）。clippy は手で回す。
2. **`ctx.options_mut(|o| o.dark_style = ...)` は素通りする。** `options_mut` は zoom 等の正当な用途を
   持つ汎用 API なので禁止対象に含めない（含めれば将来の正当な使用を塞ぐ）。受容する残余。
3. **`#[allow(clippy::disallowed_methods)]` で回避できる。** 任意の lint に内在する性質であり、
   回避がレビューで可視になる分むしろ望ましい。受容する残余。
4. **`set_theme` は含めない。** style の中身ではなく theme preference の切り替えであり、#751 の症状
   （色が旧のまま残る）とは別の操作である。src-tauri は theme preference を使わない（config 由来の色を
   直接適用する）ので、現状使用 0 件・将来も要らない。含めると reason 文と対象がずれる。
5. **`snotra-egui-runtime` は射程外。** 同じ欠陥を持ちうるが writer の使用は 0 件で、issue の射程は
   src-tauri である。ここで広げない。

## 測定 7 — 沈黙経路（不正パスは CI を赤にしない・独立導出レビューの指摘を自分で実測）

`CLIPPY_CONF_DIR` で scratchpad の `clippy.toml` を実 repo の `snotra` へ外部注入（リポジトリ無改変）。
正しい 4 パスに書き損じ 3 種を混ぜた。

```
$ CLIPPY_CONF_DIR=<scratchpad>/conf-typo cargo clippy -p snotra --all-targets -- -D warnings …
warning: `egui::Context::set_visualz` does not refer to a reachable function
warning: `egui::Contextt::set_visuals` does not refer to a reachable function
EXIT=0
```

- **メソッド名・型名の書き損じ** → warning は出るが **`-D warnings` でも exit 0**（CI は緑）
- **crate 名の書き損じ**（`eguii::Context::set_visuals`）→ **診断そのものが出ない**（上の 2 件だけが鳴った）
- 併せて `style_ui` / `settings_ui` / `set_debug_on_hover` の 3 パスが**警告を出さなかった**＝
  dev プロファイルでは**解決する**ことも確認できた（`set_debug_on_hover` は `#[cfg(debug_assertions)]`）

**hook は exit code で検出し成功時は何も出力しない**契約なので、この warning はエージェントにも届かない。
**沈黙は二重である**——詳細と帰結は `plan.md`「沈黙経路」。

## 未解決の疑問（すべて `plan.md`「未確定」で解消済み・**採否の正本は `plan.md`**）

- 禁止対象の集合 → **7 件**（測定 1 のとおり。`style_ui` / `settings_ui` / `set_debug_on_hover` は
  検討のうえ除外した——理由は `plan.md`「禁止集合」）
- `.claude/rules/safety-nets.md` の `paths` へ足すか → **足さない**
- `snotra-egui-runtime` にも置くか → **置かない（射程外・同 crate では `run_ui` 前の書き込みが正当）**
