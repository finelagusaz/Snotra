# plan.md の独立レビュー — issue #1268 Ctrl+H 案

結論: 判定表の骨格（正常 3→2・変異 1 で 3→3・脱落で観測なし）は一次資料で裏が取れた。要対処は 1 件（実装の落とし穴）で、設計の差し替えを要する所見は無い。

## 要対処

1. **述語クロージャは `.GetNewClosure()` が必須で、`$base` を StrictMode 下で束縛しないと throw する。**
   - `Wait-SnotraTraceCondition` は述語を `SnotraSmoke.psm1` のモジュールスコープで `Where-Object -FilterScript` に渡す（`SnotraSmoke.psm1:686`）。smoke スクリプトのローカル変数 `$base` は素のスクリプトブロックからは見えない。
   - 既存の `:412` は `.GetNewClosure()` を付けている。計画の「`[long]seq -gt base` を待つ」はこの形を踏襲すると明記すべき。
   - 付け忘れると `$base` が未定義になり、StrictMode では例外、そうでなければ `-gt $null` で常に真になる。後者は「ベースライン以前の 3 文字入力イベント自体に一致して即成立 → 区間内の件数が狂う」という偽の赤になる。緑にはならないが、原因の見えにくい赤を作る。
   - 区間の切り出しも同様。`Read-SnotraTraceSnapshot` の `.Events` から `[long]$_.seq -gt $base` で絞るとき、`seq` は trace.rs:55 の `"seq"`（u64）。`$pathTyped.seq` の型（JSON 由来の Int64）と揃うので `[long]` キャストで足りる。

## 軽微

1. **計画・research の「`Text("h")`」は不正確。** 変異 1 が積むのは `egui::Event::Ime(ImeEvent::Commit("h"))` であり `Event::Text` ではない（`input.rs:411-413`、`typed_text_event` → `committed_text_event` が `Event::Ime(Commit)` を返す・`input.rs:461-464`）。結論（3→3）は変わらない。根拠:
   - `TextEdit` は `owns_ime_events`（= `has_focus(id)`、`memory/mod.rs:1042-1048`）のとき `Event::Ime` を処理し、`Commit` は `clear_preedit_text`（選択範囲の削除。直前の削除で選択は空）→ `insert_text_at`（`builder.rs:1238,1325-1334`）。通常の 1 文字入力も同じ経路を通っているので、`z` `c` `:` `\` が入ることがこの経路の稼働証拠になる。
   - 計画書の表と `snotra-egui-runtime/src/input.rs` に足すコメントでは「`Text("h")`」でなく「Ime Commit `h`」と書くこと。
2. **判定表 (a) の検証結果（同一フレーム・順序）。** 成立する。
   - 変異下の 1 回の `on_keyboard_event` は `Event::Key{H, pressed, modifiers: self.modifiers}`（`:397-403`）を先に、`Ime(Commit("h"))`（`:411-413`）を後に、同じ `self.raw.events` へ積む。同一 `RawInput` の中で順序は保たれる。
   - `Ctrl+H` が `command` 分岐（C/X/V のみ・`:372-395`）に捕まらないことも確認した。
   - `filtered_events` は Tab / 矢印 / Escape 以外の `Key` と全 `Ime` を通す（`event_filter.rs:50-62`）。
   - events ループは順に適用する（`builder.rs:1109`）。`cursor_range.on_event` の最初の arm は `Key::H` を消費しない（`cursor_range.rs:116-165`。`H` の arm は無い）。
   - `Key::H if modifiers.ctrl` が `check_for_mutating_key_press` で `delete_previous_char`（`builder.rs:1431-1434`）。その後 `*galley = layouter(...)` と `cursor_range` 更新が入り（`:1347-1358`）、続く Commit が削除後の位置へ挿入する。結果は `c:h`、`changed()` は真、trace は 3→3。
3. **`self.modifiers.ctrl` が `Key` に載る経路。** 成立する。
   - Ctrl の keydown で tao が `ModifiersChanged` を送り、`input.rs:239-246` が保持とイベントの両方を行う。
   - Ctrl 自身の `KeyEvent` は `key_from_tao(Named(Control))` が `_ => None`（`input.rs:518-558`）、`key_from_key_code` にも Control の対応は無いので、egui へ `Event::Key` は積まれない。修飾キー単独の押下・解放で `changed` が出る経路は無い。
   - H 側の `logical_key` は Ctrl 併用時に `layout.get_key(mods_without_ctrl, ...)` で `Character("h")` になる（tao-0.37.1 `keyboard.rs:551,570-583`）。`WM_CHAR` 0x08 を `Backspace` と誤解釈する経路は無い。`text` は Ctrl を外した `"h"`、`text_with_all_modifiers` は `"\x08"`（`:264-275,623-630,663-673`）。
4. **判定表 (b) の検証結果（正常ビルドで 3→2 のちょうど 1 件）。**
   - `"\x08"` は `is_printable_char`（`is_ascii_control`）で弾かれる（`input.rs:445-447`）ので文字は入らず、`Key::H` の削除だけが効く。`H` の解放イベントは `pressed:false` で `builder.rs:1231-1236` の `pressed: true` に当たらず、`typed_text_event` も `pressed` で弾く。
   - `egui_input:changed` の発行元は `view.rs:874` の `response.changed()` の 1 か所だけ（`search_flow.rs:246`）。`before_chars` は `state.query()`（`:239-244`）で、パス入力後は 3。
   - **`$pathTyped` が `after_chars -eq 3` の最後の 1 件であることは `seq` 以降の混入を起こさない。** `after_chars == 3` になりうる遷移はパス入力の最後の `\` だけ（それ以前は 0〜2。`z` 再注入ループは最大 1〜2 文字）。パスクエリの遅れて届く変化は `egui_input:changed` ではなく `egui_search:settled` / `results` 系の事象で、`egui_input:changed` を発行するのは `changed()` が真のフレームだけ。
5. **カーソル位置の前提。** `Key::H` の削除は `cursor_range.primary` の直前 1 字を削る（`builder.rs:1432`）。パス入力直後のカーソルは末尾にあるはずで（挿入で `CCursorRange::one(ccursor)` が末尾へ進む・`:1152,1333`）、末尾以外なら 3→3（削除なし）か別の 3→2 になる。失敗時は「観測なし」か件数不一致で**赤**に倒れるので偽の緑にはならないが、`c:\` 打鍵後に app 側が入力欄のカーソルを動かす経路があれば新ブロックだけが間欠的に赤になる。調べた範囲（`view.rs` の Escape 復元時のキャレット同期 #840 のみ）では打鍵中に動かす経路は見ていない。
6. **ゲート `$null -ne $pathTyped` と `$failures` ゲートの関係。** 計画どおりで足りる。ただし新ブロックが `$failures` を増やした後の Escape ブロックは `if ($failures.Count -eq 0)`（`smoke-egui.ps1:424`）で飛ばされ、プロセスは `finally` で kill される。H1 等の判定も走らない。既存ブロックと同じ流儀で問題ない。
7. **隣接記述の修正。** `smoke-egui.ps1:379-380` の H6 の件は計画どおり。`SnotraTraceInvariants.psm1` の不変条件は `H1/H4/H5/H7` のみ（`:13-17,31`）で H6 は無い。あわせて `:377` の見出し「パスクエリ打鍵（#1004）」の直下コメントが「`$resultsChecked` のブロックが…」とゲートの根拠を述べているので、新ブロックのゲート根拠（`$pathTyped` 非 null）も同じ書き方で足す。
8. **`docs/build-commands.md` の更新不要判断。** 妥当。smoke-egui の説明（`:249-252`）は操作列を網羅していない（パスクエリへの言及なし）。同節は「網羅は担わず、視覚・操作列は手動 GUI smoke が補完する」と明言している。
9. **`snotra-egui-runtime/CLAUDE.md` の写し。** 「通常文字は `KeyEvent::text_with_all_modifiers()` から…`KeyEvent.text` は使わない」の条項に、退行を捕まえる検査の所在を足すかは任意。足すなら `input.rs` のテストコメントと二重になるので、どちらか一方（テストコメント側）に寄せ、CLAUDE.md は触らない方が「かぶりなく」に沿う。

## 検証した範囲での 2 点目（既存シナリオへの影響）— 偽の赤・偽の緑は見つからなかった

- **Escape → `egui_hide:done` / `egui_results:hide`。** 入力が `c:` になっても Escape ラダーは `state.on_escape()` が決め、folder / tool でなければ `Hide`（`hide_request.rs:60-82`）。`c:` への遷移は folder 突入ではない（folder は別操作）。`c:\` と `c:` で分岐は変わらない。
- **`egui_results:hide` の待ちは presence。** 既存でも `z` を Backspace で空にした時点で hide が出ている（research B5）ため、新ブロックが弱めたものは無い。強めても弱めてもいない。
- **H1。** hide 区間以降の余分な `egui_results:show` を見る。Ctrl+H は Escape より前なので窓の外。
- **H4。** `rows=0` の `egui_results:show` を見る。`c:` の検索結果が 0 件なら契約上 show でなく hide が出るので違反は生えない（`present_results` の連言②）。
- **H5。** hide を挟まない連続 show。`ResultsWindow.visible` の `swap` が遷移でだけ発火させる。`c:\` → `c:` で結果が変わるだけでは再発火しない。
- **H7。** `egui_search:settled` の `dispatch_seq < pending_seq`。`c:\` の全件走査が in flight のまま Ctrl+H で新 dispatch が走っても、古い結果は `accept_worker_rows` が弾いて `egui_search:dropped` を出す（`search_flow.rs:189-212`）。`settled` は出ないので H7 に載らない。
- **`egui_input:changed` を読む不変条件は無い。** `SnotraTraceInvariants.psm1` は `egui_input:changed` を参照しない（grep 0 件）。`Observed.ResultsShow -eq 0` の肯定証拠も `egui_results:show` の件数のみ。

## 未検証

1. ⚠️ **変異 1 の実ビルドで 3→3 の 1 件になること（C1・C5）。** ソースの読みでは成立するが、実打鍵で `WM_CHAR` 0x08 が実際に届き `text` が `"h"` になるかは実測していない（tao のコード上は `utf16parts` に 0x08 が入り `text_with_all_modifiers = "\x08"`）。AC2 の変異注入で測ること。**赤の理由が新断言の文言であること**（コンパイルエラーや別検査でないこと）も出力で確認すること（`safety-nets.md`「コンパイルエラーは検知ではない」）。
2. ⚠️ **keybd_event の Ctrl+H が実際に egui の `modifiers.ctrl` を立てて届くか。** `Send-SnotraKey` が VK_CONTROL（0x11）をどう送るか（スキャンコード・extended フラグ）は未確認。tao は `scancode == 0` なら `MapVirtualKeyExW` で補う（`keyboard.rs:533-543`）ので通るはずだが実測はしていない。外れた場合は Ctrl が立たず H 単独の 3→4 か観測なしで、いずれも赤（偽の緑にはならない）。
3. ⚠️ **ローカル環境依存の赤の可能性。** IME ON の PC では Ctrl+H が IME に奪われうる（Microsoft IME の Ctrl+H は変換中の Backspace 相当）。window_coordinator.rs:450 の `TurnOffIme` が show 時に撃たれるので通常は問題ないが、実機で赤になるなら最初に疑う。別アプリのグローバルホットキーが Ctrl+H を奪う環境も同様に赤になる（偽の緑ではない）。
4. ⚠️ **偽の緑になる経路の網羅について。** 見つかった緑の経路は無い。確認した分岐は次のとおり。
   - Ctrl 脱落 + H 到達 → 3→4 で赤。
   - Ctrl 到達 + H 脱落 → 観測なしで赤（`ModifiersChanged` だけでは `changed` は出ない）。
   - 変異 1 → 3→3 で赤。
   - Ctrl+H 全脱落 → 観測なしで赤。
   - 変異 1 のうち Key と Commit が別フレームに割れる形は、同一 `on_keyboard_event` 呼び出し内で積むので構造上起きない。
   - 検知できない変異は 1 つ（意図どおり）で、変異 2（`admit_key` 前の配送）は射程外。
   - 残る穴は「同じ `3→2` を別の原因で作る退行」だけ。たとえば Ctrl+H が Backspace 相当として別経路で処理される退行は、文字が入らず 1 字消えるので判定上は緑のまま。これは変異 1 の検出目的の外。
5. 未実走: 本レビューは smoke を実走していない（依頼どおり）。`npm run test:powershell` や `cargo` の検証も未実行。
