# research — 束C: 文書・コメントの残骸掃除（#674 + #698）

調査日: 2026-07-26 / ブランチ: `chore/bundle-c-doc-cleanup` / HEAD: `4f5a7ce`

対象 issue: **#674**（SPEC §4.8 の CSS `:hover` / `show_egui_main` の stale 例示 / `hex_color` 重複）、**#698**（`code-reviewer.md` の SolidJS 前提）。
`#674` にはサイクル前段でコメント項目を 1 つ追加済み（**SPEC §20.3 の WebView2 残骸**・[issuecomment-5081413580](https://github.com/finelagusaz/Snotra/issues/674#issuecomment-5081413580)）。

いずれも「文書とコメントが現状と食い違う」類で、**動作は変えない**。

---

## issue の要約

| 項目 | 出所 | 内容 |
|---|---|---|
| 1 | #674 | SPEC §4.8 のホバー記述が CSS `:hover` を根拠にしている |
| 2 | #674 | `show_egui_main` のコメントに stale な「展開高 例 300px」が残る |
| 3 | #674 | `hex_color` の 1 行 wrapper が `view.rs` / `results_view.rs` に重複 |
| 4 | #674 コメント | SPEC §20.3 に WebView2 期の記述（2 行構成 / `--update-toast-height` / `updateInfo`）が残る |
| 5 | #698 | `.claude/agents/code-reviewer.md` の 2 箇所が SolidJS 前提 |

---

## 各項目の現状（実測）

### 項目 3 — **既に解決済み。作業不要**

`fn hex_color` は `src-tauri/` 全体で **0 件**（Grep 実測）。git で経緯が取れる:

| commit | 出来事 |
|---|---|
| `8459023`（#532 SU4 / PR #644） | 導入 |
| `a9cb6ef`（#646 PR2 / PR #669） | `results_view.rs` へ複製（= issue が指す重複） |
| **`22dd61b`（#673 PR B / PR #679）** | **`read_visual` への集約で消滅** |

`results_view.rs` に `hex` を含む識別子は 0 件（`background_hex: None` の 1 件はテスト fixture のフィールド名）。**issue 起票後に別 PR が副作用として片付けた**形。

→ **本 PR では触らず、#674 のクローズ時にその旨を書く。**

### 項目 1 — SPEC §4.8 は**二重に**現状と食い違う

`SPEC.md:190`:

> - ホバー: CSS `:hover` による視覚フィードバックのみ。`selected` 状態は変化しない

食い違いは 2 つある。

1. **CSS が存在しない**（#532 SU7 / PR #662 でフロント撤去）— issue が指摘している側
2. **ホバーの視覚フィードバック自体が存在しない** — issue が指摘していない側

行を描くのは `results_view.rs::draw_result_row`（`:213-319`）ただ 1 つで、視覚の分岐は `selected` のみ:

```rust
let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
if selected {
    ui.painter().rect_filled(rect, 4.0, theme.selection);
    ...
}
```

`response` の用途は `scroll_to_me` と `.clicked()` の 2 つだけで、**`hovered()` はファイル全体で 0 件**（Grep 実測）。`Sense::click()` は当たり判定を作るだけで描画も cursor 変更も伴わない。

設計意図の側には記述がある——`docs/superpowers/specs/2026-07-22-su3-search-experience-design.md:141`:

> **結果行**（§4.8・`ResultRow.tsx` 相当）: …ホバーは視覚のみ（selected 不変）…

つまり **SU3 は「ホバー視覚を保つ」と書いたが、egui 実装には入らなかった**。SPEC の記述が保っていたのは意図であって as-built ではない。

> ⚠️ **要求判断**: 「as-built を書き取る」だけなら「ホバーの視覚フィードバックは無い」と書く。だが SU3 の意図を復活させるなら別の実装タスクになる。→ 下の「未解決の疑問」。

なお `ScrollArea` のフローティングスクロールバーはホバーでフェードイン（egui 内蔵・`scroll_area.rs:1473` の `animate_bool_responsive`）するが、これは**行のホバーではない**。#710 が fps の原因欄で見ているのがこれで、§4.8 の対象外。

### 項目 2 — `show_egui_main` の「例 300px」

`src-tauri/src/egui_shell/mod.rs:373-377`:

> 前回 hide 時に展開高（例 300px）のままだと position クランプが 300px で効き、show 後に view が bar_height へ collapse して視覚スナップ + 位置ずれになる。

**機構は依然有効**（同関数が `bar_height` へ collapse してから `position_on_target_monitor` を呼ぶ順序制約は生きている）。嘘なのは**数値だけ**。

#646 PR2 決定 6 以降、main の高さは `bar_height`(+ `status_height` + `toast_height`) のみで結果件数では伸縮しない（`SPEC.md` §4.7）。既定 43px × 最大 3 行 = **129px** が上限で、300px には至らない。

### 項目 4 — SPEC §20.3 の WebView2 残骸

`SPEC.md:1068-1086`。前半 5 行が WebView2 期の記述、後半 2 ブロックが egui as-built という**二層構造**になっており、前半が後半と矛盾する。

| 行 | 記述 | 現状 |
|---|---|---|
| 高さ = `bar_height` 連動 | 生きている（egui ブロックが同内容を再掲） | 重複 |
| **行1: y = 高さ × 0.25** | **stale** | #700 で 1 行中央揃えへ（`view.rs:1620-1624` に経緯コメント） |
| **行2: y = 高さ × 0.75** | **stale**（`[今すぐ更新]`/`[閉じる]` の右寄せ自体は生存） | 同上・`btn_y = rect.center().y`（`view.rs:1631`） |
| **`--update-toast-height` を加算** | **stale**（CSS 変数は不在） | 「ウィンドウ高さに加算」は egui ブロックが記述済み |
| **`updateInfo` シグナルを null** | **stale**（SolidJS シグナルは不在） | `UpdaterUi` 状態機械・「セッション中恒久」を egui ブロックが記述済み |

as-built の正本は `view.rs:1591-1672`:

- `allocate_exact_size(width, toast_height)` の**単一行**
- ボタンを先に右から詰め（`cursor_x = rect.right() - 8.0`）、`btn_y = rect.center().y`
- 本文は `text_x = rect.left() + 8.0` から、`cursor_x + 8.0` を右境界に `truncate_at_width` で末尾省略、`rect.center().y` で縦中央

**stale な 4 点は削除し、生存部分は既存の egui ブロックへ寄せる**（`AGENTS.md`「条件別チェック」の「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」）。

### 項目 5 — `code-reviewer.md` の SolidJS 2 箇所

`.claude/agents/code-reviewer.md`:

- **`:78`**（Phase 2 / 2d. リソースライフサイクルチェック）
  > クリーンアップが `await` / `.then()` の**前**に同期的に登録されているか（SolidJS: `onCleanup` は同期リアクティブコンテキストで呼ぶ）
- **`:122`**（Phase 3 チェックリスト）
  > **SolidJS 固有**: 不要な再レンダリング、`createMemo` にすべき高コスト計算

差し替え先は issue の対応案どおり:

- `:78` → Rust のリソースライフサイクル（生成/破棄ペア・`Receiver` 所有権 drop・`AtomicBool` の戻し経路・子プロセス `kill`）。**正本は `.claude/rules/src-tauri.md`「この rule が正本」節**（実在確認済み）
- `:122` → egui immediate-mode 固有（毎フレームの確保・`lock()` 回数・1 フレーム 1 回の live-read）。**live-read の正本は `src-tauri/CLAUDE.md`「モジュール構成」の `read_visual` 項**（#673）

**ユーザーの合意取得済み**（ルート `CLAUDE.md` 最重要ルール 2）。

---

## 関連コード・文書（実在確認済み）

| パス | 役割 | 本 PR で触るか |
|---|---|---|
| `SPEC.md:190` | §4.8 ホバー記述 | **触る** |
| `SPEC.md:1068-1086` | §20.3 トースト UI | **触る** |
| `src-tauri/src/egui_shell/mod.rs:373-377` | `show_egui_main` の collapse-before-position コメント | **触る** |
| `.claude/agents/code-reviewer.md:78, :122` | SolidJS 前提の 2 箇所 | **触る** |
| `src-tauri/src/egui_shell/results_view.rs:213-319` | `draw_result_row`（as-built の正本・項目1） | 読むだけ |
| `src-tauri/src/egui_shell/view.rs:1591-1672` | toast 描画（as-built の正本・項目4） | 読むだけ |
| `docs/superpowers/specs/2026-07-22-su3-search-experience-design.md:141` | SU3 のホバー意図（履歴文書） | **触らない**（設計書は当時の記録） |
| `.claude/rules/src-tauri.md` | 項目5 の差し替え先が指す正本 | 読むだけ |

---

## 既存パターン

- **stale な例示の直し方**: `mod.rs:386-388` に先例がある——「行番号参照は挿入でずれるため名前で指す」と明記して名前参照へ倒している。項目 2 も同じ倒し方（生の数値ではなく上限の導出根拠を書く）が取れる
- **二層構造の畳み方**: §11 は #654 でスコープ宣言を先頭に置いて射程を明示した。§20.3 も「WebView2 期の記述 + egui as-built」の二層だが、**両層が同じ対象を語っている**ため、スコープ宣言ではなく**古い層の削除**で解ける

---

## 技術的制約

- **Win32 依存なし・挙動変更なし。** 触る `.rs` はコメント 1 箇所のみ（`mod.rs`）
- **PostToolUse hook**: `mod.rs` の編集で clippy + `src-tauri` のテストが自動実行される（沈黙 = 合格）。`SPEC.md` / `.claude/agents/**` は **`selectChecks` に割り当てが無く沈黙は「何も走らなかった」**（#497・#698 自身が備考で指摘）
- **`governance:check`**: `SPEC.md` はガバナンス文書。カテゴリ F を手動で回す（PR CI の `governance-check` job も常時実行）。面積 ratchet の余白は実測で **常時ロード 100 字 / rules 10 字**——本 PR は `AGENTS.md` にも `.claude/rules/` にも足さないので抵触しない見込みだが、**コミット前に再測する**
- **`.claude/agents/**` の扱いは、検査ごとに違う**（#698 備考の「内容検査対象外」は G11 には当たらない・実測で訂正）:
  - `selectChecks`（PostToolUse hook）には割り当てが無い → **編集しても何も走らない**
  - **G11（見出し参照の実在）は `.claude/agents/` を母集団に含む**（`scripts/governance-check.mjs:719` にその旨が明記）。ゆえに項目 5 の差し替え文で**正準形** `` `<path>.md`「<見出し>」 `` を使えば、参照先の実在は**機械照合される**
  - 着地確認（`collectAnchors` はATX 見出し / 番号付きリスト項目 / 太字リードの 3 種・照合は正規化後の前方一致）:
    - `.claude/rules/src-tauri.md` の `## この rule が正本（CLAUDE.md に無い src-tauri 固有）` → 「この rule が正本」で前方一致・着地する
    - `src-tauri/CLAUDE.md` の `- **テーマ色・font・行高の読みは 1 フレーム 1 回（#673 spec 決定 4）**` → 太字リード・同上

---

## 未解決の疑問（ユーザー確認事項）

**項目 1: SPEC §4.8 のホバーを、どちらの向きで直すか。**

- (A) **as-built を書き取る**: 「ホバーによる視覚フィードバックは無い（描き分けは `selected` のみ）」。#674 の「現状の実装に合わせて書き換える」に忠実。ただし SU3 の意図が落ちた事実が SPEC からも消える
- (B) **as-built を書き取り、意図の欠落を別 issue へ送る**: SPEC は (A) と同じ文にしつつ、「ホバー視覚を戻すか」を新規 issue で起票する。**送り先を名指しできる**（RETROSPECTIVE「受け皿を確認せずに『別の束が拾う』と書いた」の教訓に適合）
- (C) **本 PR でホバー視覚を実装する**: `draw_result_row` に `response.hovered()` の分岐を足す。**挙動変更**であり #674 の「動作には影響しない」というスコープを破る
