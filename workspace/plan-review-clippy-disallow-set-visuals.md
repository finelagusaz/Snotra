# 独立導出レビュー — #900 `ctx.set_visuals` の禁止を clippy の `disallowed-methods` で機構化する

対象 issue: **#900**（`ctx.set_visuals` の禁止を clippy の disallowed-methods で機構化する）
導出日: 2026-08-06 / 導出者: 独立枠（`workspace/plan.md`・`workspace/research.md` を読まない前提で起動）

---

## 0. 汚染の開示（先に読むこと）

**`workspace/plan.md` と `workspace/research.md` を Read してはいない**が、
`grep -rn "set_visuals\|…" .`（リポジトリ全体・拡張子未指定）を 1 回打ったことで、
両ファイルの**該当行が grep 出力として文脈へ流入した**。流入した内容には、plan が挙げる
7 メソッドの一覧・`options_mut` / `set_theme` を除外した旨・「settings は巻き込まれない」旨・
実測ログの断片が含まれる。

**どこまでが流入前の導出か**——`egui-0.35.0/src/context.rs` の `pub fn` 一覧の取得と
`:2080-2230` の読解は、**この grep と同一メッセージで並行発行した**バッチの中にある。
すなわちメソッド列挙の一次証拠は流入前に手元にあった。以降の測定（clippy.toml のスコープ規則・
不正パスの扱い・UFCS・`EguiView::setup` の呼び出し位置）はすべて本レビューが自前で実施した。

**突き合わせ材料としての価値は「一致部分」ではなく「差分」にしかない**ため、
plan と食い違う所見を §1・§3 の冒頭に置いた。要点は 2 つ:

1. **候補が 3 件漏れている**——`Context::style_ui`（`context.rs:3564`）・
   `Context::set_debug_on_hover`（`:3072`）・`Context::settings_ui`（`:3187`）。
   いずれも内部で書き込み口（`set_style_of` / `all_styles_mut` / `options_mut`）を呼ぶ
   `Context` の inherent method であり、**clippy は呼び出し点しか見ない**ので
   7 件を禁じても素通りする（→ §1.1 の規則と §1.3 の 8〜10）。
   **「global style を書くための API」を名前で選ぶ限り、この 3 件には到達しない。**
2. **「巻き込んではならない正当な使用」は snotra-settings だけではない。** `src-tauri` 自身に
   `EguiView::setup`（`run_ui` の**外**で呼ばれる）という**欠陥を持たない書き込み地点が 2 つ**
   構造的に存在する（→ §3-B）。「src-tauri では常に誤り」という前提は成り立たない。

---

## 1. 禁止すべきメソッドの完全な列挙

### 1.1 母集団の取り方（全称主張の前提）

`egui::Context` の inherent method は**すべて `context.rs` の中にある**。

```
$ grep -rn "impl Context" egui-0.35.0/src/
  → 18 ヒット、全件 context.rs（他ファイルに `impl Context` は無い）
```

ゆえに「`context.rs` の `pub fn` を数え上げれば `Context` のメソッドは尽きる」が言える。
その一覧（`grep -n "pub fn \w+" context.rs`）から、`Memory::options` の
`style` / `dark_style` / `light_style` へ**書く**ものを本体の実装で選別した。

**選別の規則（これを §1.3 の表より上位に置く）**:

> **`Options` の style へ着地する `Context` のメソッドは、直接書くものだけでなく
> 「内部で他の書き込み口を呼ぶもの」も同じ抜け道になる。**
> clippy の `disallowed_methods` は**呼び出し点の def_id 一致**でしか判定せず、
> callee の中身（別 crate）はリントされないためである。

この規則を立てて初めて `style_ui`（`set_style_of` を呼ぶ）・`set_debug_on_hover`
（`all_styles_mut` を呼ぶ）・`settings_ui`（編集済み `Options` を `options_mut` で丸ごと書き戻す）が
拾える。**「global style を書くための API」を名前から選ぶだけでは 3 件取りこぼす。**

### 1.2 欠陥の機序（なぜ「`set_visuals` 固有」ではないのか）

- `context.rs:788` — `run_ui_dyn` が `Ui::new(...)` で root `Ui` を作る
- `context.rs:798` — **その後で** user callback `run_ui(&mut root_ui)` を呼ぶ
- `ui.rs:135` — `Ui::new` は `let style = style.unwrap_or_else(|| ctx.global_style());`
- `context.rs:2107-2109` — `global_style()` は `options(|opt| Arc::clone(opt.style()))`

root `Ui` は pass 冒頭で `Arc<Style>` を**掴んで**しまうため、callback 内から `Options` 側の
style をどう書いても当該 pass には届かない。**この機序は書き込み口の名前に依存しない**——
`Options` の style へ着地する経路はすべて同じ欠陥を持つ。

### 1.3 該当するメソッド（10 件）

| # | メソッド | `context.rs` | 実装（どこへ書くか） | 判定 |
|---|---|---|---|---|
| 1 | `set_visuals` | 2212-2214 | `style_mut_of(self.theme(), \|s\| s.visuals = visuals)` | **該当** |
| 2 | `set_visuals_of` | 2199-2201 | `style_mut_of(theme, \|s\| s.visuals = visuals)` | **該当** |
| 3 | `style_mut_of` | 2169-2174 | `options_mut` → `Arc::make_mut(&mut opt.dark_style / light_style)` | **該当** |
| 4 | `set_style_of` | 2182-2188 | `options_mut` → `opt.dark_style / light_style = style` | **該当** |
| 5 | `global_style_mut` | 2121-2123 | `options_mut` → `mutate_style(Arc::make_mut(opt.style_mut()))` | **該当** |
| 6 | `set_global_style` | 2132-2134 | `options_mut` → `*opt.style_mut() = style.into()` | **該当** |
| 7 | `all_styles_mut` | 2145-2150 | `options_mut` → `dark_style` と `light_style` の両方を `make_mut` | **該当** |
| 8 | `style_ui` | 3564-3568 | `style_of(theme)` を clone → `Style::ui(ui)` → **`set_style_of(theme, style)`** | **該当（境界事例 A）** |
| 9 | `set_debug_on_hover` | 3072-3074 | **`all_styles_mut(\|style\| style.debug.debug_on_hover = …)`** | **該当（境界事例 A′）** |
| 10 | `settings_ui` | 3187-3200 | `options(\|o\| o.clone())` → `Options::ui(ui)` が **`Arc::make_mut(dark_style / light_style)`** で style を編集（`memory/mod.rs:427-432`）→ **`options_mut(\|o\| *o = options)`** で書き戻す | **該当（境界事例 A″）** |

1〜7 は「global style を書くために存在する」API であり、判定に迷いは無い。
**8〜10 は名前から選べない**——§1.1 の規則（内部で書き込み口を呼ぶものも同じ抜け道）でしか拾えない。

### 1.4 境界事例（含めるか割れるもの）

#### A. `Context::style_ui`（`context.rs:3562-3569`）— ⚠ 割れる。**含めることを推す**

```rust
pub fn style_ui(&self, ui: &mut Ui, theme: Theme) {
    let mut style: Style = (*self.style_of(theme)).clone();
    style.ui(ui);
    self.set_style_of(theme, style);   // ← global style への書き込み
}
```

- **該当する理由**: `Context` の inherent method であり、内部で `set_style_of` を呼ぶ。
  **clippy は呼び出し点しか見ない**（`disallowed_methods` の判定は def_id 一致であり、
  callee の中身は別 crate ゆえリントされない）ので、1〜7 を禁じても
  `ctx.style_ui(ui, theme)` は素通りする。callback 内から呼べば書き込みは当該 pass に届かない。
- **含めない側の言い分**: 用途が debug/inspector UI であり、製品コードで呼ぶ動機が薄い。
  第 2 引数に `&mut Ui` を取る＝ pass の中で呼ぶことが前提の API なので、
  「callback 外での正当な使用」がほぼ無く、禁止しても失うものが無い（これは**含める側**の
  補強でもある）。
- **含める場合の帰結**: 禁止が 1 件増える。`src-tauri` の現在の使用は 0 なので即時の影響は無い。
- **含めない場合の帰結**: 「global style を書く Context メソッドを全部塞いだ」という主張が
  **偽になる**。7 件で「全部」と書けば、`AGENTS.md`「検証の作法」の
  「全称表現は前提条件とセットで書く。書けないなら書かない」に抵触する。
  除外するなら `clippy.toml` のコメントに `style_ui` を名指しで残余として書くこと。

#### A′. `Context::set_debug_on_hover`（`context.rs:3070-3074`）— ⚠ 割れる。**含めることを推す**

```rust
#[cfg(debug_assertions)]
pub fn set_debug_on_hover(&self, debug_on_hover: bool) {
    self.all_styles_mut(|style| style.debug.debug_on_hover = debug_on_hover);
}
```

- **該当する理由**: `all_styles_mut` を内部で呼ぶ＝ global style を書く。7 件を禁じても素通りする。
  書く先が `style.debug` である点は `ADR-visuals-application-target` の記述
  （egui 内部が global style から読むのは `interact_radius` / `text_options` / `error_fg_color` /
  `animation_time` / `scroll_animation` / `dark_mode` / **`debug.*`**）と整合し、
  **egui 内部が実際に読む項目**である。
- **含めない側の言い分**: 書くのは visuals ではなく debug フラグ。`#[cfg(debug_assertions)]` で
  release には存在しない。実害はデバッグ表示が 1 フレーム遅れることだけ。
- **⚠ 追加の注意（含めた場合）**: このメソッドは `#[cfg(debug_assertions)]` ゆえ、
  **`--release` で clippy を回すとパスが解決しなくなる**（→ §5-R1 の沈黙経路に乗る）。
  CI・hook はどちらも dev プロファイルで回すので現状は解決する（実測・§付録）が、
  将来 release で clippy を回す経路を足すとこの 1 行だけが黙って死ぬ。

#### A″. `Context::settings_ui`（`context.rs:3186-3200`）— ⚠ 割れる。**含めることを推す**

```rust
pub fn settings_ui(&self, ui: &mut Ui) {
    let prev_options = self.options(|o| o.clone());
    let mut options = prev_options.clone();
    …
    options.ui(ui);                                  // ← style を編集する
    if options != prev_options {
        self.options_mut(move |o| *o = options);     // ← style ごと書き戻す
    }
}
```

`Options::ui`（`memory/mod.rs:373-432`）は `CollapsingHeader::new("🎑 Style")` の中で
`Arc::make_mut(match theme { dark_style / light_style })` を取り、**style を直接編集する**。
`settings_ui` はその結果を `options_mut` で `Options` ごと書き戻すので、
**`options_mut` 経由の global style 書き込みそのもの**である。

- **含めない側の言い分**: debug/inspector 用の API であり製品コードで呼ぶ動機が無い
  （`src-tauri` の使用は 0 件）。
- **含める側の言い分**: A と同じ——「全部塞いだ」と書くなら含めねば偽になる。
  加えて §5-R2 で「`options_mut` 直書きは残余」と書く以上、**その残余へ通じる名前つきの口**を
  1 つだけ開けておく理由が無い。

#### B. `Context::set_theme`（`context.rs:2102-2104`）— ⚠ 割れる。**含めないことを推す**

```rust
pub fn set_theme(&self, theme_preference: impl Into<crate::ThemePreference>) {
    self.options_mut(|opt| opt.theme_preference = theme_preference.into());
}
```

- **該当しない理由**: style の**中身**を書かない。書くのは `theme_preference`（dark/light の選択）。
- **該当すると言える理由（⚠）**: **症状は同じである**。root `Ui` は `opt.style()`＝
  「その時点の theme が指す style」の `Arc` を掴むので、callback 内で theme を切り替えても
  **当該 pass の見た目は変わらない**。「書いたのに今フレーム効かない」という #751 の症状の
  観点では 1〜8 と同類である。
- **含めない場合の帰結**: `ctx.set_theme` を callback 内で呼ぶコードが将来入ると、
  1 フレーム遅れて切り替わる（黙って）。src-tauri の現在の使用は 0 件で、
  この crate は config の hex 色から直接 `Visuals` を作っており theme preference を使わない
  設計なので、実害の見込みは低い。
- **含める場合の帰結**: 禁止が 9 件になる。`reason` を「global style 書き込み」で統一できなくなり、
  文言を 2 系統に分けるか「pass 冒頭の snapshot に間に合わない」という上位概念へ寄せる必要がある。

#### C. `Context::options_mut`（`context.rs:1069-1073`）— **含めない（受容残余）**

`ctx.options_mut(|o| o.dark_style = ...)` は 1〜8 のすべてを迂回する。
だが `options_mut` は zoom・`theme_preference`・`max_passes`・`system_theme` 等を扱う汎用
アクセサであり、禁止すれば正当な用途を全部巻き込む。→ §5 の残余へ。

#### D. `Context::set_fonts`（`context.rs:2038`）/ `Context::add_font`（`context.rs:2061`）— **含めない**

どちらもフォント定義であって style ではない（書く先は `Memory::new_font_definitions` /
`Memory::add_fonts` であり `Options` の style ではない——実装で確認）。
**しかも `set_fonts` は `src-tauri` に実使用がある**
（`src-tauri/src/egui_shell/font_stack.rs:192`）。到達フレームが遅れる点は似ているが、
それは既知・既記述の事実であり（`view.rs:517` の
`// set_fonts は次フレーム適用——欠くと新フォントが 1 イベント遅れる` と、その直後の
`ctx.request_repaint()` が対処）、`disallowed-methods` に入れれば**現行の正しいコードが赤くなる**。
`add_font` は現在使用 0 件だが、同カテゴリゆえ同じく含めない。

#### E. `set_pixels_per_point` / `set_zoom_factor`（2228 / 2269）— **含めない**

zoom であり style ではない。doc 自身が `Will become active at the start of the next pass` と
遅延を明示しており、既知の契約として扱われている。

#### F. `Ui::style_mut` / `Ui::visuals_mut` — **対象外**

`Context` のメソッドではなく、#751 が**規定した置き換え先そのもの**。

---

## 2. リポジトリ内の現在の使用箇所

計測: `grep -rn "<8 メソッド> | set_theme | options_mut | set_fonts" --include=*.rs <crate>/`
（crate ごとに個別実行）。**コメント内の言及と実際の呼び出しを分けて記す。**

### snotra-core — **0 件（構造的に 0）**

`snotra-core/Cargo.toml` に **egui 依存が無い**（grep 実測）。
ゆえに「今 0 件」ではなく「egui の API を呼びえない」。

### snotra-egui-runtime — **実呼び出し 0 件**

`runtime.rs:384` で `egui::Context::default()` を作り `:447` で `run_ui` を回すが、
style を書く 8 メソッドの呼び出しは 1 件も無い。

### src-tauri — **実呼び出し 0 件 / コメント内の言及 16 行**

実呼び出し（10 メソッド）: **0 件**。
`set_fonts` の実呼び出しが `src-tauri/src/egui_shell/font_stack.rs:192` に 1 件あるが、
これは禁止対象外（→ §1.4-D）。

**この 0 件は grep ではなく clippy 自身で測った**（§3-A の `CLIPPY_CONF_DIR` 実測）。
10 件 + `set_theme` を `disallowed-methods` に入れて `cargo clippy -p snotra --all-targets` を
回し、`use of a disallowed method` が **1 件も出ないこと**を確認した。
grep は名前の一致しか見ないが、この測定は**型解決を経た呼び出し点**を見るので、
別名 import・UFCS・再エクスポート経由の呼び出しも同時に否定できる。

コメント内の言及（すべて `//!` / `//` / `///`。リントされない）:

| ファイル:行 | 種別 | 語 |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs:9` | `//!` | `ctx.set_visuals`（反映境界の列挙） |
| `src-tauri/src/egui_shell/view.rs:16-17` | `//!` | `ctx.set_visuals`（「全域 grep で 0 件」） |
| `src-tauri/src/egui_shell/view.rs:22-24` | `//!` | `ctx.set_visuals` / `ctx.global_style()` |
| `src-tauri/src/egui_shell/view.rs:410` | `//` | `ctx.set_visuals`（適用先の由来） |
| `src-tauri/src/egui_shell/view.rs:482-486` | `//` | `ctx.global_style()` / `ctx.set_visuals` |
| `src-tauri/src/egui_shell/view.rs:494` | `//` | `ctx.set_visuals`（順序不変条件の説明） |
| `src-tauri/src/egui_shell/view.rs:501-502` | `//` | `ctx.set_visuals`（消してよい根拠） |
| `src-tauri/src/egui_shell/view.rs:660` | `//` | `ctx.set_visuals` |
| `src-tauri/src/egui_shell/view.rs:1365-1371` | `///` | `ctx.global_style()` / `ctx.set_visuals`（テストの doc） |
| `src-tauri/src/egui_shell/font_stack.rs:1,46` | `//!` / `///` | `set_fonts` |

### snotra-settings — **実呼び出し 2 件**

| ファイル:行 | 呼び出し | 文脈 |
|---|---|---|
| `snotra-settings/src/app.rs:52` | `ctx.set_visuals(visuals)` | `fn apply_win11_theme(ctx: &egui::Context)` の末尾 |
| `snotra-settings/src/style.rs:81` | `ctx.all_styles_mut(\|style\| { … })` | `pub fn apply_type_ramp(ctx, heading_semibold)` |

（`snotra-settings/src/font.rs:94` に `ctx.set_fonts(fonts)` があるが禁止対象外。
`snotra-settings/SETTINGS-DESIGN.md:12` と `style.rs:72` はコメント／文書内の言及。）

---

## 3. 巻き込んではならない正当な使用

### 3-A. snotra-settings の 2 件 — **欠陥を持たない。理由は issue の説明より強い**

issue は「起動時に一度だけ設定する静的テーマで、色が動かないため #751 の症状を持たない」と書く。
**これは正しい結論だが理由が弱い**——「色が動かない」は症状が**見えない**理由であって、
欠陥が**無い**理由ではない。実際の理由は呼び出し**位置**である。

```rust
// snotra-settings/src/app.rs:665-678
eframe::run_native(
    title,
    options,
    Box::new(move |cc| {
        let heading_semibold = crate::font::configure_fonts(&cc.egui_ctx);
        apply_win11_theme(&cc.egui_ctx);              // :670 → app.rs:52 の set_visuals
        style::apply_type_ramp(&cc.egui_ctx, …);      // :671 → style.rs:81 の all_styles_mut
        Ok(Box::new(SettingsApp::new(…)))
    }),
)
```

どちらも **`eframe::run_native` の creation closure**（`CreationContext` を受ける）から呼ばれる。
これは最初の `App::update` より**前**＝**どの `run_ui` callback の中でもない**。
ゆえに書き込みは第 1 pass の `Ui::new` が `global_style()` を掴む**より前**に `Options` へ着地し、
**第 1 pass から効く**。#751 の欠陥は原理的に発生しない（色が静的かどうかとは独立）。

**`src-tauri/clippy.toml` という配置で保護されるか → はい。実測で確定。**

`clippy.toml` のスコープ規則は**一次情報として自前で測った**（リポジトリの作業ツリーには一切
触らず、scratchpad に使い捨て workspace を作って測定）。clippy 0.1.94 (4a4ef493e3 2026-03-02)。

```
scope/                       members = ["a", "b"]
scope/a/clippy.toml          disallowed-methods = [ std::string::String::len ]
scope/clippy.toml            disallowed-methods = [ std::string::String::capacity ]
scope/{a,b}/src/lib.rs       どちらも s.len() と s.capacity() を呼ぶ

$ cargo clippy --workspace --all-targets      # workspace ルートから実行
warning: use of a disallowed method `std::string::String::len`       --> a\src\lib.rs:2:7
warning: use of a disallowed method `std::string::String::capacity`  --> b\src\lib.rs:2:17
```

読み取れることが 2 つある。

1. **package スコープである**（求めていた性質）。`a/clippy.toml` の `len` は `a` でのみ発火し、
   `b` では発火しない。`--workspace` で 1 回のプロセスとして回しても混ざらない。
   ゆえに `src-tauri/clippy.toml` は `snotra-settings` を巻き込まない。
2. **設定はマージされない**（求めていなかった性質・§5 の残余へ）。`a` は自分の `clippy.toml` を
   見つけた時点で探索を止め、ルートの `capacity` を**受け取らない**。

**探索基準を上書きするものがリポジトリに無いことも確認した**——`CLIPPY_CONF_DIR` は
`*.mjs` / `*.ps1` / `*.yml` / `*.json` / `*.toml` の全域 grep で 0 件、`.cargo/config.toml` も
存在しない（`find` 実測）。ゆえに実 repo でも探索基準は各 crate の `CARGO_MANIFEST_DIR` である。

**さらに、その `CLIPPY_CONF_DIR` の不在を梃にして、実 repo でのパス解決を無改変で測った。**
scratchpad に本番候補の 11 パス（§1.3 の 10 件 + `set_theme`）と**陽性対照 2 件**
（`egui::Context::set_visualz` = メソッド名の書き損じ / `egui::Contextt::set_visuals` = 型名の
書き損じ）を書いた `clippy.toml` を置き、env で `src-tauri` へ外部注入した:

```
$ CLIPPY_CONF_DIR=<scratchpad>/conf cargo clippy -p snotra --all-targets -- -A clippy::needless_return
warning: `egui::Context::set_visualz` does not refer to a reachable function
warning: `egui::Contextt::set_visuals` does not refer to a reachable function
    Finished `dev` profile … EXIT=0
```

陽性対照 2 件だけが鳴り、**候補 11 件は 1 件も鳴らなかった**。読み取れることが 3 つある。

1. 設定は確かに読まれ、`snotra` は確かにリントされた（陽性対照が証明する）
2. **11 パスはすべて `src-tauri` の実依存グラフに対して解決する**（→ 旧「未検証-1」の主要部が消えた）
3. `use of a disallowed method` が 0 件＝**現在の src-tauri に違反は 1 件も無い**（→ §2 に反映）

注: package 名は `snotra` であってディレクトリ名 `src-tauri` ではない（`src-tauri/Cargo.toml`）。
`-A clippy::needless_return` は fingerprint を変えて再リントさせるためだけの無害なフラグである。

### 3-B. ⚠ **issue が見落としている正当な地点 — `EguiView::setup`（src-tauri 内）**

```rust
// snotra-egui-runtime/src/runtime.rs:380-386
fn new(window: tauri::Window, mut view: Box<dyn EguiView>) -> Result<Self, RuntimeError> {
    …
    let context = egui::Context::default();
    view.setup(&context);          // ← run_ui の外。フレームはまだ 1 枚も走っていない
```

`EguiView::setup(&mut self, _context: &egui::Context)`（`runtime.rs:23`）は
**`run_ui` の外**で呼ばれる。src-tauri はこれを 2 箇所で実装している:

- `src-tauri/src/egui_shell/view.rs:373` — `fn setup(&mut self, context: &egui::Context)`
- `src-tauri/src/egui_shell/results_view.rs:513` — 同上

**ここでの global style 書き込みは 3-A と構造的に同一で、#751 の欠陥を持たない。**
現在そのような書き込みは 0 件だが、**地点は構造として存在する**。しかも
`ADR-visuals-application-target`「帰結」が

> **global style はもう 3 値を持たない。** main 窓へ新しく egui コンテナを足すなら、その Ui へ
> 自分で visuals を渡す必要がある（`ctx.global_style()` から Ui を作るため既定色で描かれる）。

と書く問題に対して、**`setup` での一度きりの global style 書き込みは正攻法の解の一つである**
（新コンテナごとに visuals を手渡す代わりに、pass の外で土台色を決める）。
crate 全体の禁止はこの解を塞ぐ。

**帰結の書き分け**:

- 塞ぐと決めるなら、それは**意図した副作用**として記録すべきであり、`reason` の文言を
  「使ってはならない」ではなく **「callback 内からは当該 pass に届かない（#751）」** と
  条件つきで書く（回避が要るときに `#[allow]` + 理由コメントという正しい逃げ道が読める）。
- 塞がないと決めるなら、`clippy.toml` は crate 全体にしか効かない以上、
  **選択肢は「`setup` を持つファイルだけ `#[allow]`」しか無い**——これは事実上「禁止の穴を
  最初から開ける」ことなので、**塞ぐ側を推す**。ただし判断は明示的に下すこと。

### 3-C. テストターゲットも対象に入る（`--all-targets`）

CI（`.github/workflows/ci.yml:126`）と PostToolUse hook（`.claude/hooks/post-edit.mjs:308-312`）は
どちらも `cargo clippy --workspace --all-targets ... -- -D warnings` を回す。
ゆえに `src-tauri` の `#[cfg(test)]` も禁止対象になる。
`view.rs:1379` は `egui::Context::default()` を作って `run_ui` を 1 回だけ回すテストであり、
**「pass の前に global style を積んでから 1 pass 走らせる」形の対照テスト**（欠陥を持たない
正当な構成）を将来書こうとすると赤くなる。実害は小さいが、塞ぐ側の帰結として記録しておく。

---

## 4. 変更すべきファイルとシンボル

### 4.1 新規作成

**`src-tauri/clippy.toml`** — このリポジトリ初の `clippy.toml`。

- `disallowed-methods` に §1.3 の該当メソッドを列挙（**10 件**を推す。8〜10
  〔`style_ui` / `set_debug_on_hover` / `settings_ui`〕を落とすなら、
  落とした分を同ファイルのコメントに残余として名指しで書く）
- **11 パスすべてが実依存グラフに対して解決することは測定済み**（§3-A）。
  採用する集合を変えたら、その集合で測り直すこと（§5-R1 が沈黙経路を持つため）
- `reason` は**条件つきの文言**にする（→ §3-B）。全件共通で構わない
- **TOML コメントで「含めなかったもの」を記す**——`options_mut`（汎用アクセサ）・`set_theme`
  （theme preference であって style の中身ではない・ただし症状は同型）・`set_fonts`
  （src-tauri に正当な実使用あり）。否定の知識を残す場所がここ以外に無い

### 4.2 この変更で**偽になる／不正確になる**既存の散文

| 場所 | 現在の記述 | なぜ不正確になるか |
|---|---|---|
| `src-tauri/CLAUDE.md`「モジュール構成」内、`egui_shell/` の「テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）」の項 | 「**`ctx.set_visuals` を使ってはならない**」 | ①**規範だけで守っている**という含意が偽になる（機構が加わる）。②**名指しが 1 メソッドしか無い**——機構は 10 メソッドを拒むので、`ctx.global_style_mut` を書いた読者は文書に警告されないまま赤を踏む。**メソッド名ではなく類（`Context` 経由の global style 書き込み全般）で書き直す**のが正確な形 |
| 同上・同じ項 | 「**この順序に検知手段は無い**」 | **偽にならない。書き換えてはならない**（→ §4.4） |
| `src-tauri/src/egui_shell/view.rs:16-17`（`//!`） | 「**`ctx.set_visuals` は `src-tauri/src/` の全域 grep で 0 件である**」 | 字義としては真のままだが、**根拠の性質が変わる**（観測された 0 件 → 機構が保証する 0 件）。「grep で 0 件」は「今そうなっている」という観測の言明であり、機構が入った後もそう書き続けると**次の読者が機構の存在を知らないまま**この行を消したり戻したりしうる。1 語の追記で足りる（軽微） |

### 4.3 手順として踏むべきもの（成果物ではないが漏らすと機構が空になる）

- **`.claude/rules/safety-nets.md`「効いていることは、フォールトインジェクションで一度は実測する」**
  に従い、`view.rs` へ禁止対象を 1 行足して clippy が赤になることを実測する。
  同 rule「注入したことと、注入が正しい強さであることは別である」に照らすと、
  **注入は `set_visuals` 1 件では足りない**——**採用した全件**（本レビューの推奨なら 10 件）を
  注入して**全件が個別に赤を出すこと**を見る。1 件だけ測って「効いた」と読むと、
  パス文字列を書き損じた残りが黙って死ぬ（→ §5-R1 が現実の脅威にする）。
- **CI での実測は PR 本文のチェックリストへ送る**（`safety-nets.md`「CI の実測は PR が在って
  初めて行える」・`plan.md` に置くと `gh pr create` が未チェック `- [ ]` で拒む）。
- **`governance:check` は走らせるが、索引の追随は不要**（根拠は §4.4 末尾）。

### 4.4 この変更後も**書き換えてはならない**記述

いずれも「この変更で偽にならない」ことを独立に確認した。

1. **`src-tauri/CLAUDE.md`「この順序に検知手段は無い」（＋ `view.rs:494-497` の同旨コメント）**
   `disallowed_methods` は**呼び出しの有無**を def_id 一致で判定するだけで、
   `ui.visuals_mut()` が「visuals を読む最初の操作」より前にあるかという**位置**を見る述語を
   持たない。ゆえに順序不変条件は依然として検知器を持たない。issue の主張の追認ではなく、
   lint の仕組みから独立に導ける。

2. **`docs/adr/ADR-visuals-application-target.md`（全文）**
   `.claude/rules/governance-docs.md`「**ADR 本文内の参照は照合されない——凍結された歴史であり
   腐るに任せる**」。加えて内容面でも、この ADR が「見送り」と記録したのは**順序**の検知器で
   あって禁止の機構化ではないので、本変更後も偽にならない。

3. **`src-tauri/src/egui_shell/view.rs:480-502` の適用点コメント**
   `src-tauri/CLAUDE.md` がここを順序不変条件の**正本**と名指ししている
   （「正本は `view.rs` の適用点のコメント」）。機構は順序を守らないので、正本の役割は変わらない。

4. **`view.rs:9-11` の「反映境界は 5 つ（… `ctx.set_visuals` …）」の列挙 — ⚠ 判断が割れる。維持を推す**
   「禁止された API を反映境界として列挙し続けるのは誤読を生む」という読みは成り立つ。
   だが**この列挙は「visuals が画面へ届く経路の分類」であって「この crate が呼ぶ API の一覧」
   ではない**（同じ `//!` が :11 で「本ファイルが直接呼ぶのは 2 つ」と既に区別している）。
   `ctx.set_visuals` を列挙から抜くと、:22-24 の「**`ctx.set_visuals` へ戻すとこの対称が壊れる**」
   が指示対象を失う。**維持を推すが、採否は判断として明示すること。**

5. **`snotra-settings/src/app.rs:52` / `snotra-settings/src/style.rs:81` の実装と
   `snotra-settings/SETTINGS-DESIGN.md:12` / `style.rs:72` の記述**
   §3-A のとおり正当であり、§3-A の実測どおり `src-tauri/clippy.toml` は届かない。

6. **モジュール索引（`src-tauri/CLAUDE.md`「モジュール構成」のファイル一覧）**
   追随**不要**。`AGENTS.md` の当該トリガーは「ファイル（**`.rs`**）を追加/削除」であり、
   `G-module-index` の母集団も `.rs` 系に限られる（`scripts/governance-check.mjs:117`
   の順方向パターンは `` `…\.(?:rs|ts|tsx|html)` ``、`:129` の逆方向は `cfg.src` 配下 ×
   `cfg.exts`）。`src-tauri/clippy.toml` は `src-tauri/src/` の外にあり拡張子も対象外。

---

## 5. この機構が守れないもの（抜け道・受容せざるを得ない残余）

測定はすべて scratchpad の使い捨て workspace（clippy 0.1.94）で実施。リポジトリは無改変。

### R1. ⚠ **不正なパスは CI を赤にしない（＝検出器が黙って劣化しうる）— 要対処**

`clippy.toml` に解決できないパスを書くと clippy は診断を出すが、**`-D warnings` で
昇格されない**。禁止対象の呼び出しが 1 件も無い状態で測った（他のリントも出ない
最小コードにした）:

```
$ cargo clippy -p a --all-targets -- -D warnings
warning: `std::string::String::set_visualz` does not refer to a reachable function
warning: `std::strang::String::len` does not refer to a reachable function
    Finished `dev` profile …
EXIT=0
```

**さらに悪い分岐がある**——パスの**先頭セグメントが依存グラフに無い crate** のとき、
**診断そのものが出ない**。同じ `clippy.toml` に置いた `egui::Context::set_visuals`
（当該 crate に egui 依存が無い）と `eguii::Context::set_visuals`（crate 名の typo）は
**1 行も警告を出さなかった**。つまり:

- メソッド名・モジュール名の書き損じ → warning は出るが **exit 0**（CI は緑）
- **crate 名の書き損じ／egui 依存の消滅** → **完全な沈黙**

`src-tauri/Cargo.toml` は `egui.workspace = true` を直接依存に持ち、
**候補 11 パスが実際に解決することは §3-A で測定済み**である。
だが**この検出器は「自分が死んだこと」を CI に告げる手段を持たない**。

**しかも沈黙は二重である。** exit 0 のとき、この warning は**エージェントにも届かない**——
PostToolUse hook（`.claude/hooks/post-edit.mjs`）は **exit code で検出し、
成功した検査は何も出力しない**契約だからである（ルート `CLAUDE.md`「フック」
「**検出は exit code、出力は証拠**」）。ゆえに不正パスの警告を見る経路は
「緑の CI ログを全文読む」しか残らず、実際には誰も読まない。
`clippy.toml` が空洞化しても、**hook は沈黙し（＝合格と読まれ）、CI は緑になる**。
`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える変更は、その状態に
到達する全経路を列挙する」に照らすと、**沈黙経路が 2 本ある機構である**ことを
`clippy.toml` のコメントに明記し、フォールトインジェクションを全件で行う（§4.3）ことが
最低限の対処。塞ぎたければ CI で clippy の stderr を `does not refer to a reachable function`
で grep して落とす手があるが、費用対効果は要判断。

### R2. `ctx.options_mut(|o| o.dark_style = …)` は素通りする

§1.4-C。汎用アクセサゆえ禁止できない。

### R3. `#[allow(clippy::disallowed_methods)]` は完全に無効化する（実測）

`#[allow]` を付けた関数内の禁止呼び出しは `-D warnings` 下でも 1 件も報告されなかった。
clippy 自身の help 文言（`to override -D warnings add #[allow(clippy::disallowed_methods)]`）が
逃げ道を案内する。**規範ではなく機構だが、機構としては opt-out 可能**である。

### R4. 他 crate に薄いラッパーを置けば見えなくなる

`snotra-egui-runtime` に `pub fn set_style(ctx: &egui::Context, s: Style)` を足して
`src-tauri` から呼べば、clippy は**呼び出し点しか見ない**ので素通りする
（`clippy.toml` は package スコープなので runtime 側では発火しない）。
**これは `ADR-visuals-application-target` が却下した「runtime 側に style 設定フックを設ける」
案とちょうど同じ形**であり、ADR が却下理由に挙げた「同じ結果へ 2 経路」がここでは
「機構の抜け道」としても現れる。

### R5. UFCS では抜けられない（残余では**ない**・実測）

`String::len(&s)` の完全修飾呼び出しは**発火した**
（`error: use of a disallowed method` / `a\src\lib.rs:4:5`）。
`egui::Context::set_visuals(&ctx, v)` の形も塞がれる。念のため測ったが穴ではない。

### R6. 順序不変条件は依然として守られない

issue の立場どおり。§4.4-1 に独立の根拠。

### R7. ⚠ `src-tauri/clippy.toml` は将来のルート `clippy.toml` を**遮蔽する**

§3-A の測定 2 のとおり、clippy.toml はマージされない——crate は自分のディレクトリで
見つけた時点で探索を止める。ゆえに**後日リポジトリルートに workspace 共通の `clippy.toml` を
置いても、`src-tauri` だけがそれを受け取らない**。しかも黙って。
「最も守りたい crate だけが workspace 共通ルールから抜ける」という向きの事故になる。
`src-tauri/clippy.toml` のコメントに 1 行残すべき残余。

### R8. `clippy.toml` の消失・空洞化を鳴らす検査は存在しない

`governance:check` の `G-workspace-lints` は**自身のコメントで clippy カテゴリを射程外と
宣言している**（`scripts/governance-check.mjs:303-305`:
「見るのは `rustdoc` カテゴリだけである。`[workspace.lints.clippy]` 等が降格されてもこの検査は
鳴らない」）。`clippy.toml` を消す／`disallowed-methods` を空配列にする変更は、
**CI も hook も governance:check も緑のまま通す**。

### R9. ⚠ egui のバージョンに結合した禁止である

この禁止が根拠にするのは egui 0.35.0 の `run_ui` の順序である。上流が直せば禁止は
**過剰な制約として残る**（緑のまま古びる）。
`ADR-visuals-application-target` は「`ctx.set_visuals` が届かないことを固定する対のテストを
置く」を **「上流が直した日に緑のビルドが赤になる」** という理由で却下した。
**本変更は同じ命題を別の道具で固定する行為であり、ADR の却下理由と同じ土俵にある。**
違いは**壊れる向き**である——テストは上流修正で**赤くなる**（有害）が、clippy の禁止は
上流修正後も**緑のまま余分に縛る**（無害だが陳腐化する）。
ゆえに却下理由はそのままでは当たらないが、**「ADR が却下した命題を別形で採用している」
ことを PR 本文か `clippy.toml` のコメントで自覚的に書く**べきである。書かないと、
後日 ADR を読んだ人が矛盾と読む。

### R10. `Cargo.toml` の `[lints]` へ移す道は無い

`disallowed-methods` は lint の**設定値**であり lint レベルではないので、
`[lints.clippy]` テーブルには書けない。`clippy.toml`（または `CLIPPY_CONF_DIR`）が唯一の口。
`clippy.toml` という**新種のガバナンス生成物**が 1 つ増えることは受け入れざるを得ない。

---

## 6. 所見の 3 分類

### 要対処（4 件）

| # | 所見 | 根拠 |
|---|---|---|
| 要-1 | **禁止集合は「global style を書くための API」ではなく「内部で書き込み口を呼ぶものを含む」規則で選ぶ。** 名前から選ぶと `style_ui` / `set_debug_on_hover` / `settings_ui` の **3 件を取りこぼす**（clippy は呼び出し点の def_id しか見ず callee はリントされない）。7 件だけを禁じて「global style を書く Context メソッドを全部塞いだ」と書くと全称主張が偽になる | §1.1 の規則・§1.3 の 8〜10・`context.rs:3072-3074` / `:3187-3200` / `memory/mod.rs:427-432`・`AGENTS.md`「全称表現は前提条件とセットで書く」 |
| 要-2 | **`EguiView::setup`（`view.rs:373` / `results_view.rs:513`）という欠陥を持たない書き込み地点が src-tauri 内に構造として在る**ことを認識したうえで、塞ぐ判断を明示的に下し、`reason` を条件つき文言にする | §3-B・`runtime.rs:380-385`（`run_ui` の外で `view.setup(&context)`） |
| 要-3 | **不正パスは CI を赤にせず、型名・crate 名の書き損じは完全に沈黙する。しかも hook は exit code で検出するため沈黙が二重になる**。フォールトインジェクションは**全件**で行い、沈黙経路を `clippy.toml` のコメントに明記する | §5-R1（実測: exit 0 / 診断ゼロ / hook 契約）・`.claude/rules/safety-nets.md`「全経路を列挙する」 |
| 要-4 | **`src-tauri/CLAUDE.md` の「`ctx.set_visuals` を使ってはならない」を、メソッド名ではなく類で書き直す。** 機構が 10 件を拒むのに文書が 1 件しか名指ししないと、文書は機構より狭い | §4.2 |

### 軽微（4 件）

| # | 所見 |
|---|---|
| 軽-1 | `view.rs:16-17` の「全域 grep で 0 件」に、根拠が観測から機構へ移った旨を 1 語足す（§4.2） |
| 軽-2 | `clippy.toml` のコメントに「含めなかったもの」（`options_mut` / `set_theme` / `set_fonts`）と R7（ルート clippy.toml の遮蔽）を残す（§4.1・§5-R7） |
| 軽-3 | R9（ADR が却下した命題を別形で採用していること）を PR 本文かコメントで自覚的に書く（§5-R9） |
| 軽-4 | `--all-targets` ゆえテストも縛られる。将来「pass の前に global style を積む対照テスト」を書けなくなる（§3-C） |

### 未検証（3 件）

| # | 所見 | 何を測れば決着するか |
|---|---|---|
| 未-1 | **実 repo での「赤くなること」だけが未測定**（パス解決と違反 0 件は §3-A で測定済み）。違反の注入は `.rs` の変更を要するため本レビューでは行っていない | `src-tauri/clippy.toml` を置き `view.rs` の `let visuals = ui.visuals_mut();` の直前へ**採用した全件**を注入して `cargo clippy --workspace --all-targets -- -D warnings` を回す。**全件が個別に error を出すこと**を行番号で確認する（1 件だけの確認では要-3 の沈黙経路を見逃す）。なお `set_debug_on_hover` は `#[cfg(debug_assertions)]` ゆえ注入コードも dev でしか通らない |
| 未-2 | ⚠ **`set_theme` を含めるべきかの最終判断**（§1.4-B）。「style の中身ではない」ゆえ除外を推したが、**症状は同型**である。判断が割れる | 判断であって測定ではない。ただし「callback 内で `set_theme` を呼ぶと当該 pass で theme が切り替わらない」ことは、`view.rs:1378` と同型のテスト（1 pass だけ走らせて `ui.visuals()` を読む）で測れる |
| 未-3 | ⚠ **`snotra-egui-runtime` に同じ `clippy.toml` を置くべきか**を本レビューは判断していない。同 crate は `run_ui` の**呼び出し側**であり、`run_ui` より前の global style 書き込みは**正当**である（`EguiWindow::new` がまさにその位置）。ゆえに単純な複製は誤り | 判断であって測定ではない。少なくとも「src-tauri と同じ禁止集合をそのまま複製してはならない」ことは `runtime.rs:384-385` から言える |

---

## 付録: 実施した測定の一覧（すべてリポジトリ無改変）

| 測定 | 場所 | 結果 |
|---|---|---|
| `impl Context` の所在 | `egui-0.35.0/src/` 全域 grep | 18 件すべて `context.rs`（母集団が閉じる） |
| style writer の選別（直接） | `context.rs:2080-2230` を読解 | 7 件（§1.3 の 1〜7） |
| style writer の選別（間接） | `context.rs:3562-3569` / `:3064-3075` / `:3185-3200` + `memory/mod.rs:373-432` を読解 | 3 件（`style_ui` / `set_debug_on_hover` / `settings_ui`） |
| 除外側の確認 | `context.rs:2038-2080`（`set_fonts` / `add_font`）・`:1069-1073`（`options_mut`）・`:2102-2104`（`set_theme`）・`:3223-3560` を grep | `inspection_ui` / `texture_ui` / `loaders_ui` / `memory_ui` に global style 書き込み無し（`ui.style_mut()` のみ＝ローカル） |
| root `Ui` の snapshot | `context.rs:780-807` / `ui.rs:100-159` | `Ui::new`（:788）が callback（:798）より前・`ui.rs:135` で `ctx.global_style()` |
| repo 内使用 | crate ごとに `grep -rn --include=*.rs` | src-tauri 実呼び出し 0 / settings 2 / core 0（egui 非依存）/ runtime 0 |
| `EguiView::setup` の位置 | `runtime.rs:380-386` | `run_ui` の外 |
| settings の呼び出し位置 | `app.rs:665-678` | `run_native` の creation closure 内＝ pass の外 |
| clippy.toml の package スコープ | scratchpad の 2-member workspace | `a` のみ発火・`b` は非発火（`--workspace` 実行でも） |
| clippy.toml のマージ有無 | 同上（root と `a` に別内容） | **マージしない**。`a` はルートの設定を受け取らない |
| 不正パスの扱い | 同上 | メソッド／モジュール名 typo → warning だが **exit 0**。crate 名 typo・未依存 crate → **無診断** |
| UFCS | 同上 | **発火する**（抜け道ではない） |
| `#[allow]` | 同上 | **完全に抑止する** |
| `CLIPPY_CONF_DIR` / `.cargo/config.toml` | repo 全域 grep / find | どちらも不在（探索基準の上書き無し） |
| **実 repo でのパス解決**（陽性対照つき） | `CLIPPY_CONF_DIR=<scratchpad> cargo clippy -p snotra --all-targets`・リポジトリ無改変 | 候補 11 件は**全件解決**。陽性対照 2 件（`set_visualz` / `Contextt`）だけが警告 |
| **src-tauri の違反件数**（型解決経由） | 同上 | `use of a disallowed method` **0 件**（grep より強い否定） |
| CI / hook の clippy 起動形 | `ci.yml:126` / `post-edit.mjs:308-312` | 双方 `--workspace --all-targets -- -D warnings` |
| `G-module-index` の母集団 | `governance-check.mjs:117,129` | `.rs/.ts/.tsx/.html` × `cfg.src` 配下 → `clippy.toml` は対象外 |
| clippy 版 | `cargo clippy --version` | 0.1.94 (4a4ef493e3 2026-03-02) |
