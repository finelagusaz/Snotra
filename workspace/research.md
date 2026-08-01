# research — #870 フォルダ展開中の現在地を中間省略する

## issue の要約

フォルダ展開中の入力欄プレースホルダ（`SPEC.md`「6.7 フォルダ展開中の現在地表示」）が、深い階層で**異なるディレクトリを区別しない**。egui の `hint_text` が末尾を `…` にするため、パスの末尾（＝いま居るフォルダ名）から削れる。

2026-08-01 の #836 カテゴリ D 実測（英語 UI・既定幅 600px・128 字の fixture）で、第 3 階層と LEAF 到達の main 窓キャプチャが **SHA256 で 1 バイトも違わなかった**。どちらも `Search in C:\tmp\snotra836\snotra836-entry\very-long-directory-name-for-truncation…` で止まる。

この機能は #743 の「`←` が階層を上げていない」という**誤読**を防ぐために入った。表示が現在地を区別しないなら、目的が深い階層で成立していない。

`docs/adr/ADR-folder-location-display-surface.md` の却下 3 が「カテゴリ D の目視項目 7・10 で受容できないと判明したらこの案を follow-up として起こす」と観測点を残しており、その観測点が答えを返した。**2026-08-01 にリポジトリ所有者が採用を裁定した。**

## 関連ファイル・モジュール・関数（すべて grep で実在確認済み）

| 位置 | 役割 |
|---|---|
| `src-tauri/src/egui_shell/view.rs:398-421` | hint の 3 分岐（tool > folder > results）。**closure の外** |
| `src-tauri/src/egui_shell/view.rs:468-495` | `Frame::inner_margin(inset).show(ui, \|ui\| { ... add_sized(vec2(ui.available_width(), field_height), TextEdit::singleline(..).hint_text(RichText::new(hint).font(bar_font))) })` |
| `src-tauri/src/egui_shell/strings.rs:59-64` | `folder_hint(l, dir)`。doc コメントが「省略は呼び出し側で組まない」と書いており **#870 で偽になる** |
| `src-tauri/src/egui_shell/strings.rs:250-256` | `folder_hint_uses_ascii_ellipsis_not_u2026` — 書式の三点は **ASCII `...`**、U+2026 を含まないことを測る |
| `src-tauri/src/egui_shell/results_view.rs:440-454` | `truncate_middle(s, avail_px, per_char_px) -> String`（`pub(crate)`）。head/tail を等分し `…`（U+2026）を挟む |
| `src-tauri/src/egui_shell/results_view.rs:382-392` | 唯一の呼び出し点。**実 galley から `per_char_px` を実測している** |
| `src-tauri/src/egui_shell/results_view.rs:598,667-679` | `truncate_middle` のユニットテスト 2 本 |
| `src-tauri/src/egui_shell/layout.rs` | 純粋核（egui/Win32 非依存・ユニットテスト対象）。`path_size` / `status_size` / `Metrics` / `present_results` / `Debouncer` 等 |
| `SPEC.md:267-272`（§6.7） | 最終行「**アプリ側に省略の機構は持たない**」が偽になる |
| `docs/adr/ADR-folder-location-display-surface.md` | 却下 3 と「受容した残余」の 2 か所を書き換える |

## 却下 3 の理由が既に解かれていること（一次証拠）

### 理由 1「`per_char_px` は呼び出し側が渡す推定値」→ 既存の呼び出し点は推定していない

`results_view.rs:382-392`（実物）:

```rust
let path_full = ui.painter().layout_no_wrap(result.path.clone(), path_font.clone(), theme.path_color);
let path_str = if path_full.size().x <= avail {
    result.path.clone()
} else {
    // per-char 幅は実 galley から実測(CJK 過小評価対策・#632 の方針を継承)
    let per_char_px = path_full.size().x / (result.path.chars().count().max(1) as f32);
    truncate_middle(&result.path, avail, per_char_px)
};
```

ADR を書いたときにこの呼び出し点を見ていなかった。

### 理由 2「接尾辞の実幅も別途推定が要る」→ **推定そのものを消せる**

`folder_hint(l, 候補)` を**丸ごと**測れば、固定部（日本語の接尾辞・英語の接頭辞 + 接尾辞）の幅を別に出す必要が無い。今回は「候補を書式へ埋めた文字列の実幅」を測定関数として注入する形にするので、固定部の推定は設計から消える。

### 理由 3「実運用のパスはたいてい収まる」→ 今も正しい

変わったのは頻度ではなく、溢れたときの質（「読めない」→「区別できない」）である。

## egui 0.35 の hint 省略機構（registry の実物で確認）

`~/.cargo/registry/src/index.crates.io-*/egui-0.35.0/src/widgets/text_edit/builder.rs`:

- `:135` — `margin: Margin::symmetric(4, 2)`（`TextEdit` の既定）
- `:592-599` — 入力が空で `hint_text` があるとき、最初の Text atom に `atom_shrink(true)` / `atom_grow(true)` を付ける（コメント: `// elide the hint_text if needed`）。**ゆえに省略は AtomLayout が担い、`RichText::new(hint)` は単一 atom なので組み立て後の文字列の末尾に当たる**
- `:614` — `let available_width = allocate_width - margin.sum().x;`

→ hint に許される内幅は **`allocate_width - 8.0`**。`allocate_width` は `add_sized` に渡す `desired_size.x` = `ui.available_width()`。

**未検算の残り**: `builder.rs:721-725` に `outer_margin(Margin::same(-(visuals.expansion as i8)))` があり、frame expansion がこの内幅にどう効くかまでは読み切っていない。**推定で埋めず、カテゴリ D の実測（`…` が中間と末尾へ二重に付いていないこと）で閉じる**。

## 再利用できる既存パターン

- **測定は `ui.painter().layout_no_wrap(text, font, color)`** — `results_view.rs` と同じ手。galley は egui がキャッシュするので、同じ文字列の再測定はハッシュ引きになる
- **純粋核 + driver の分離** — `layout.rs` が egui 非依存の純関数を持ち、`view.rs` が値を配線する（`Metrics` / `present_results` と同じ形）。今回は **測定関数を注入する**ことで「幅で縮める」アルゴリズム全体を純粋核へ入れられる
- **フェイク測定によるユニットテスト** — 測定を注入するので、`ASCII=10px / CJK=20px` のようなフェイクで CJK 混在の縮み方まで固定できる

## 技術的制約

1. **幅とフォントは closure 内でしか取れない**。`ui.available_width()` / `ui.painter()` は `Frame::show` に渡る内側の `ui`。hint 構築は現在 closure の**外**
2. **`folder_current_dir()` の読み取り位置は動かせない**（`view.rs:370-377` の長文コメント）。`update()` 冒頭からこの位置までに `&mut self.controller` を取る呼び出しが挟まり、前寄せすると hint が**遷移前**のディレクトリを描く。→ **読み取りは現在位置に据え置き、書式化と省略だけを closure 内へ移す**
3. **優先度ラダー（tool > folder > results）と `Option` 直接分岐には触らない**（ADR 却下 4・却下 5 が理由を持つ）
4. **`truncate_middle` の `max_chars < 4` ガードは幅を非単調にする**（keep=3 で元文字列全体を返す）。二分探索の探索範囲を `[4, n]` に閉じる必要がある
5. **三点が 2 種類混ざる**: 書式の末尾は ASCII `...`（parity・`folder_hint_uses_ascii_ellipsis_not_u2026` が測る）、中間省略は U+2026 `…`（`truncate_middle` と同じ・結果行と揃う）。**混在は意図である**
6. **release は `panic="abort"`**。文字境界・空文字列・`max_chars` 境界で範囲外アクセスを作らない（`truncate_middle` の doc が同じ理由を記録）

## 未解決の疑問（plan.md「未確定」で潰す）

- `truncate_middle` を `layout.rs` へ統合するか、`truncate_middle_chars` だけ切り出して委譲させるか
- hint の内幅が `ui.available_width() - 8.0` で正しいか（frame expansion の効き方）
- SPEC §6.7 と ADR の実測値（「71 字なら全部見え、94 字で削られる」）をどう更新するか
