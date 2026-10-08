# 調査: #1266 tauri 2.12 へ移行する（tao 0.37 で通常文字の ReceivedImeText が消えた）

作業ブランチ: `chore/deps-20261008`（draft PR #1265）。`Cargo.toml` の固定 3 行を `=2.12.1` へ上げ済み、main（#1267）を merge 済み。

## issue の要約

tauri / tauri-runtime / tauri-runtime-wry を 2.12.1 へ揃えると、ビルド・テスト・clippy・`npm test` は緑のまま smoke-egui が 2 項目で赤になる（CI run 37781825112・ローカルで決定的に再現）。

1. `egui_results:show not observed within 8000ms x2 after typing 'z'`——通常文字が入力欄に入らない（`egui_input:changed` が `before_chars=0 after_chars=0`）
2. `toast ありの show #1: 可視区間中に egui_main:bar_rect_mismatch が 1 件`（show 616x82 / frame 616x52）

## 1. 通常文字が届かない

### 事実（一次ソース・tao 0.35.3 / 0.37.1 のレジストリ展開を読んだ）

- 0.35.3 は `platform_impl/windows/minimal_ime.rs` が `WM_CHAR` / `WM_SYSCHAR` を 1 文字ずつ `WindowEvent::ReceivedImeText` にしていた（`is_msg_ime_related` が `WM_CHAR` を含む）。IME 確定も、`DefWindowProc` が `GCS_RESULTSTR` から作る `WM_IME_CHAR` → `WM_CHAR` をこの層が `ENDCOMPOSITION` 以降まとめて拾っていた
- 0.37.1 には `minimal_ime.rs` が無い。`ReceivedImeText` を送るのは `event_loop.rs` の `WM_IME_ENDCOMPOSITION` 分岐だけで、中身は `ImeContext::get_composed_text()`（= `ImmGetCompositionStringW(GCS_RESULTSTR)`）
- 0.37.1 の `keyboard.rs` は `WM_CHAR` を直前の `WM_KEYDOWN` の `event_info` に連結して `KeyEvent.text` にする。**`event_info` が無い `WM_CHAR`（IME 由来）は "The message is probably IME" として捨てる**（`MatchResult::Nothing`）
- `KeyEvent.text` は Ctrl が押されていると **Ctrl を外した文字**になる（`ctrl_on` 枝が `layout.get_key(mod_no_ctrl, …).to_text()` を入れる）。Ctrl+A で `"a"`
- `KeyEvent::text_with_all_modifiers()`（公開 API・`event.rs:805`）は `utf16parts` そのもの＝ `WM_CHAR` / `WM_SYSCHAR` の中身。tao 自身の doc が「Ctrl+a は `Some("\x01")`」と書く。**旧 `minimal_ime` が `ReceivedImeText` で送っていた文字列と同じもの**である
- `VK_PROCESSKEY`（IME 変換中の打鍵）は `logical_key = Key::Process`。`WM_CHAR` が続かないので `text` / `text_with_all_modifiers` は `None`（`PartialKeyEventInfo::from_message` の初期値 `PartialText::System(Vec::new())`・`finalize` は空なら `None`）

### この crate の現状

- `snotra-egui-runtime/src/input.rs` の `on_keyboard_event` は `KeyEvent.text` を意図的に捨てる（末尾コメント「Tao emits normal WM_CHAR text and IME commits through ReceivedImeText. Routing KeyEvent::text too would insert every printable character twice.」）
- `WindowEvent::ReceivedImeText(text)` → `committed_text_event`（空と制御文字を弾く）→ `egui::Event::Ime(ImeEvent::Commit)`。**通常文字も IME 確定も同じ `Commit` で egui へ入っている**
- `snotra-egui-runtime/src/windows_ime.rs` の `classify_ime_message`: `WM_IME_STARTCOMPOSITION` と未確定の `WM_IME_COMPOSITION` は `Suppress`、`GCS_RESULTSTR` を含む `WM_IME_COMPOSITION` と `WM_IME_ENDCOMPOSITION` は `PassThrough`（tao へ）。`ENDCOMPOSITION` では自分で空の `Preedit` を送ってから通す
- 不変条件の正本: `snotra-egui-runtime/CLAUDE.md`「一般」の 2 項目（「通常文字と IME 確定文字は Tao の `ReceivedImeText` だけを egui へ渡し…」「…確定 `GCS_RESULTSTR` だけ Tao へ通し `ReceivedImeText` で受ける」）

### 0.37 で IME 確定を tao の経路に任せたときの懸念（未実測）

- **`ENDCOMPOSITION` を伴わない確定を落とす**: `GCS_RESULTSTR | GCS_COMPSTR` が同時に立つ `WM_IME_COMPOSITION`（変換中に続けて打鍵して前の文節が確定する・韓国語 IME の音節確定）では `ENDCOMPOSITION` が来ないか、来ても最後の分しか `GCS_RESULTSTR` に残らない。0.35 は `WM_CHAR` 経由で毎回拾えていた
- **`ENDCOMPOSITION` 時に古い `GCS_RESULTSTR` を読む**: Escape で変換を取り消したときも `ENDCOMPOSITION` は来る。そのとき `GCS_RESULTSTR` が前回の確定を保持していれば二重確定になる
- **確定文字が `KeyEvent.text` へ混ざる**: `GCS_RESULTSTR` を tao 経由で `DefWindowProc` へ通すと `WM_IME_CHAR` → `WM_CHAR` が生まれる。`event_info` が無ければ tao は捨てるが、直前の `VK_PROCESSKEY` の `WM_KEYDOWN` が `next_kbd_msg` でそれを拾って `event_info` を残す並びがあると、確定文字が `KeyEvent.text` にも乗る

### 再利用できるパターン

- `committed_text_event`（空と制御文字を弾く）——`text_with_all_modifiers` へ当てれば旧経路の弾き方と同じになる
- `windows_ime.rs` は既に `ImeContext::composition_data(kind)` で任意の `GCS_*` を読める（`GCS_RESULTSTR` も同じ関数で読める）。preedit を `mpsc` で送り `InputState` が `drain` する経路（`PlatformIme::drain`）がそのまま確定にも使える
- winit の Windows 実装は `WM_IME_COMPOSITION` で `GCS_RESULTSTR` を自分で読み、`DefWindowProc` を呼ばずに 0 を返す（`WM_IME_CHAR` を作らせない）——**ローカルにソースが無く未確認。前例として挙げるだけで根拠にはしない**

## 2. `bar_rect_mismatch`（起動後の初回 show）

### 実測（2026-10-08・ローカル・DPI 100%・`read_frame_geom` と show / 検出器へ一時 `eprintln!` を入れて smoke を走らせた。計装は戻した）

toast シナリオ（`target/smoke-egui/profile-toast/stderr.log`）:

```
TMP_SHOW bar_height=43 toast_h=43 width=600
TMP_GEOM label=main visible=Ok(false) outer=616x61 inner=600x22 scale=1   ← 起動後の初回 show（まだ一度も表示していない）
egui_show:done height=86
TMP_GEOM label=main visible=Ok(true)  outer=616x95 inner=600x86 scale=1
TMP_CHECK bar_height=43 show=616x82 frame=616x52                         ← 発火
...（hide → show）
TMP_GEOM label=main visible=Ok(false) outer=616x95 inner=600x86 scale=1   ← 2 回目以降は正しい
TMP_CHECK bar_height=43 show=616x52 frame=616x52
```

- バー高（論理 43）は両側で同じ。**ずれは `read_frame_geom` の `inset_h`（`outer − inner`）が初回 show だけ 39、以後 9 であること**。差の 30 はキャプション 1 本分
- 生成時は `inner_size(window_width, 52.0)`（`egui_shell/mod.rs` の `create`）。outer 61 = 52 + 9 は tao が装飾なしとして外形を計算した結果で、client 22 = 52 − 30 は OS がキャプション付きとして非クライアント領域を計算した結果——**外形と非クライアント計算が食い違ったまま生まれている**
- 一度 `set_size` が当たる（`SetWindowPos` でサイズが変わる → `WM_NCCALCSIZE` が tao の subclass に届く）と 9 に戻る

### 機序（読んだもの・推定を含む）

- 0.35.3 の `window.rs` は生成専用の `window_proc` を持ち、「subclass が付くまでの間も `WM_NCCALCSIZE` に装飾なしとして応答する」とコメントしていた
- 0.37.1 は winit 式の `InitData` に変わり、`create_window`（`WM_NCCREATE` 相当の段）で `WindowState::set_window_flags`（→ `SWP_FRAMECHANGED`）を撃つ。**この時点で装飾なしの `WM_NCCALCSIZE` 処理が繋がっていない**ため `DefWindowProc` がキャプション付きで計算した、というのが推定。**機序の裏付けは tao のソースを最後まで追っておらず、実測（初回だけ 39）だけが一次証拠である**

### 影響

- 起動後の初回 show で、位置決め（`position_on_target_monitor`）に渡すバー矩形の高さが 30 物理 px 大きい。作業領域の下端近くへ保存位置がある場合だけ、バーが 30 px 上へ置かれうる（可視中のクランプは `!any_down()` のフレームで走るが、上へ置かれたものは境界内なので戻さない）
- 見た目のキャプションは出ない（show が `set_size` を撃ってから `show()` するため、表示時には 9 に戻っている）
- results ウィンドウも同じ生成経路（`decorations(false)`・`visible(false)`）。results の幾何を読む関数が初回 `set_size` 前に results の inset を読むかは未確認

## 3. 版を名指した根拠の再検証が要る箇所

挙動がその版に依存している記述（コードと規範文書）。日付付き設計書（`docs/superpowers/`）と ADR は凍結された歴史ゆえ対象外。

| 箇所 | 依存している挙動 |
|---|---|
| `snotra-egui-runtime/src/input.rs:83`・`:220` / `snotra-egui-runtime/CLAUDE.md`「focus を獲得した瞬間に押されていたキー」 | `WM_SETFOCUS` で合成 press を作る・合成 press が `Focused(true)` より先に届く（`admit_key` の前提。崩れると Escape で 2 窓が閉じる／Escape が永久に効かない） |
| `snotra-egui-runtime/src/runtime.rs:517` | `set_cursor_icon` が `SetCursor` を直接撃つ |
| `snotra-egui-runtime/src/proof.rs:10` | runner の `event_buffer` 再入規律 |
| `src-tauri/src/egui_shell/window_coordinator.rs:398` | 当該メッセージを tao の wndproc が扱わない |
| `src-tauri/src/egui_shell/results_window.rs:150`・`:240` / `src-tauri/CLAUDE.md`「新しい操作を raw へ寄せるかは…」 | `set_size` / `set_position` / `set_background_color` が `apply_diff` で差分ゼロなら早期 return・`apply_diff` 末尾の `SW_HIDE` |
| `src-tauri/CLAUDE.md`「宣言的なウィンドウ属性…」 | `MARKER_DONT_FOCUS` と `SW_SHOWNOACTIVATE` の分岐 |
| `src-tauri/CLAUDE.md`「Win32 メッセージ配送の注意」・「setup フック自身もイベントループの中で走る」 / `src-tauri/src/main.rs:301` / `src-tauri/src/egui_shell/mod.rs:383` | tauri 2.11.4 の `emit_filter` 同期実行・setup が `Ready` arm で走る・`impl Clone for Window` |
| `SPEC.md:449` | `@tauri-apps/cli` 2.11.4 の NSIS テンプレートの `DeleteRegValue`（CLI は 2.12.1 へ上がる） |

## 技術的制約

- IME の確定経路は**人間の実打鍵でしか確かめられない**（`SendInput` で IME を駆動できない。メモリ `feedback_win32_input_trace_smoke`）。`SNOTRA_EGUI_INPUT_TRACE` / `SNOTRA_EGUI_IME_TRACE` の trace 照合で実観測する
- smoke-egui はローカルで決定的に赤を再現できる（`scripts/smoke-egui.ps1 -ExePath target/release/snotra.exe`。`npm run smoke:egui -- -ExePath` は PowerShell から呼ぶと `--` が食われて npm が引数を誤読する）
- `cargo test` / clippy / `npm test` は両方の回帰を通す——検知はカテゴリ C（smoke）だけ
- `egui = "=0.36.1"` の固定はこの件と独立

## 敵対的調査（`workspace/adversarial-1266.txt`・sonnet 1 体・ソース読みのみ）の採否

| 争点 | 判定 | 採否と理由 |
|---|---|---|
| 1 通常文字は `KeyEvent` だけ・IME 由来 `WM_CHAR` は捨てる | 壊せず | 採る（本文どおり） |
| 2 `text_with_all_modifiers` ≡ 旧 `ReceivedImeText` | 壊せた 3 件 | **採る**。(a) 合成 press は `text_with_all_modifiers = Some(..)` を持つ → text の配送は `admit_key` の**後**（`on_keyboard_event` の内側）に置く。(b) サロゲートは旧経路が 1 単位ずつ `from_utf16` して**捨てていた**のが 0.37 で連結される——改善側の差なので受け入れ、テストで固定する。(c) focus 獲得時に押下中だったキーの文字も release まで落ちる——`admit_key` の意図（合成 press を実らせない）と同じ向きなので受け入れ、`CLAUDE.md` に明記する |
| 3 `VK_PROCESSKEY` は text 無し・確定文字の混入経路 | 壊せず（⚠️実在未測） | 採る。混入の機序の説明は委譲側の訂正（再入中の通常キーへ連結）を採る。いずれにせよ**確定 `WM_IME_COMPOSITION` を `DefWindowProc` へ通さなければ `WM_IME_CHAR` → `WM_CHAR` 自体が生まれない**ので、設計で経路ごと消す |
| 4 初回 inset 39 の機序・`FRAMECHANGED` で直る | 一部壊せた | **採る**。「`set_size` で 9 に戻る」は `set_inner_size` が stale な offset を足す読みと矛盾し、直したのは `position_on_target_monitor` の `SetWindowPos` と推定される（未実測）。**位置が動かない初回 show では outer が 30 px 大きく出うる**——生成直後に `FRAMECHANGED` を撃てばどちらの経路にも依存しなくなる。直ったことは計装で実測する（計画の未確定）。results も同じ生成経路 |
| 4 追加: 既定シナリオは `bar_rect_mismatch` を断言しない | 壊せた | 採る（事実）。smoke の断言を増やすのはセーフティネットの変更ゆえ**計画に入れず提案に留める** |
| 5 版依存表の崩れ | 崩れ 0 行（⚠️ 2） | 採る。⚠️ `PendingEventQueue` 新設で合成 press の順序保証が弱まった——破綻は「遅れた press が次の `Focused(false)` の後に届く」並びだけで fail-open 側ではない。受容残余として `CLAUDE.md` の記述を 0.37.1 の根拠へ書き換える。行 6 の機序の不正確さは既存の誤り（本件と独立）——該当文だけ直す。行 8（NSIS）は確認不能——CLI 2.12.1 で `installer.nsi` を生成して読む（計画の未確定） |
| 6 表外の版依存 | 壊せた | 採る。`proof.rs:40`・`windows_ime.rs` の "Tao's own subclass"・`view.rs:1632`/`:1729`・`results_window.rs:107-111`・`window_coordinator.rs:1136` を計画の文書更新へ加える |
| 測定環境 | 壊せた | 採る。smoke の打鍵は `keybd_event(vk)` で実キーボードと同じ枝（`VK_PACKET` ではない）——**smoke の赤→緑は通常文字の修正の証拠になる**。IME 確定・AltGr・デッドキー・サロゲート・合成 press は smoke の外。smoke は `window.bin` を消すので保存位置の枝は通らず、DPI は 100% のみ |

## 未解決の疑問

- IME 確定を tao の `ENDCOMPOSITION` 経路に任せるか、`windows_ime.rs` で `GCS_RESULTSTR` を自分で読むか（上の 3 つの懸念がどれだけ実在するか）
- 初回の inset 39 は main だけか results もか。直し方（生成直後に `SWP_FRAMECHANGED` を撃つ等）で 39 が 9 になるか
- 表の再検証で崩れている前提があるか
