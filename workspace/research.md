# research: #949 テーマ 3 値の適用を `search_input_ui` へ吸収する

## issue の要約

#751 が新設した 2 つの不変条件のうち、**禁止**（`Context` 経由で global style を書かない）は
#900 が `src-tauri/clippy.toml` の `disallowed-methods` で機構化した（マージ済み・657d9fd）。
残る**順序**（適用は visuals を読む最初の操作より前）は「検知手段が無い受容残余」として
`ADR-visuals-application-target` と `src-tauri/CLAUDE.md` に記録されている。

本 issue は、3 値の適用を唯一の消費者を描く関数 `search_input_ui` へ吸収して位置を
「関数の入口」に固定し、**同一 pass の子 `Ui` を実コードのまま観測するテスト**で守る案である。

## 関連ファイル・モジュール・関数（すべて grep で実在確認済み）

| 対象 | 位置 | 役割 |
|---|---|---|
| `search_input_ui` | `src-tauri/src/egui_shell/view.rs:220` | `TextEdit` を描く唯一の関数。`RuntimeFrame` にも controller にも触らず kittest から実コードで駆動できる |
| `SearchInputParams` | 同 `:190` | `search_input_ui` が要る 1 フレーム分の値 |
| 3 値の適用点 | 同 `:503-510` | `let visuals = ui.visuals_mut();` + 3 代入。**移設元** |
| 適用点の長文コメント | 同 `:480-502` | 順序不変条件の**正本**（受容残余の記述を含む） |
| `search_input_ui` 呼び出し | 同 `:688` | `update()` 内の唯一の製品呼び出し。`hint` クロージャを渡す |
| `hint` クロージャ本体 | 同 `:688-729` | **子 `Ui` を引数に受ける**（`Frame::show` の中で呼ばれる）＝観測点 |
| `ui_visuals_mut_reaches_child_ui_in_the_same_pass` | 同 `:1378` | 既存テスト。egui の伝播だけを測る（本体の呼び出し位置は縛らない旨をコメントが明記） |
| `caret_harness` / kittest 3 検査 | 同 `:1236-1360` | `search_input_ui` を実コードのまま駆動する既存パターン |
| `VisualSnapshot.{input_bg, selection, hint}` | `src-tauri/src/egui_shell/visual.rs:70,72,74` | 3 値の供給元 |
| `read_visual` | `src-tauri/src/egui_shell/mod.rs:391` | 1 フレーム 1 lock の読み取り |
| `disallowed-methods` 7 本 | `src-tauri/clippy.toml:50-58` | #900 の機構。**禁止**側だけを守る |
| ADR | `docs/adr/ADR-visuals-application-target.md` | 却下 4「新設した順序不変条件に検知器を置く: 見送り」がある |
| 規範 | `src-tauri/CLAUDE.md`「モジュール構成」の `egui_shell/` 項 | 「**この順序に検知手段は無い**」の記述 |

## 3 値の消費者を数え上げた（移設の安全性）

`update()` 全体で `ui.label` / `ui.button` / `ui.add` は **0 件**（`view.rs` 本体での `ui.add` は
テストモジュール内 3 件のみ・grep 実測）。egui ウィジェットは `search_input_ui` の
`ui.add_sized(… TextEdit …)` **1 つだけ**である。

3 値を読まない前置きの描画:

- `ui.interact`（`:397` ドラッグ掴み）— `create_widget` を呼ぶがヒットテスト矩形を積むだけ
- status overlay（`:815-831`）・toast（`:841-`）— `ui.painter()` に色を**明示渡し**する
  （`visual.input_bg` / `visual.hint` を config 由来の値として直接読む。style を経由しない）

results 窓は別 `Context` ゆえ影響外（`visual.rs` の doc・`RowTheme` を別に持つ）。

→ **移設しても現在の描画結果は変わらない。**

## 再利用できる既存パターン

- **kittest で `search_input_ui` を実コードのまま駆動する**（`caret_harness`・#872 で確立）
- **クロージャ引数を観測点にする**（`hint: impl FnOnce(&mut egui::Ui) -> String` が既に子 `Ui`
  を渡している。テスト側で `String::new()` を返すだけの実装を渡す先例が `:1249` にある）
- **`ctx.run_ui` を 1 pass だけ走らせて初回 pass を測る**（`:1388` の既存テスト）
- **故障注入で discriminate を確かめてからテストを確定する**（`:1279-1283` のコメントが先例。
  「キャレットを先頭へ置かないと検査は discriminate しない」を実測で発見している）

## 技術的制約

- `egui = "=0.35.0"`（ルート `Cargo.toml:11`）/ `egui_kittest = "0.35"`（dev-dependency・テスト専用）
- 子 `Ui` は生成時に親の style を `Arc::clone` する（`ui.rs:236`・既存コメントが一次資料で確認済み）
  → **親への `visuals_mut()` は「子の生成より前」でしか届かない**。これが順序不変条件の機序
- `weak_text_color` は `Option<Color32>`。読む側は解決 getter `weak_text_color()` を使う
  （生フィールドを見ると TextEdit の実経路を素通りする・既存テストのコメント）
- ADR は凍結された歴史であり**書き換えない**（`ADR-adr-frozen-history`）。上書きは新 ADR で行う
- `src-tauri` は `[lib]` を持たないため `cargo test -p snotra --lib` は常に失敗する
- clippy.toml は cargo の fingerprint に入らない（今回は触らないが、周辺知識として）

## 未解決の疑問（`plan.md` の未確定欄へ引き継ぐ）

1. **新テストは issue が挙げる 3 つの回帰で本当に落ちるか。** とくに「`ui.visuals_mut()` を
   子 `Ui` の生成より後ろへ移す」は、移設後は「`Frame::show` の呼び出しより後ろ」でしか
   再現しない（`Frame::show` の**中**で child へ適用する形は正しく動くので退行ではない）。
   `ctx.set_visuals` へ戻す形は clippy が既に塞いでおり、テストは二重の防御になる。
2. **移設は 3 値の到達範囲を縮める。** 現状は `update()` 冒頭適用ゆえ以後の全描画に届くが、
   移設後は `search_input_ui` 以降に限られる。ADR の帰結 2（新しい egui コンテナを足すなら
   自分で visuals を渡す）がより厳しくなる ＝ **新しい受容残余**として記述が要る。
3. **既存テスト `ui_visuals_mut_reaches_child_ui_in_the_same_pass` を残すか。**
4. **新 ADR の slug 名**と、旧 ADR・`src-tauri/CLAUDE.md` の書き換え範囲。
5. **カテゴリ D の要否。** `check:colors` が自動判定するのは main/results の**背景**だけで、
   3 値（入力欄背景・選択色・hint 色）は目視項目である。`smoke:manual` はエージェントが
   実行できない（`Read-Host`）。
