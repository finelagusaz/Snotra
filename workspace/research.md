# 調査: #1038 — trailing 発火の直後に Enter を押すと最終クエリでない行で起動する

## issue の要約

`should_flush_on_enter` の第 3 引数が `Debouncer::is_armed()` である。同期実装の頃は
`armed == false` が「最終クエリの結果が反映済み」を含意していたが、#1004 の worker 化で
その含意が壊れた。trailing 発火の直後（同一フレーム内）は必ず「`armed == false` かつ
worker に in-flight あり」になるため、Enter が flush せず、最終クエリでない行を起動しうる。

直し方は第 3 引数を「**最終クエリの結果がまだ反映されていないか**」へ替えること
（`armed || dispatch.pending_seq() != 0`）。

## 一次証拠（実測・grep）

### 1. フレーム順序（issue の主張どおり）

`src-tauri/src/egui_shell/view.rs`:

| 行 | 呼び出し |
|---|---|
| 1077 | `self.controller.drain_search()` |
| 1080 | `self.controller.poll_search_debounce(&ctx)` |
| 1084 | `let post = read_post_widget_input(&ctx)` |
| 1086 | `self.controller.on_enter(post.shift, &ctx)` |

`poll_search_debounce`（`launcher_controller.rs:1292`）は `poll` が真なら `run_search()` を呼ぶ。
`run_search_with` の `Results / Plain`（非空・非 indexing）枝（同 778–805）は
`self.dispatch.issue(...)` で seq を振り worker へ送るだけで、**行を差し替えない**。
`Debouncer::poll`（`layout.rs:459`）は発火時に `self.armed = false` を立てる。
ゆえに `on_enter` に着いた時点で `armed == false` かつ `pending_seq() != 0` が確定する。

### 2. 現在の判定

`search_state.rs:415`:

```rust
pub fn should_flush_on_enter(view_kind: ViewKind, is_plain: bool, armed: bool) -> bool {
    view_kind == ViewKind::Results && is_plain && armed
}
```

呼び出し点は `launcher_controller.rs:1317`（第 3 引数 `self.search_debounce.is_armed()`）**1 箇所のみ**。
`mod.rs:76` で re-export。

### 3. `pending_seq() == 0 ⇔ in-flight なし` は健全

`search_dispatch.rs`:

- `issue` は `self.next_seq += 1` を**先に**行うため最初の seq は 1。0 は sentinel として安全（`pending_seq` は `map_or(0, ..)`）
- `pending` を消す経路は `accept`（seq 一致時のみ `take`）と `invalidate` の 2 つだけ
- worker 送信が `Err`（worker 死亡）の枝（`launcher_controller.rs:790–804`）は `invalidate()` + `set_results(Vec::new())` を撃つ。**送信失敗で pending が残る経路は無い**
- 追い越し（supersede）は `issue` が `pending` を新しい `Some` で上書きするため、pending が 2 つ残ることは無い

### 4. `is_armed()` の消費点（同一パターン全コードパス検索）

壊れた含意は「`armed == false` ⇒ 反映済み」である。母集団は `is_armed()` の呼び出し 3 箇所
（`layout.rs:650/652` はテスト、`results_view.rs` の 2 件はコメント内の引用）。

| # | 位置 | 用途 | 判定 |
|---|---|---|---|
| 1 | `launcher_controller.rs:1299` | `poll_search_debounce` の repaint 再要求 | **影響なし**。「時間経過で解消する不成立」の再要求であり、worker の in-flight は時計と無関係（`src-tauri/CLAUDE.md`「期限を待つ状態（armed）」の規範に照らして、ここへ `pending_seq` を足すと **`request_repaint_after` の永久スピン**になる。worker の完了は worker 自身が `wake_main` する）。**触らない** |
| 2 | `launcher_controller.rs:1320` | `should_flush_on_enter` の第 3 引数 | **本 issue の対象** |
| 3 | `launcher_controller.rs:197` (`is_search_armed`) → `view.rs:1120` の `settled` → `results_view.rs:669` の icon worker ゲート | 連打中は icon を積まない perf 最適化 | **同じ壊れた含意を持つ**が、帰結は「差し替わる直前の行のアイコンを取りに行く」＝**無駄仕事だけ**である（アイコンは path キーのキャッシュゆえ誤表示にならず、`icon_prefetch_range` で viewport に絞られている）。**本 issue の受け入れ条件に含まれず、範囲外の残余として記録する**（#1039 の `is_settled()` が入れば自然に揃う） |

### 5. 文書側の記述（更新要否）

| 文書 | 記述 | 要否 |
|---|---|---|
| `SPEC.md` | flush-on-Enter・trailing 窓・debounce の Enter 例外の記述は**無い**（`rg -n "flush\|Enter\|debounce\|trailing" SPEC.md` で確認。§15.3 のコマンド即実行に「debounce をキャンセル」があるだけ） | **更新不要**。`SPEC.md` 記載のフロー・状態遷移を変えないので「仕様変更」ではなく**バグ修正**（`AGENTS.md` ワークフロー 1 の 2 参照で判定） |
| `docs/architecture.md`「検索フロー」 | mermaid の `opt Enter が trailing 窓（50ms）内に来た Plain（should_flush_on_enter）` と、補足の「**例外は Enter である**——trailing 窓内の Enter は…」 | **更新必要**。条件が「trailing 窓内」から「最終クエリが未反映（trailing 窓内 **または** worker in-flight）」へ広がる |
| `docs/superpowers/specs/2026-08-10-search-worker-design.md` §4.7 | 「flush-on-Enter だけは同期の `engine.search` を残す」 | **更新不要**。`docs/superpowers/README.md` が「内容は各時点の設計書のスナップショット。…**更新されない**」と明記している |
| `search_state.rs` / `launcher_controller.rs` の doc コメント | 「trailing 窓内（打鍵後 50ms 以内）」「armed でなければ flush 不要」 | **更新必要**（実装と同じ差分で） |

## 関連ファイル・シンボル（grep で実在確認済み）

- `src-tauri/src/egui_shell/search_state.rs` — `should_flush_on_enter`（415）、テスト `flush_on_enter_only_for_armed_plain_results`（499）
- `src-tauri/src/egui_shell/search_dispatch.rs` — `SearchDispatch`、`issue` / `accept` / `invalidate` / `pending_seq`（60）
- `src-tauri/src/egui_shell/launcher_controller.rs` — `on_enter`（1311）、`poll_search_debounce`（1292）、`run_search_with`（756）、`drain_search`（865）、`is_search_armed`（196）
- `src-tauri/src/egui_shell/layout.rs` — `Debouncer`（423）、`is_armed`（444）、`poll`（459）
- `src-tauri/src/egui_shell/mod.rs` — re-export（76）
- `src-tauri/src/egui_shell/view.rs` — フレーム順序（1076–1086）、`settled`（1120）
- `docs/architecture.md` —「検索フロー（入力 → 結果表示）」（151–）

## 再利用できる既存パターン

- **純粋核へ述語を置く**: `search_state.rs` / `search_dispatch.rs` / `layout.rs` が既に
  「driver から時刻・状態を注入して判定だけを純粋関数へ出す」形を採っている。合成
  （`armed || pending_seq != 0`）もこの形で書けばユニットテストで固定できる
- **flush 枝の本体（`launcher_controller.rs:1322–1340`）は #631/#1004 の既存コードで、
  今回 1 行も変えない**。空クエリ・indexing 中は `None` → `set_results(Vec::new())` へ落ち、
  `dispatch.invalidate()` は**どちらの枝でも**撃たれる（同 1334 のコメントが正本）

## 技術的制約

1. **`on_enter` にテスト席が無い**。`launcher_controller.rs` に `mod tests` は無く（`rg -n "mod tests"` が 0 件）、
   `on_enter` は `AppHandle` と `AppState`（engine lock）を要求する。受け入れ 2 は
   **受け入れ 1 のユニットテスト + 「flush 枝の本体は未変更」の論証**で担保するのが妥当で、
   そのためにハーネスを新設しない
2. **`should_flush_on_enter` の第 3 引数は既に素の `bool`** ゆえ、`(Results, plain, true)` は
   今でも真を返す。**受け入れ 1 を満たすテスト対象は合成の側**（`armed || pending != 0`）である。
   合成を呼び出し点の式のまま書くとテストできる単位が無くなる
3. **#1 の repaint 再要求へ `pending_seq` を混ぜてはならない**（上表 #4-1）。
   `request_repaint_after(ZERO)` の永久スピンになる
4. **`docs/build-commands.md` カテゴリ A**（fmt / clippy / test）は PostToolUse hook が自動実行する。
   doc コメントを触るので `cargo doc --workspace --no-deps --document-private-items` は**手で走らせる**
   （`.claude/rules/comments.md`）

## 既知の偽陽性（設計上受容する）

**単打鍵バースト**: 1 打鍵 → leading が seq=1 を発行 → 50 ms 以内に結果が届き `accept`（pending クリア）
→ trailing が**同じクエリ**で seq=2 を再発行 → その in-flight 中の Enter が同期 flush を払う。
行は既に最終クエリのものなので**結果は変わらず、費用だけ増える**。issue が
「トレードオフ: Enter が同期 flush を払う頻度が増える」として受容しているものの内側であり、
根治（「その seq が現在の行と同じクエリか」を持つ）は #1039 の来歴（`in_flight: Option<u64>` を
型の内側へ）の領分である。**本 issue では直さない。**

## 未解決の疑問

なし（下記 3b で潰した項目を含め、実装前に決めるべき事項は `plan.md` の「未確定」へ移した）。

## 敵対的調査（3b）の結果

全文は `workspace/adversarial-1038.txt`（general-purpose / sonnet 1 体）。争点 8 件すべてに判定が返り、
**壊せた項目は 0 件**、⚠️ 2 件、追加発見 2 件。

### 壊せなかった項目（8 / 8）

争点 1（フレーム順序）・2（sentinel の健全性）・3（flush 枝の全域性）・4（`is_armed()` 消費点の完全性）・
5（呼び出し点 1 箇所）・6（文書更新要否）・7（受容した偽陽性）・8（repaint 永久スピン）。
いずれも一次証拠（ソース直読・grep・実機 `config.toml`）つきで支持された。

### 採用した所見

| # | 所見 | 採否 | 反映 |
|---|---|---|---|
| A | 争点 2 の**根拠列挙が争点の要求より狭い**（hide/show 跨ぎと panic 経路を research.md 本文が論じていない） | **採用** | 上「3. sentinel の健全性」へ 2 経路を追記（下記） |
| B | 争点 8 の「永久スピン」は⚠️——通常は worker の走査中（40〜95 ms）のバースト浪費で、文字どおり無限になるのは worker 死亡時のみ | **採用（機序は自分で裁定した）** | 下記「worker 死亡時の劣化」へ |
| C | `launcher_controller.rs` に `mod tests` が無いことの実測確認 | **採用**（既存の技術的制約 1 の裏づけ） | 変更なし |
| D | #1039 の `enter_tool` / `on_escape` の dispatch 漏れ 3 経路は `ViewKind::Tool` / フォルダ復帰専用で、`should_flush_on_enter` の `ViewKind::Results` ガードと構造的に直交する | **採用** | 本 issue の範囲外であることの根拠として記録 |
| E | debounce の interval は config 由来でなく `Duration::from_millis(50)` のハードコード（`launcher_controller.rs:149` / `984`）——「50 ms」は環境依存の前提ではない | **採用（自分で再実測した。`rg -n "from_millis\(50\)"` で 2 箇所）** | 前提の確定として記録 |

**採らなかった機序の説明**: B の「worker 死亡は現状到達不能」という但し書きは**採らない**。
`search_worker.rs` の doc（31–37 行）が「**検知は 1 要求ぶん遅れる**——死を招いた要求は応答もクリアも
得ず、次の送信が失敗して初めて現れる」と明記しており、`AppState` 不在は到達しなくても
**engine lock の毒（debug/test）** は到達しうる。到達可能性の一般化は採らず、下の帰結だけを採る。

### 3. への追記（所見 A）

`pending` を消す経路として上に挙げた 2 つに加え、**hide/show をまたぐ経路**も塞がれている:
`launcher_controller.rs:980` の `consume_reset_pending` が `dispatch.invalidate()` を撃つ
（「hide を跨いだ in-flight は show 後の行を汚さない」）。**panic 経路**は release では
`panic = "abort"`（プロセスごと終了）、debug/test では worker が死んで下の劣化モードへ落ちる。

### worker 死亡時の劣化（所見 B の帰結・**本修正に有利**）

worker が要求 N の処理中に死ぬと、`pending` は seq=N のまま残り、次の送信が `Err` になるまで
クリアされない（`search_worker.rs` の doc が正本）。このとき新しい判定 `pending_seq() != 0` は
**真のまま固着する**が、帰結は「Enter が毎回同期 flush を払う」——すなわち
**worker 無しで最終クエリの結果を出す**である。**現行（`armed` だけ）はこの状況で
flush せず古い行を起動し続ける**ため、劣化モードとしても新判定の方が安全側に倒れる。

### repaint 再要求へ混ぜてはならない理由（所見 B で精密化）

trailing 発火後は `armed == false` かつ `elapsed >= interval` ゆえ
`remaining = interval.saturating_sub(elapsed)` は `ZERO` である。ここへ `pending_seq` を混ぜると
`request_repaint_after(ZERO)` を**worker の走査中（実運用点で 40〜95 ms）毎フレーム撃ち続ける**。
worker 死亡時は上記のとおり pending が固着するため**文字どおり無限**になる。
`src-tauri/CLAUDE.md`「期限を待つ状態（armed）」の「再要求してよいのは**時間経過で解消する不成立**
だけである」に正面から触れる。**混ぜない。**
