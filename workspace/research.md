# 調査 — issue #1268 smoke-egui に Ctrl+英字の打鍵を足す

## issue の要約

#1265（#1266）で通常文字を `KeyEvent::text_with_all_modifiers()` から取るようにした。変異注入（#1265 PR 本文「変異注入」）で次の 2 つがどの検査にも落ちないと判明した。

1. `event.text_with_all_modifiers()` → `event.text` の取り違え（Ctrl+A で `a` が入る）
2. 文字の配送を `admit_key` より前へ動かす（focus 復帰時に押下中キーの文字が入る）

提案: `scripts/smoke-egui.ps1` に Ctrl+英字の打鍵を 1 本足し、1 を捕まえる。足した検査が変異 1 のビルドで実際に赤になることを変異注入で確かめてから入れる。2 はこの形では捕まらず、費用判断は別。smoke はセーフティネットゆえ合意が要る（ルート `CLAUDE.md`「最重要ルール」）。

## 関連ファイル・シンボル（grep で実在確認済み）

- `snotra-egui-runtime/src/input.rs`
  - `on_keyboard_event`（:350）— `typed_text_event(pressed, event.text_with_all_modifiers())`（:411）が変異 1 の注入点
  - `typed_text_event`（:454）・`committed_text_event` → `is_printable_char`（制御文字を弾く）
  - Ctrl+C / X / V は `self.modifiers.command` のとき `Copy` / `Cut` / `Paste` を積んで **return**（文字経路へ届かない）
  - 単体テスト :742-743 のコメントが「呼び出し点の取り違えはこのテストに映らない」と自認
- `snotra-egui-runtime/CLAUDE.md`「一般」— 通常文字の経路の不変条件（`KeyEvent.text` を使わない／`admit_key` の後）
- `src-tauri/src/egui_shell/launcher_controller/search_flow.rs` `on_input_changed`（:228）— trace `egui_input:changed` に `before_chars` / `after_chars` / `appended_at_end` を出す。**TextEdit が `changed()` を返したフレームでだけ出る**
- `scripts/smoke-egui.ps1`
  - シナリオ 1: hotkey → `egui_show:done` → 1 文字クエリ `z` → `egui_results:show` → パスクエリ `c:\`（:377-416・`after_chars -eq 3` を `Wait-SnotraTraceCondition` で観測）→ Escape → hide 系 → H1 等の不変条件判定
  - パスクエリブロックのゲートは `$failures.Count -eq 0 -and $resultsChecked`
- `scripts/lib/SnotraSmoke.psm1` — `Send-SnotraKey` / `Send-SnotraKeyChord`（押下順に押し逆順に離す・既定 50ms 間隔）/ `Wait-SnotraTraceCondition`（`MinMatchCount`・**一致の最後の 1 件を返す**）/ `Get-SnotraTraceEventCount` / `Read-SnotraTraceSnapshot`
- `.github/workflows/smoke.yml` — `scripts/smoke-egui.ps1` と `snotra-egui-runtime/**` は PR の paths に入っている（変更 PR で smoke が自動起動）

## egui 0.36.2 が Windows で Ctrl+英字に持つ束縛（`~/.cargo/registry/.../egui-0.36.2`）

- `text_selection/cursor_range.rs:116` Ctrl+A = 全選択
- `widgets/text_edit/builder.rs:1197-1214` Ctrl+Y / Ctrl+Z = redo / undo
- 同 :1431-1446 Ctrl+H / K / U / W = 削除系
- 同 cursor_range.rs:150 の P/N/B/F/A/E は `OperatingSystem::Mac` 限定
- Snotra 本体（`src-tauri/src/egui_shell`）は Ctrl+英字を一切束縛していない（`Key::` の使用は矢印・Enter・Escape のみ）

→ **束縛の無い英字（例: G）** を使えば、変異 1 のときの症状は純粋に「`g` が 1 文字入る」になる。

## 技術的制約・設計上の所見

- **issue の「文字数が増えないことを断言」は Ctrl+A だと変異を取り逃しうる**: Ctrl+A は全選択を伴うので、変異 1 では `c:\` が `a` に置き換わり 3 → 1 と**減る**。断言は「増えない」ではなく「Ctrl 打鍵で入力が変わらない」の形にする。束縛の無い英字ならこの問題自体が消える
- **不在の断言は沈黙で合格しうる**（`.claude/rules/safety-nets.md`「これまで無意味だった状態に意味を与える…」）。Ctrl 打鍵の後に固定 sleep して「`egui_input:changed` が出ない」を見るだけだと、打鍵が丸ごと落ちても緑になる。**後続の打鍵（Backspace）を肯定的な標識に使う**: Ctrl+G → Backspace と送り、ベースライン以降の最初の `egui_input:changed` が `before_chars=3, after_chars=2` であることを要求する
  - 正常: Ctrl+G は文字を入れず（`text_with_all_modifiers` = `"\x07"` を `is_printable_char` が弾く）、Backspace で 3 → 2 の 1 件だけ
  - 変異 1・別フレーム: 3 → 4（`g`）→ 4 → 3 の 2 件。最初の 1 件が 3 → 4 で不一致
  - 変異 1・同一フレーム: 3 → 3 の 1 件で不一致
  - Backspace が落ちた: 予算切れで赤（偽の緑にはならない）
- `Wait-SnotraTraceCondition` は**一致の最後の 1 件**を返すので、「ベースライン以降の最初の 1 件」を見るには待ちの後に `Read-SnotraTraceSnapshot` から `seq` で切り出す必要がある（`Test-SnotraNoTraceEventInWindow` と同じ考え方）
- ベースラインは打鍵の**前**に取る（`Get-SnotraTraceEventCount` のコメント・#755/#801 是正 A）
- 配置はパスクエリブロックの後（入力が `c:\` の 3 文字であることが `$pathTyped` で確定している唯一の地点）。その後の Escape → hide → 不変条件判定は入力内容に依存しない
- 変異 2（`admit_key` の前へ配送）は smoke の hotkey が Alt を離してから show するため押下中キーが残らず、この形では捕まらない（issue 本文のとおり・コード上も show 前に全キーが解放される）

## 再利用できる既存パターン

- パスクエリブロック（:384-416）の「打鍵 → `Wait-SnotraTraceCondition` で `egui_input:changed` の述語を観測 → `$failures` へ積む」
- シナリオ 2 の「打鍵前にベースラインを数え、`seq` で区間を切る」（:595-603・:663-673）

## 未解決の疑問

- 変異 1 のビルドで新しい検査が実際に赤になるか（**未測**・`/implement` の委譲先が worktree で測る）
- Ctrl+G の `WM_CHAR` が `0x07` として届き `text_with_all_modifiers` が `"\x07"` を返すこと（tao の `KeyEvent` 組み立て）は推論。変異注入で `g` が入ることを観測すれば裏が取れる
- 変異 2 を捕まえるシナリオ（キーを押したまま focus を移す・#927 の再現形）を足すか——issue は「別に判断する」としている
- 隣接する古い記述: `scripts/smoke-egui.ps1:379` は「H6 が判定する」と書くが、`SnotraTraceInvariants.psm1` に H6 は無い（導入コミット `925ef3a`〔#1030〕でもコメントにだけ現れる）。新ブロックの直前の節なので「古い情報を残さない」の射程に入る

## 敵対的調査（3b）の反映 — `workspace/adversarial-1268.txt`

壊せなかった項目: 争点 1（tao 0.37.1 `keyboard.rs` の WM_CHAR arm と `finalize` で、Ctrl 併用時 `text_with_all_modifiers` = WM_CHAR そのもの・`text` = Ctrl を外した字）／争点 2（egui に G の束縛なし・Ctrl+A は `delete_selected` してから挿入するので減る）／`changed()` は正味ではなく変異の有無で真／H6 は計画で取り下げ済み／H1/H4/H5/H7 を壊す道筋なし／`-Trace` で trace が出る・入力欄は毎フレーム focus を取り戻す・CI paths は smoke を起動する。

| 所見 | 採否 | 理由・反映 |
|---|---|---|
| B1: Ctrl+G が丸ごと落ちると Backspace だけで 3→2 になり、変異 1 でも緑 | **採る（最重要）** | 設計を差し替える。`push_key` trace で到達を証明する案は採らない——Ctrl を「効果を持つ打鍵」にすれば打鍵自体が到達の証拠になる。**Ctrl+H**（egui 0.36.2 `builder.rs:1431` が Windows でも 1 字削除に束縛）を使う: 正常は 3→2 の 1 件。変異 1 は同じ `on_keyboard_event` 呼び出しが `Key(H, ctrl)` と `Ime(Commit("h"))`（`committed_text_event`）を**同じ RawInput に**積む（`input.rs` :399-412）ので同一フレームで処理され（`builder.rs:1109` の events ループが順に適用）、削除してから `h` を入れて 3→3。丸ごと脱落は観測なしで赤。Ctrl だけ脱落して H が届くと 3→4 で赤（偽の緑ではなく騒がしい赤） |
| B2: 変異 2 は「原理的に」ではなく「通常は」捕まらない | 採る | 文言を弱める（Alt 解放が ShowAfterAltRelease の 350ms を超えて遅れると押下中キーが残りうる・未測） |
| B3: 3 文字確定の地点は唯一ではない | 採る（文言のみ） | 配置は据え置く。1 文字クエリの直後に置くと、パスクエリブロックの「先行の 1 文字クエリを Backspace で消す」前提が変わるため |
| B4: 「最初の 1 件」は過剰 | 一部採る | 「`seq` > ベースライン の `egui_input:changed` を待ち、区間内の全件が 1 件かつ 3→2」で判定する（1 件と決め打ちするのは、想定外の追加変化も赤にするため） |
| B5 ⚠️: "c:" で results が hide に倒れ得る | 影響なしと裁定 | 既存フローでも `z` → Backspace で空になり `egui_results:hide` は Escape より前に一度出ている。Escape 後の hide 待ちが presence 検査であることは既存の性質で、新たに弱めない。H1 が orphan を見る |
| B6: H6 の主張全体が今は何も判定していない | 採る | :379-380 の文を、判定は無い形へ直す |
| B7: ゲートに `$pathTyped` 非 null が要る | 採る | ゲートは `$failures.Count -eq 0 -and $null -ne $pathTyped` |
| B8: ローカルで前面でないと別アプリへ飛ぶ | 受容 | 既存の打鍵と同じ前提・`$failures` ゲートで既存と同等 |
| C1 ⚠️ WM_CHAR が実機で制御文字として届く | 未測 | 変異注入で `h` が入ること（3→3）の観測が裏取りになる |
| C2 ⚠️ IME ON で Ctrl が奪われる | 受容 | 既存の `z` 打鍵が同じ前提で通っている。CI runner は IME なし |
| C5 ⚠️ 変異ビルドで実際に赤になるか | 未確定欄へ | `/implement` の委譲先が worktree で測る |
