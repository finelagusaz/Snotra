# research: #836 フォルダ展開中に現在のフォルダを画面に示す

## issue の要約

フォルダ展開中、画面のどこにも「いま何というディレクトリを見ているか」が出ていない。#743 の誤読（`←` が階層を上げていないように見えた）を成立させた条件の 1 つがこれ。現在のディレクトリを画面に示す。

`SearchState::folder_current_dir()` は既に存在し `#[allow(dead_code)]`（表示側だけが無い）。

## 一次証拠: WebView2 版の as-built（撤去済みフロントを git 履歴から復元）

issue コメント（2026-07-28 16:09Z）が挙動を、添付スクリーンショットが実物を示している。撤去コミット `15933af`（#532 SU7 PR3）の親から実物のソースを復元して裏取りした。

| 出所 | 内容 |
|---|---|
| `git show 15933af^:ui/src/components/SearchWindow.tsx:269-281` | `placeholderText()` は `viewKind()==="folder"` のとき `t("search.placeholder.folder", { dir: fs.currentDir })` を返し、`<input placeholder>` へ渡していた |
| `git show 15933af^:ui/src/lib/i18n.ts:42` | Ja: `{dir} 内を検索...`（`{dir}` + U+0020 + 内を検索 + **ASCII ピリオド 3 個**。`od -c` で確認） |
| `git show 15933af^:ui/src/lib/i18n.ts:76` | En: `Search in {dir}...`（`{dir}` の直後にピリオド 3 個・空白なし） |
| issue 添付スクリーンショット | `C:\Toolbox\ghost-launcher 内を検索...` が入力欄に淡色で表示。キャレットは先頭 |

**コメント散文とスクリーンショット/ソースが食い違う。** コメント本文は「**（フォルダ名）**内を検索…」と書くが、スクリーンショットも復元したソース（`fs.currentDir` をそのまま渡す）も**フルパス**である。一次証拠が 2 つとも一致するため、**フルパスを採る**。

**描画面は入力欄の placeholder ただ 1 つである。** HTML の `placeholder` は input の値が空のときだけ描かれる——フォルダ展開中の input 値はフォルダ内絞り込み文字列なので、**1 文字でも打つとパスは消える**。WebView2 版はこの挙動で出荷されていた（別の面に逃がす実装は無い・`SearchWindow.tsx` 全文で確認）。

## 関連コード

| ファイル:位置 | 事実 |
|---|---|
| `src-tauri/src/egui_shell/view.rs:341-357` | `in_tool` / `in_folder` を `view_kind()` から算出済み。`hint` は `let hint: &str` で `in_tool ? tool_select_hint : search_hint` の 2 分岐 |
| `src-tauri/src/egui_shell/view.rs:373-375` | `in_folder` のとき `buf` は `folder_filter()`（= 入力欄の値）。egui の `hint_text` は buf が空のときだけ描かれる（HTML placeholder と同条件・同ファイルのコメントが明記） |
| `src-tauri/src/egui_shell/view.rs:419` | `.hint_text(egui::RichText::new(hint).font(bar_font))` |
| `src-tauri/src/egui_shell/search_state.rs:260-265` | `folder_current_dir()` は `#[allow(dead_code)]`。doc は「driver は `parent_dir()` 越しに使い、生の accessor は直接呼ばない」と**断定している** |
| `src-tauri/src/egui_shell/search_state.rs:236-258` | `enter_folder` / `navigate_folder` が `current_dir` を書く。**列挙の非同期到着より前に書かれる** |
| `src-tauri/src/egui_shell/search_state.rs:300-312, 330-338` | `enter_tool` は folder frame を残したまま `tool` を積む。`view_kind()` は tool > folder ゆえ tool-on-folder では Tool。Escape で `saved_folder_filter` が復元される |
| `src-tauri/src/egui_shell/strings.rs` | 文言テーブル。`search_hint` / `tool_select_hint` は `&'static str`、引数を取るものは `String` を返す（`launch_failed` 等） |
| `src-tauri/src/egui_shell/mod.rs:79` | `pub(crate) use strings as ui_strings;` |
| `src-tauri/src/egui_shell/notify.rs:33-43` | `overlay_kind` は **排他ラダー** indexing > launching > notice |
| `src-tauri/src/egui_shell/layout.rs:62-68` | `main_window_height = bar_height + status_height? + toast_height?` |
| `SPEC.md:180-187`（§4.7） | 「案内の描画面は status 行ただ 1 つ」「**入力欄の hint は本来のプレースホルダに徹する**」（#700） |
| `SPEC.md:216-261`（§6） | フォルダ展開機能。現在ディレクトリの提示に関する記述は**無い**（issue 本文が言う「任意扱い」の文言は SPEC 側には存在せず、`search_state.rs:261` の doc コメントが SPEC §6 を指してそう述べている） |

（上表のファイル・関数・行はすべて実物を開いて確認済み）

## 既存パターン

- **文言テーブル + 引数**: `launch_failed(l, detail) -> String` / `update_available(l, version) -> String` が先例。`&'static str` を返す関数と混在してよい
- **純粋核 + ユニットテストで固定**: `overlay_kind`（`notify.rs`）が分岐を純粋関数へ出してテストで優先順を固定している。文言の codepoint 一致も `strings.rs` の `tests` が固定している（`hotkey_change_failed_matches_i18n` 等）
- **status 行と hint は別の役割**: `overlay_text`（`String`・毎フレーム生成）が status 行、`hint`（`&str`）が入力欄

## 技術的制約（一次資料で確認）

- **egui 0.35 の `hint_text` は singleline で自動的に末尾省略する。**
  - `egui-0.35.0/src/widgets/text_edit/builder.rs:675-680`: `wrap_mode = if multiline { Wrap } else { Truncate }`（コメント「This wrap mode only affects the hint_text」）
  - 同 `:584-600`: buf が空のとき hint atom に `atom_shrink(true)` を付ける。`atom_layout.rs:382-393` で shrink atom が残余幅を受ける
  - `epaint-0.35.0/src/text/text_layout_types.rs:663-671, 700-708`: `TextWrapping::truncate_at_width` は `overflow_character: Some('…')`（Default 由来）
  - → **溢れたら末尾が `…` になる。追加の省略機構は要らない**
- **hint の色は `Visuals::weak_text_color` だけが効く**（`builder.rs:591` が `map_texts` で無条件上書き。egui 自身が "users won't be able to override it" と注記）。`view.rs:299` で `visual.hint` を設定済みゆえ、新しい文言も自動で同じ色になる
- **可用幅の見積り**: `window_width` 既定 600（`snotra-core/src/config.rs:356`。既定リテラルはここ 1 か所・#795）。`view.rs` の toast ブロックのコメントが既定幅での可用幅を 532px と記録している。`font_size` 既定 15。Ja 接尾辞「 内を検索...」は概算 90px ゆえパス部の予算は概ね 440px ≒ ASCII 55〜60 字
- **`main_window_height` は変わらない**: hint は入力欄の中に描かれ、行を積まない

## 未解決の疑問

なし（下記は plan.md 側で裁定する設計判断であって、調査で判明しなかった事実ではない）。

- 絞り込み文字列を打った瞬間にパスが消えることを受容するか（WebView2 as-built はそうだった）
- 末尾省略（egui 既定）でよいか、先頭側を省略してフォルダ名と接尾辞を残すか
