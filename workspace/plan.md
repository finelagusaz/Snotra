# plan: #654 — 入力欄 text_color の config 化 + updater 失敗詳細の表示

前提は `workspace/research.md`。要旨: **2 項目とも実装する**（2026-07-26 ユーザー判断）。issue の「WebView2 parity」という当初の根拠は SU7 完了で失効しており、根拠は差し替わっている——項目 2 は「§11 規範『色は config `[visual]` から取る』の唯一の違反であり、**既定設定で既に見える不整合**」、項目 1 は「失敗理由が `SNOTRA_TRACE` にしか残らず、実測で幅は足りる」。

2 つの Phase は**互いに独立**（触るファイルは `view.rs` と `SPEC.md` で重なるが、行が離れている）。Phase 1 を先に置くのは、1 行で既定設定のバグが直り、視覚スモークの A/B が単純だから。

---

## Phase 1 — 検索入力欄の 2 色（入力テキスト + hint）を config から取る（項目 2 + Step 2b ★）

**当初は項目 2（入力テキスト）だけの Phase だったが、Step 2b の独立再導出が同一ウィジェットに 2 件目の欠陥を見つけたため拡張した**（2026-07-26 ユーザー判断で本 PR に含める）。どちらも「config テーマが消費されていない」という同一の欠陥クラスで、`SPEC.md:571` は**両方について偽**である。

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/egui_shell/view.rs` | `TextEdit::singleline` のビルダ鎖（1464 行付近）に `.text_color(bar_theme.name_color)` を追加。`.font()` と `.hint_text()` の間に置く |
| 同 | `set_visuals` ブロック（1210-1215 行）へ `visuals.weak_text_color = Some(visual.hint);` を追加（**hint の実効経路**） |
| 同 | `hint_text` の `.color(bar_theme.path_color)`（1472 行）を**削除**する——egui 0.35 が無条件上書きするため dead であり、残すと「効いている」と読める |
| 同 | コメントブロック（1446-1449 行の「§11 Part C（#643）: …」）を更新し、2 色それぞれが**なぜその経路なのか**（入力テキストはウィジェット単位の明示指定、hint は `weak_text_color` 経由でしか届かない）を記す |
| `SPEC.md` §11 as-built（571 行） | 入力テキスト = `text_color`、hint = `hint_text_color`（**実際に効くようになった**）を書く |
| `SPEC.md` §11 見た目の規範（559 行） | 「色の指定を省いて UI ライブラリの既定色に委ねない」を追記（**規範の変更**・下記セルフレビュー「規範の穴を塞ぐ」が根拠） |

### 実装

**(a) 入力テキスト本体** — ウィジェット単位の明示指定が最優先で効く（`egui-0.35.0/src/widgets/text_edit/builder.rs:463-466` で確認）:

```rust
    .interactive(input_editable)
    .font(bar_font.clone())
    .text_color(bar_theme.name_color)   // ← 追加
    .hint_text(egui::RichText::new(hint).font(bar_font)),  // ← .color() を削除
```

**(b) hint** — `set_visuals` 経由でしか届かない:

```rust
    visuals.extreme_bg_color = visual.input_bg; // TextEdit 背景
    visuals.selection.bg_fill = visual.selection;
    visuals.weak_text_color = Some(visual.hint); // ← 追加: TextEdit の hint はここだけが効く
    ctx.set_visuals(visuals);
```

**なぜ (a) と (b) で経路が違うのか**（コメントに残す）: `builder.rs:589-591` が hint を `hint_text.map_texts(|t| t.color(ui.style().visuals.weak_text_color()))` で**無条件に上書き**する。egui 自身のコメントが "Sucks, since it means users won't be able to override it" と述べており、`RichText::color()` は届かない。入力テキスト本体は逆に `self.text_color` が最優先で、`override_text_color` より前に見られる。**同じウィジェットの 2 つの文字列が、別の機構で色を受け取る。**

`bar_theme` は既に `let bar_theme = &visual.row;`（1450 行）で、`visual.hint` は同じ `visual` から取れるため、**新たな lock も新たな読みも増やさない**（`src-tauri/CLAUDE.md`「テーマ色・font・行高の読みは 1 フレーム 1 回」を満たす）。

### 不変条件

- **入力テキストと hint は別の色であり続ける**: `name_color`（config `text_color`）と `hint`（config `hint_text_color`）。#643 が意図して分けた 2 色で、同色にしてはならない
- **`weak_text_color` の差し替えが他の描画へ及ばない**: main 窓の Context で weak text を使う描画は TextEdit の hint だけである。status 行（`view.rs:1563` の `visual.hint`）・toast 本文（`theme.name_color`）・`draw_toast_button`（`theme.name_color` / `path_color`）はいずれも色を明示指定しており、`Visuals` の既定色経路を通らない。**results 窓は別 Context** ゆえ影響を受けない
- **入力テキストに `visuals.override_text_color` を使う代替を採らない**: `override_text_color` は `Visuals::text_color()` の起点であり、`weak_text_color()` の導出元（`text_color().gamma_multiply(weak_text_alpha)`）でもある。ここを差し替えると **hint の色が入力テキスト色から派生してしまい**、(b) で明示指定する意図と衝突する。入力テキストは**ウィジェット単位の `.text_color()` に限定する**
- **失敗しない変更である**: `text_color` は `Color32` を取る純粋なビルダ設定で、異常系を持たない。config の hex が不正なときの fallback は `visual.rs` の `hex_or` が既に処理済み（不正値 → 既定色。`visual.rs` のテストが固定）

### テスト方針

- **ユニットテストは書けない**（imperative shell の配線であり、`RowTheme` の導出テストは「config → 色」を証明するだけで「TextEdit がそれを消費する」ことは証明しない）
- **検知手段は視覚スモーク（`docs/build-commands.md` カテゴリ D）に限られる**。これは受容する残余であり、plan に明記して隠さない
  1. `%APPDATA%\Snotra\config.toml` の `[visual]` に `text_color = "#FF00FF"`（マゼンタ）と `hint_text_color = "#00FF00"`（緑）を置く。**2 色を別々の目立つ色にする**——同じ色にすると、どちらの経路が効いたのか区別できない
  2. `cargo run -p snotra` → ホットキーで表示
  3. **未入力時の hint「検索...」が緑**であることを確認する（(b) の検証）
  4. 文字を打ち、**入力した文字がマゼンタ**であることを確認する（(a) の検証）
  5. **副作用の確認**: status 行・トーストのボタン・結果行の色が変わっていないこと（`weak_text_color` の差し替えが他へ及んでいないこと）
  6. A/B: 変更前の `main` でも同じ config で走らせ、**入力文字が灰（`#B4B4B4`）・hint が暗い灰（`#545454`）のまま**であることを確認する（`feedback_ai_review_practices`: GUI 挙動の由来切り分けは A/B が最速）
- **PostToolUse hook の沈黙はこの検証を含まない**（`.claude/rules/src-tauri.md`: カテゴリ A は C/D を含まない）。「hook が黙ったから検証済み」と報告しない

### SPEC.md 更新

§11 as-built 571 行を次の形へ:

> 検索入力欄は `font_size` に追従し、**入力テキストは `text_color`・hint は `hint_text_color`** で描かれる。（以下現行のまま）

**この行は変更前の時点で 2 つの意味で偽だった**——入力テキストは config を見ておらず（項目 2）、hint も egui の `weak_text_color` へ落ちていた（Step 2b ★）。「hint は `hint_text_color` で描かれる」という記述自体は #643 の意図どおりだが、egui 0.35 の TextEdit atoms 化で失効していた。

規範側（559 行）は**当初「変更しない・実装が追いついていなかっただけ」と判断したが、plan-review 指摘 D を受けて撤回した**——現行の文言は「色リテラルを書かない」しか禁じておらず、**今回のバグの様態（指定を省いてライブラリ既定へ委ねる）を捕まえない**。詳細と追記文は下記セルフレビュー「規範の穴を塞ぐ」。

---

## Phase 2 — updater 失敗トーストに失敗理由を併記する（項目 1）

### 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/egui_shell/strings.rs` | `update_failed(l: Language) -> &'static str` を `update_failed(l: Language, reason: &str) -> String` へ。**セパレータは関数側で作る**（`launch_failed` の `detail` とは契約が違うため引数名を変える・plan-review 指摘 B） |
| `src-tauri/src/egui_shell/notify.rs` | `UpdaterPhase::InstallFailed` → `InstallFailed { message: String }`。`ToastKind::Failed` → `Failed { message: String }`。`toast()` の対応 arm で clone |
| 同 | `InstallFailed` の doc コメントを書き換える（#648(C) の「dead payload ゆえ削除」という経緯注記が現状と食い違うため。**削除ではなく更新**——なぜ一度落として戻したかを 1 行残す）。**`UpdateToast.tsx` / `MainApp.tsx` への parity 参照は不在ファイルを指すので消す** |
| `src-tauri/src/egui_shell/mod.rs` | `spawn_update_check` に `SNOTRA_EGUI_FAKE_UPDATE_FAILED` の視覚スモーク hatch を追加（**Phase 2 の描画を観測する唯一の手段**・Step 2b 発見 4） |
| `src-tauri/src/egui_shell/view.rs` | `spawn_install` の `Err(e)` 枝で `let detail = e.to_string();` を束ね、trace と `InstallFailed { message: detail }` の両方へ渡す（**`e` は trace で消費されるのでコンパイラが強制する**） |
| 同 | toast 描画の `ToastKind::Failed { message }` arm で `update_failed(l, message)` を呼ぶ。**`..` で受けない**（Step 2b 発見 3） |
| 同 | メッセージ描画をクリップから**末尾省略**へ（`TextWrapping::truncate_at_width` + `break_on_newline = false`） |
| `SPEC.md` §20.3 | updater トーストが失敗理由を併記し、幅超過時は末尾省略することを 1 行追記（**§11 ではない**・plan-review 指摘 A） |

### 実装順序（依存関係）

`strings.rs` → `notify.rs` → `view.rs` の順。**新 API の導入と呼び出し点の移行は 1 コミットに束ねる**（`AGENTS.md`「条件別チェック」: `-D warnings` 下で未使用の新 API は `dead_code` で落ち、旧 API を残せば導出が 2 箇所になる）。`update_failed` は**シグネチャを変える**ので、下流の compile-fail がそのまま移行漏れ検出器になる。

### 1. `strings.rs`

```rust
/// `reason` は**整形前の生の失敗理由**（`tauri_plugin_updater` のエラー文字列）。
/// 空なら generic 文言へ戻る。
///
/// **`launch_failed` / `launch_timeout` の `detail`（呼び出し側が `" (msg)"` まで
/// 整形済み）とは契約が違うため、引数名を変えてある。** セパレータをこちら側に
/// 置くのは、「理由が空のときコロンだけ残る」という失敗様態を
/// `strings.rs` のユニットテストで固定できるようにするため——呼び出し側で
/// 整形すると view.rs のインライン処理になり、検知手段が視覚スモークだけになる
/// （plan-review 指摘 B）。区切り文字は元から 2 関数で違う（`" (msg)"` vs `": msg"`）
/// ので、契約を揃える利得は元々無い。
pub fn update_failed(l: Language, reason: &str) -> String {
    let base = match l {
        Language::Ja => "更新に失敗しました",
        Language::En => "Update failed",
    };
    if reason.is_empty() { base.to_string() } else { format!("{base}: {reason}") }
}
```

**既存の文言は 1 文字も変えない**（`strings.rs` の `//!`: 文言は計画書の写しではなく実物のソースを見る。上の 2 文字列は現行 103-108 行から逐語で写した）。

### 2. `notify.rs`

```rust
    // 失敗理由を toast に併記するため message を持つ（#654）。#648(C) で一度
    // dead payload として落としたが、描画側が消費するようになったので戻した
    // ——「描かないなら型に持たない」という当時の判断は今も正しい。
    InstallFailed { message: String },
```

```rust
pub enum ToastKind {
    Available { version: String },
    Installing,
    Failed { message: String },
}
```

```rust
            UpdaterPhase::InstallFailed { message } => Some(ToastRow {
                kind: ToastKind::Failed { message: message.clone() },
                show_install: false,
                buttons_enabled: true,
            }),
```

### 3. `view.rs` — `spawn_install`

```rust
                    if let Some(st) = handle.try_state::<crate::egui_shell::UpdaterUiState>() {
                        st.0.lock().unwrap().phase =
                            crate::egui_shell::UpdaterPhase::InstallFailed { message: e.to_string() };
                    }
```

`e.to_string()` は**既に 1 行上の trace で作っている**ので、`let detail = e.to_string();` へ括り出して両方で使う（`/dry-check`: 同じ式を 2 回書かない）。

### 4. `view.rs` — toast 文言

```rust
                crate::egui_shell::ToastKind::Failed { message } => {
                    // 整形（空理由でコロンだけ残さない）は update_failed 側の責務。
                    // ここは生の理由を渡すだけにする（plan-review 指摘 B）。
                    crate::egui_shell::ui_strings::update_failed(l, message)
                }
```

### 5. `view.rs` — 末尾省略へ

現行（クリップ・省略記号なし）:

```rust
            let text_clip = egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2((cursor_x + 8.0).max(rect.left()), rect.bottom()),
            );
            ui.painter().with_clip_rect(text_clip).text(...);
```

置換後（`results_view.rs:264-273` と同じ形）:

```rust
            // メッセージはボタン群の左端で**末尾省略**する（衝突回避）。`cursor_x` は
            // 最後のボタンぶん進んだ位置ゆえ、間隔の 8.0 を戻して境界にする。
            // クリップではなく省略にするのは、失敗理由（#654）が幅を超えたときに
            // 「切れている」ことが読者に伝わるようにするため——クリップは文字の
            // 途中でぶつ切りにし、続きがあることを示さない。
            let text_x = rect.left() + 8.0;
            let avail = ((cursor_x + 8.0) - text_x).max(0.0);
            let mut job = egui::text::LayoutJob::single_section(
                line1,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(theme.status_size),
                    color: theme.name_color,
                    ..Default::default()
                },
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(avail);
            // `single_section` の既定は `break_on_newline: true`（epaint 0.35
            // `text_layout_types.rs:177`）だが、現行の `painter().text()` が使う
            // `simple_singleline` は false（同 162）。`max_rows: 1` と組むと、改行入りの
            // 失敗理由が**幅と無関係に**そこで切れて `…` になる。挙動を変えないために戻す。
            job.break_on_newline = false;
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(
                egui::pos2(text_x, rect.center().y - galley.size().y / 2.0),
                galley,
                theme.name_color,
            );
```

`line1` は `String` なので `single_section` へ move できる（`clone` 不要）。`Align2::LEFT_CENTER` の等価は `y - size.y / 2.0`（`results_view.rs` の同パターンと一致）。

**この置換は 3 variant 共通の描画点である**（`view.rs:1623-1629` の 1 箇所）。ゆえに `Available` / `Installing` の溢れ表現も hard clip → `…` に変わる。**副作用ではなく意図した挙動変更**として PR 本文と SPEC に書く（「Failed だけ省略」は分岐を足さないと書けず、その分岐に価値が無い）。

### 6. `mod.rs` — 失敗局面の視覚スモーク hatch

**これが無いと Phase 2 の描画は一度も観測できない**（実 install 失敗の再現には実 release への到達 + download 失敗が要る）。既存の `SNOTRA_EGUI_FAKE_UPDATE`（`mod.rs:126-137`）と同じ形で 2 本目を足す（`trace::env_flag` は bool 専用ゆえ、既存フラグの値で分岐できない）:

```rust
    // 視覚スモーク専用: 失敗トーストの描画（理由の併記 + 末尾省略）を観測する。
    // 実 install 失敗は実 release への到達 + download 失敗が要り再現できないため、
    // **これが Phase 2 の唯一の観測点である**（#654）。理由は既定幅で省略が起きる
    // 長さにしてある——短い理由では `…` が出ず、省略経路を目視できない。
    if crate::trace::env_flag("SNOTRA_EGUI_FAKE_UPDATE_FAILED") {
        if let Some(st) = app.try_state::<UpdaterUiState>() {
            st.0.lock().unwrap().phase = crate::egui_shell::UpdaterPhase::InstallFailed {
                message: "Network Error: error sending request for url \
                          (https://example.invalid/releases/latest.json)"
                    .into(),
            };
        }
        return;
    }
```

既存 hatch の直前に置く（どちらも `return` するので順序が意味を持つ——**失敗の注入を先に見る**）。`docs/build-commands.md` の視覚スモーク節へ 2 本とも記載する（**既存 hatch は `docs/` にも `scripts/` にも記載が無い**——Step 2b が実測。ここで documented にする）。

### 不変条件

- **`Failed` 以外の 2 局面の見た目を変えない**: `Available` / `Installing` も同じ描画コードを通るため、末尾省略化はこの 2 つにも及ぶ。両者は短い定型文言（実測 117px 以下）で 532px の可用幅を超えないので、**表示は変わらない**。境界条件として「窓幅を極端に狭めると Available も省略される」——これは以前は無言のぶつ切りだったものが `…` になるだけで、退行ではない
- **`show_install` は `false` のまま**: 失敗時に `[今すぐ更新]` を出さない現行挙動を変えない（幅の前提でもある——install ボタンが出ると可用幅が約 100px 減る）
- **`dismiss()` の挙動を変えない**: `InstallFailed` は dismiss 可能（`notify.rs:162-168` の `Installing` のみ拒否）。message 追加はこの分岐に触れない
- **並行境界を増やさない**: `spawn_install` の `Err` 枝は従来どおり「lock 取得 → `phase` 代入 → lock 解放 → `wake_main`」の 1 往復。String の move が 1 つ増えるだけで、lock の保持区間も `.await` との位置関係も変わらない（`/race-check` 対象だが、**送信から適用までの窓は新設されない**）
- **hidden 中に失敗しても失われない**: `phase` は状態として残り、次の show でフレームが回ったときに描かれる（`wake_main` は可視中の即描画のため。`InstallFailed` は時限ではないので reset-on-show の backstop 対象外——現行と同じ）
- **異常な message でも壊れない**: 極端に長い文字列は `truncate_at_width` が 1 行へ畳む（`max_rows: 1`）。改行を含む文字列は `single_section` + `max_rows: 1` で 1 行に収まる（`LayoutJob` の `break_on_newline` は既定 false）

### テスト方針

| 追加/更新するテスト | 固定する不変条件 | 場所 |
|---|---|---|
| `update_failed_appends_reason_in_both_languages`（新規） | 理由併記の書式が両言語で正しい。**空理由でコロンだけが残らない**（plan-review 指摘 B を検知可能にした当のテスト） | `strings.rs` tests |
| `params_are_interpolated_in_both_languages`（既存・更新） | 既存の launch/available の主張は保つ。`update_failed` の行を足すのではなく**上の新規テストで独立に書く**（既存テストが証明していた命題を薄めない・`AGENTS.md` Step 4） | `strings.rs` tests |
| `toast_projection_carries_failure_message`（新規） | `InstallFailed { message }` → `ToastKind::Failed { message }` が値を運ぶ（#648(C) で消えた経路の復活を固定する） | `notify.rs` tests |
| `dismiss_is_refused_while_installing`（既存・更新） | 266 行の `u.phase = UpdaterPhase::InstallFailed;` が compile-fail になるので `InstallFailed { message: "e".into() }` へ。**証明している命題（Installing は dismiss 拒否・InstallFailed は許可）は不変** | `notify.rs` tests |

検証コマンド: `cargo test -p snotra`（PostToolUse hook が自動実行・カテゴリ A）。**末尾省略の見た目はユニットテストで固定できない**ため、下の視覚スモークが唯一の検知手段。

### 破壊不変条件 + 検知手段

| 壊れたら即アウト | 検知手段 |
|---|---|
| updater トーストが**描かれなくなる**（layout_job 化のミス） | `SNOTRA_EGUI_FAKE_UPDATE` による fake 注入で `Available` トーストを出す視覚スモーク（既存の仕組み） |
| 失敗トーストの詳細が**描かれない / 省略記号が出ない** | `SNOTRA_EGUI_FAKE_UPDATE_FAILED`（本 PR で新設）による fake 注入の視覚スモーク。**これが無ければ観測手段はゼロだった** |
| 失敗トーストにコロンだけが残る（理由が空） | `strings.rs::update_failed_appends_reason_in_both_languages`。**セパレータ生成を `strings.rs` へ寄せたのは、この失敗様態をユニットテストで捕まえるため**（呼び出し側整形だと view.rs のインライン処理になり検知手段が視覚スモークだけになる・plan-review 指摘 B） |
| 失敗理由が toast へ運ばれない（payload の断線） | `notify.rs::toast_projection_carries_failure_message` |
| 実際の install 失敗時に詳細が出ない（`spawn_install` の配線ミス。fake hatch は `spawn_update_check` 経由で `spawn_install` を通らない） | **受容残余**——実 install 失敗の再現手段が無い。ただし `SNOTRA_TRACE` の `egui_update_install_failed` は従来どおり残るため、**詳細を失う経路は無い**（表示に出ないだけで trace には残る） |
| `Available` / `Installing` の見た目が変わる | 視覚スモーク（fake 注入）で 2 局面を目視 |

### SPEC.md 更新

**追記先は §20.3「トースト UI」であって §11 ではない**（plan-review 指摘 A で確定した。保留にしていた判断をここで解く）。理由: §11 は文字サイズ・色・面の**規範**を置く場所で、§20.3 が「トーストが何を表示するか」の正本である。両方に書くと正本が 2 箇所へ分散する。

§20.3 の egui 経路の項（1075-1079 行）へ追記:

> 失敗時は理由を併記する（`更新に失敗しました: {理由}`）。理由が幅を超えるときは末尾省略（`…`）し、`SNOTRA_TRACE` の `egui_update_install_failed` には全文が残る（#654）

§11 側は Phase 1 の入力欄 `text_color` の追記だけに留める。

**スコープ外として触らないもの**: §20.3 の 1071-1074 行（`行1: … y = 高さ × 0.25` / `行2: … y = 高さ × 0.75` / `--update-toast-height` CSS 変数 / `updateInfo` シグナル）は、#700 の 1 行化と SU7 のフロント撤去で既に腐っている記述で、**#654 が新たに生む不整合ではない**。束 C（#674 + #698「WebView2/SolidJS 時代の残骸掃除」）の対象として残す。

---

## 変更後の検証（`AGENTS.md` Step 8・コマンド本体の SSOT は `docs/build-commands.md`）

| カテゴリ | 実行するもの | 備考 |
|---|---|---|
| A（Rust） | `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` | PostToolUse hook が `*.rs` 編集で自動実行する（**沈黙 = 合格**） |
| D（視覚スモーク） | `cargo run -p snotra` の目視 | **両 Phase の主要な検知手段**。hook の沈黙はこれを含まない |
| F（ガバナンス） | `npm run governance:check` | `SPEC.md`（`*.md`）を編集するため該当（plan-review 指摘 C）。**`*.md` 編集では PostToolUse hook は何も走らない**——沈黙は「合格」ではなく「何も走らなかった」 |

カテゴリ C（`smoke:startup` / `smoke:egui`）は非該当。ウィンドウ生成・表示順・ホットキー・スラッシュコマンド経路のいずれにも触れない（`.claude/rules/src-tauri.md`「検証カテゴリは拡張子でなく変更が触れるコードパスの意味で決める」）。ただし `src-tauri/**` の変更ゆえ CI の Smoke workflow は paths 一致で自動起動する。

---

## セルフレビュー

### `/race-check`（計画レビューモード・インライン実施）

**計画は spawn / channel / listener / 共有スロットを一つも新設しない。** 既存の level-triggered 共有スロット `UpdaterUiState.phase` の payload を変えるだけであり、境界は既存の 2 端。

| # | 境界 | 判定 |
|---|---|---|
| 1 | ① worker spawn + ⑦ `.await` + ④ managed state 書き — `view.rs::spawn_install` の `Err` 枝 | **安全** |
| 2 | ④ managed state 読み（4e live-read） — `view.rs::update` の toast 描画 | **安全** |

- **4a wake 義務 [OK]**: `wake_main(&handle)` は既存で、message 追加は送信バーストの形を変えない
- **4b staleness [OK]**: staleness 機構は**level-triggered 状態**（スキルの型表が名指しで「updater の phase」を例示する型）。`phase` の書き手は全 6 箇所（`mod.rs` の `spawn_update_check` に 3・`notify.rs::try_begin_install` に 3・`view.rs::spawn_install` に 1）。`spawn_update_check` は起動時 1 回で、その最後の書き込みが `Available` を置いてからでないと `try_begin_install` は `Installing` へ遷移できない（`Available` からのみ遷移）。**後着の check が失敗表示を巻き戻す経路は無い**
- **4c hide/show [OK・意図的に跨ぐ]**: `UpdaterUi` が view-local でなく managed state にあるのは reset-on-show に一掃させないため（`egui_shell/mod.rs` の `UpdaterUiState` 直前のコメントが理由を明記）。クリアされないことに理由が書かれている＝スキルの判定条件を満たす
- **4d 順序 [OK]**: `Err` 枝の順序（trace → lock → 代入 → 解放 → wake）は不変。`let detail = e.to_string();` の括り出しは **lock の前**に置く（lock 保持区間を伸ばさない）
- **4e live-read [軽微な懸念]**: `message` は上流 `tauri_plugin_updater` の生エラー文字列で**ロケール非依存**。言語を切り替えると前半だけ追従し詳細は英語のまま残る。翻訳手段が無いため**受容する**

### `/plan-review` の結果

サブエージェント 3 体（Explore ×2 + 独立再導出 Plan ×1）。

**要対処 → 全件反映済み**

- **A（scout-docs）**: 「§20.3 に触れるか実装時に確認」の保留を解いた。**追記先は §20.3**（トーストが何を表示するかの正本）、§11 は入力欄 `text_color` のみ。両方に書くと正本が分散する
- **B（scout-docs）**: 「コロンだけ残る」の検知手段がユニットテストだという主張は**成立していなかった**（整形が `view.rs` インラインのため）。**訂正ではなく設計を変えて解決**——セパレータ生成を `update_failed` 側へ寄せ、引数を `detail`（整形済み）から `reason`（生の理由）へ変えた。これで失敗様態が `strings.rs` のユニットテストで固定できる。`launch_failed` との契約差は引数名で示し、doc に理由を書く
- **C（scout-docs）**: `npm run governance:check`（カテゴリ F）を検証コマンドへ追加した

**軽微な懸念（対応方針）**

- **scout-rust**: `Available` / `Installing` が可用幅を超えないという主張は research.md の一時プローブ実測に依拠しており、scout は独立再現していない。**実測はオーケストレーター自身が行ったもの**（`AGENTS.md`「判定の中核は自分で測る」）で、サブエージェントの報告を一次証拠にしていない
- **scout-docs D**: §11 の規範文「色は config `[visual]` から取る。描画コードに色リテラルを書かない」は、今回のバグの様態（**指定を省いてライブラリ既定へ委ねる**）を名指していない。規範を忠実に守る読者でも同じバグを書けるため、**1 節を追記する**（下記「規範の穴を塞ぐ」）
- **scout-docs スコープ外付記 / オーケストレーター確認**: §20.3 の 1071-1074 行の腐り（2 行構成・CSS 変数・signal）は束 C へ送る

### 規範の穴を塞ぐ（§11 見た目の規範）

`SPEC.md:559` を次へ拡張する（**規範の変更なので report で明示する**）:

> - **色は config `[visual]` から取る。** 描画コードに色リテラルを書かないこと、および**色の指定を省いて UI ライブラリの既定色に委ねないこと**。egui の既定はコード上に現れない色リテラルであり、省略は「書かない」を満たしても規範を破る（#654）

**根拠**: 今回のバグは色リテラルを書いたのではなく指定を省いたもので、現行の文言では捕まらない。`.claude/rules/safety-nets.md`「規範のフォールトインジェクションとは回避しようとする読者である」「**忠実に従う読者が誤る経路は、手を抜く読者からは見えない**」に該当する実例が出た以上、穴は塞ぐ。

### 独立導出との差分（Step 2b）

`plan.md` / `research.md` を読ませない Plan エージェントに、issue とコードだけから変更集合を再導出させた。

#### 漏れ（導出 ∖ plan）— **すべてオーケストレーターが一次資料で裏取り済み**

1. **★ hint の色指定が egui 0.35 で死んでいる**（最も重い発見・スコープ判断が要る）

   `egui-0.35.0/src/widgets/text_edit/builder.rs:589-591`:
   ```rust
   // Since we can't set a fallback color per atom, we have to override it here.
   // Sucks, since it means users won't be able to override it.
   hint_text.map_texts(|t| t.color(ui.style().visuals.weak_text_color()));
   ```
   **無条件の上書き**である（egui 自身のコメントが「ユーザーは上書きできない」と述べる）。帰結:
   - `view.rs:1472` の `.color(bar_theme.path_color)` は **dead code**
   - 実際の hint 色 = `Visuals::weak_text_color()`（`style.rs:1135-1138`）= `text_color().gamma_multiply(weak_text_alpha)` = gray(140)（`style.rs:1679`）× 0.6（`style.rs:1498`）≒ **`#545454`**。config 既定 `hint_text_color = "#808080"` より暗い
   - **`SPEC.md:571`「hint は `hint_text_color` で描かれる」は既に偽**。Phase 1 が編集する当の行である
   - 修正は 1 行: `view.rs:1210-1215` の `set_visuals` ブロックへ `visuals.weak_text_color = Some(visual.hint);`（`Visuals::weak_text_color: Option<Color32>` は `style.rs:1023` の pub フィールド）
   - 影響範囲は main 窓の Context のみ。weak text を使う他の描画は存在しない（status 行・toast 本文・`draw_toast_button` はいずれも `theme` の色を明示指定）

   **なぜ私の同一パターン検索が捕らえられなかったか**: 「色を**渡していない**箇所」を探したが、これは「渡したが**無視されている**箇所」である。呼び出し側だけを見て callee が尊重するかを確かめなかった。**枠組みが違えば見えるものが違う**という Step 2b の前提そのものの実例。

2. **`LayoutJob` 化で改行の扱いが変わる**（挙動変化・裏取り済み）

   `LayoutJob::single_section` は `break_on_newline: true`（`epaint-0.35.0/src/text/text_layout_types.rs:177`）だが、現行の `painter().text()` が内部で使う `simple_singleline` は `false`（同 162）。`max_rows: 1` と組み合わさるため、**素直に置換すると改行入りエラーが幅と無関係にそこで切れる**。

   → **`job.break_on_newline = false;` の 1 行で現行挙動を保つ**。導出が提案した `sanitize_failure_detail` 純関数は不要（新関数を足さずに済む＝より単純）。

3. **`ToastKind::Failed { .. }` を `..` で受けない**（採用）

   `..` で受けると「payload を足したが描いていない」が `-D warnings` でも通り、#648(C) が dead payload を作った経路を再生産する。**フィールドを明示束縛する**ことで、消費し忘れが compile-fail になる。

4. **`SNOTRA_EGUI_FAKE_UPDATE_FAILED` の視覚スモーク hatch**（採用）

   これが無いと **Phase 2 の描画は一度も観測できない**（実 install 失敗の再現には実 release への到達 + download 失敗が要る）。既存の `SNOTRA_EGUI_FAKE_UPDATE`（`mod.rs:126-137`）と同じ形で、`trace::env_flag` は bool 専用ゆえ 2 本目のフラグを足す。**省略が起きる長さの理由を注入する**（短い理由では `…` が目視できない）。

   これはスコープ拡大ではなく**検知手段の確保**である（`AGENTS.md` 事前調査「破壊不変条件は検知手段とセットで」）。

5. **`e.to_string()` の二重消費**（既に plan にあり・裏取りで確認）

   `view.rs:1038` の trace が `e` を消費するため、`let detail = e.to_string();` の束ね出しは**コンパイルが強制する**（plan は DRY 上の選択として書いていたが、実際には必須である）。

#### スコープ過剰（plan ∖ 導出）

なし。導出は plan と同じ 2 項目に収束した。

#### 不一致（判断が割れた点と決着）

| 論点 | plan | 導出 | 決着 |
|---|---|---|---|
| 区切り文字の置き場所 | 当初は呼び出し側（後に `strings.rs` へ変更） | `strings.rs` 側 | **一致**（plan-review 指摘 B の反映後） |
| detail の長さ上限 | 設けない | 捕捉時に文字数上限 | **設けない**（YAGNI）。`toast()` は既に `Available` で `version.clone()` を毎フレーム行っており形は同じ。イベント駆動でトースト静止中はフレームが回らないため、毎フレーム確保という前提自体が成り立たない |
| 改行の扱い | 言及なし | sanitize 関数を新設 | **`break_on_newline = false` の 1 行**（新関数不要・現行挙動を保つ） |
| `LayoutJob` の 2 箇所重複 | 言及なし | 許容（別窓・別 Context） | **許容**。共通化すると幅・font・色だけの薄いラッパーになる（`/dry-check` の判断として明記） |

#### 一致（盲点が無いことの能動的証拠）

独立に再導出しても同じ結論に達した主要判断:

- 変更ファイル集合（`strings.rs` / `notify.rs` / `view.rs` / `SPEC.md`）と、各ファイルのシンボル
- `TextEdit::text_color` の優先順位（明示指定が最優先・`builder.rs:463-466`）
- `interactive(false)` は色の選択に関与しない → `/state-check` 案件ではない
- SPEC の追記先は **§20.3**（トーストの正本）で §11 へ写しを作らない（scout-docs 指摘 A と独立に一致）
- 既存テスト `dismiss_is_refused_while_installing`（`notify.rs:266`）が compile-fail による移行漏れ検出器になる
- カテゴリ C は非該当・カテゴリ D が唯一の接地した観測点
- `docs/superpowers/` の SU5・SU6.5 設計書は日付付きの決定記録ゆえ retro-edit しない
- `SPEC.md:1071-1072` の 2 行構成の記述は既存の腐り（束 C 送り）
- `ToastKind::Failed` と `LaunchStatus::Failed` は同名別概念・`snotra-settings` の TextEdit は対象外

#### 決着（2026-07-26 ユーザー判断）

**★ の hint 修正は本 PR に含める。** Phase 1 を「入力欄の 2 色を config から取る」へ拡張した。理由: 同一ウィジェット・同一欠陥クラスであり、Phase 1 が編集する `SPEC.md:571` は**両方について偽**だったため、片方だけ直すと偽の記述を隣に残すことになる。検証も同じ視覚スモークの A/B に相乗りする（2 色を別々の目立つ色にして区別する）。

### 5b の 3 観点

1. **境界条件**

| 境界 | 検証 |
|---|---|
| `message` が空文字 | `update_failed` がコロンを出さない。**`strings.rs` のユニットテストが固定する**（plan-review 指摘 B の反映後） |
| `message` が可用幅を超える | `truncate_at_width` が `…` で畳む。実測 617px vs 可用 532px の例が research にある |
| `message` に改行が含まれる | `break_on_newline = false` で現行挙動（1 行に流す）を保つ。放置すると幅と無関係にそこで切れる（Step 2b 発見 2） |
| `avail` が 0 以下（窓を極端に狭める） | `.max(0.0)` で負を潰す（`results_view.rs:263` と同じ防御） |
| config `text_color` / `hint_text_color` が不正 hex | `visual.rs` の `hex_or` が既定色へ fallback（既存テストが固定）。**hint も同じ経路**（`visual.hint` は同じ `hex_or` を通る） |
| 入力欄が空（hint 表示）↔ 非空（入力テキスト表示） | 2 色が別経路（`weak_text_color` / `.text_color()`）で届くため、切り替わりで色が入れ替わらないこと。視覚スモーク手順 3・4 が両方を見る |
| 言語が En（ボタン幅が 3.8px 広い） | 省略位置が数 px 動くだけ。追加対処なし（research の判断） |
| `Available` / `Installing`（省略化の巻き添え） | 実測 117px 以下で可用幅内。表示は変わらない（不変条件節） |

2. **シンプル化の挑戦**

- **新しい状態を導入していない**: `AtomicBool` も `Mutex` も子プロセスも増えない。`UpdaterPhase` に String フィールドが 1 つ戻るだけで、状態機械の**局面数も遷移も不変**
- **より単純な代替を検討した結果**:
  - 項目 2 で `visuals.override_text_color` を使う案 → **却下**（副作用が対象外の描画に及ぶ。不変条件節に理由を記載）
  - 項目 1 で「詳細をトーストに出さず trace のみ」→ ユーザー判断で却下（受容案は選ばれなかった）
  - 項目 1 で「2 行トーストにして詳細を下段へ」→ **却下**。#700 が 2 行構成をやめた直後であり、行高（= `bar_height`）が 2 行を収められないことは実測済み。1 行 + 末尾省略が最小
- **「この操作が失敗したらどうなるか」**: 両 Phase とも失敗しうる操作を導入しない（純粋なビルダ設定とレイアウト計算のみ）

3. **破壊不変条件 + 検知手段** — Phase ごとの節に記載済み。**両 Phase とも主要な検知手段は視覚スモーク（カテゴリ D）であり、ユニットテストと PostToolUse hook では捕まらない**ことを明記した。これは隠された前提ではなく、記録された残余である
