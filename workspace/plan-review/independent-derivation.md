# 独立再導出 — #743（SPEC §6.1 as-built 明文化 + 純粋核テスト）

**枠組み**: issue の物語（「← が効かない」）を捨て、**「候補行の集合が置き換わる経路」を状態機械の側から全列挙し、各経路で `selected` がどう決まるかをコードで確定する**という軸で導出した。issue の症状記述は実機トレースで否定済みのため、症状からの逆算は使っていない。

**読んだもの**: issue #743 / `SPEC.md`（全文の見出し + §4.5〜§4.8・§5・§6・§8.6・§15.4・§18.4〜§18.5・§19.7）/ `src-tauri/src/egui_shell/search_state.rs`（全文）/ `launcher_controller.rs`（`on_nav_keys` / `run_search_with` / `poll_async` / `spawn_folder_load` / `on_escape_pressed`）/ `results_view.rs`（scroll gate）/ `layout.rs`（`present_results` / `clamp_results_height`）/ `snotra-core/src/folder.rs`・`engine.rs`（`FolderListContext` / `filter_sorted` / `error_result`）/ `AGENTS.md` / ルート・`src-tauri` の `CLAUDE.md` / `docs/development-principles.md`（「列挙の完全性」節を含む全文）/ `.claude/rules/spec.md`・`src-tauri.md` / `docs/build-commands.md`（見出し）。
**読んでいないもの**（指示どおり）: `workspace/plan.md` / `research.md` / `plan-snapshot.md` / `plan-review/` の他ファイル。

---

## 1. コードで確定した as-built（すべて追跡済み・推測なし）

### 1.1 `selected` の代入点（全数・`search_state.rs`）

証跡: `grep -c "self\.selected = " src-tauri/src/egui_shell/search_state.rs` → **11 代入文**、担い手は **10 メソッド**（`move_selection` だけが 2 箇所書く）。SSOT は存在せず、各メソッドが独立に 0 を書く。

| メソッド | 値 | 本件との関係 |
|---|---|---|
| `enter_folder` | `0` | **対象**（通常検索の ← / → からの突入） |
| `navigate_folder` | `0` | **対象**（展開中の ← / → ） |
| `set_folder_filter` | `0` | 隣接（§6.3・フォルダ内フィルタ打鍵） |
| `reset_selection` | `0` | 隣接（通常検索の毎打鍵・SPEC 未記載） |
| `enter_tool` | `0` | 別軸（§18.4） |
| `reset` | `0` | 別軸（show 時） |
| `set_results` | `clamp_selected(len, selected)` | **上の 0 を潰さない**（min クランプ） |
| `on_escape` tool 分岐 | `clamp_selected(len, restore_selected)` | **例外**（§18.4 復元） |
| `on_escape` folder 分岐 | `clamp_selected(len, restore_selected)` | **例外**（§6.4 復元） |
| `move_selection`（2 箇所） | 空なら `0` / 飽和クランプ | ↑↓ |

→ **「候補が置き換わったら先頭」は全称として偽である。** `on_escape` の 2 分岐が反例で、既存テスト `escape_folder_restores_then_hides`（`selected()==2` を assert）がそれを固定している。SPEC の文は**前進方向の置換（→ 深掘り・展開中の ←・通常検索からの ← 突入）に限定して書く**しかない（`AGENTS.md`「全称表現は前提条件とセットで書く」）。

### 1.2 ← の 1 打鍵で起きること（時系列・2 段）

`launcher_controller.rs::on_nav_keys` の `ViewKind::Folder` 分岐:

1. **押下フレーム**: `parent_dir()`（= `compute_parent_dir(frame.current_dir)`）が `Some` なら `navigate_folder(parent)` → `current_dir` 書き換え・`folder_filter` クリア・**`selected = 0`**・`folder_gen += 1`。続けて `folder_cache = None` / `folder_error = None` にし `spawn_folder_load` を投げる。**`self.results` はこのフレームでは触らない**（`rows_generation` は進まない）。
2. **到着フレーム**: `poll_async` が `accept_folder_result(token)` を通った最新 msg だけ適用 → `folder_cache` or `folder_error` を立てて `run_search()` → `set_results(...)` で行が差し替わり `rows_generation += 1`、`selected` は `clamp_selected(len, 0)` = **0**。

**帰結（as-built の要点）**: 選択のリセットは**即時**、行の差し替えは**到着後**。1 と 2 の間、画面には**直前のフォルダの行が残ったまま選択ハイライトだけが先頭へ動く**。dead/slow UNC ではこの窓が長い。issue が報告した「階層は変わらず選択だけが動く」という見え方は、**この過渡状態と完全に同型**である（実機トレースでは列挙が速く受理されたため 2 まで進んだ）。

さらに `results_view.rs` を辿ると: 押下フレームは `snapshot.generation` が不変で `selected` だけ変わる → `generation_changed == false`・`do_scroll == true` → `scroll_directive` が**アニメーション付き**スクロールを出す。到着フレームは世代交代ゆえ**瞬時**スクロール（#714）。つまり過渡状態は「古いリストの先頭へヌルッと動く」として観測される。

### 1.3 ルート到達時

`compute_parent_dir` が `None`（`C:\` / `\\srv\share`）→ `if let Some(parent)` が成立せず**何も呼ばれない**。`folder_gen` も `selected` も `folder_filter` も不変。SPEC の「無反応」は字義どおり（選択位置も動かない）。

---

## 2. 深掘り A — 空フォルダ（列挙 0 件）と列挙失敗（§6.6）の区別

### 2.1 コード上の分岐点

分岐は `spawn_folder_load` の `ctx.read_dir_entries(...)` の `Result` **1 か所だけ**である。

- **`Err`（`std::fs::read_dir` が失敗＝アクセス拒否・不在・切断）** → `folder::error_result(dir)` = `[SearchResult { name: "", path: dir, is_folder: false, is_error: true }]` を `FolderMsg::Failed` で送信 → `folder_error = Some(1 行)` / `folder_cache = None` → `run_search` の Folder 分岐は **`self.state.set_results(err.clone())`**。
- **`Ok(vec![])`（読めたがエントリ 0・フィルタ 0 件・隠しファイル除外で 0）** → `finalize_folder_list_unlimited([])` = `[]` → `FolderMsg::Loaded` → `folder_cache = Some((ctx, []))` / `folder_error = None` → `run_search` は `ctx.filter_sorted(&[], filter)` = `[]` → `set_results(vec![])`。

### 2.2 観測できる差（コードから導出・全数）

| | 空フォルダ（0 件） | 列挙失敗（§6.6） |
|---|---|---|
| 行数 | 0 | 1（エラー行） |
| `results` 窓 | **出ない**（`present_results` が `result_count==0` → `results_window_height==0` → `Hidden`） | 出る |
| `selected` | 0（**指す行は無い**） | 0（エラー行を指す） |
| `view_kind` | `Folder` のまま | `Folder` のまま |
| `folder_load_pending` | `false`（cache 到着済み） | `false`（error 到着済み） |
| Enter | 対象行なし（`results().get(0)` が `None`） | `is_error` ガードで無効（§6.6 既述） |
| → | 対象行なしで無反応 | `is_error` ガードで無反応 |
| ← | **有効**（`parent_dir` は `current_dir` 由来で行に依らない） | **有効**（同上） |
| Escape | §6.4 どおり復帰 | §6.4 どおり復帰 |
| フォルダ内フィルタ打鍵 | 適用される（`filter_sorted`・結果は 0 のまま） | **適用されない**（`set_results(err.clone())` は filter を通さない） |

### 2.3 SPEC にどう書けるか / 書けないか

- **書ける**: 「0 件のときも選択位置は先頭（= 0）だが、それが指す行は無い」。これは選択規則の文が 0 件で無意味にならないために**必要**な但し書きである。
- **書ける**: 「フォルダ展開モードは維持され、← による上昇は候補の中身に依らない」。これは「行から親を導く」誤読（通常検索モードの ← は行の `path` から導くので、混同が起きやすい）を塞ぐ。
- **書くべきでない（写しになる）**: 「0 件なら `results` 窓が出ない」を §6.1 に書くこと。正本は §4.5（0 件は高さ 0）と §4.7（空なら非表示）で、**参照で足りる**（`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」）。
- **書けない（今回の範囲では）**: 「空フォルダでは窓が消えるので、フォルダ展開モードにいる手がかりが画面に無い」という**受容残余**。これは「フォルダ展開中に現在フォルダを UI に出す」という範囲外項目そのものであり、対応する issue が**まだ存在しない**（§7 参照）。番号のない参照は書けないため、SPEC には入れず本書に残す。
- **§6.6 側は触らない**。上表の差は「§6.6 が既に書いていること（エラー行 1 行・Enter 無効・右/左/Escape 有効）」の外側には出ず、新たに書くと §6.6 の写しになる。ただし**フィルタ非適用**だけは §6.3「文字入力時は現在フォルダ内で絞り込み」を**エラー行表示中に限って偽にしている**（隣接の過剰主張・§7 に記録、本タスクでは直さない）。

---

## 3. 深掘り B — §6.1 に書く語（「選択位置」は使えるか）

`SPEC.md` 全文で「位置」を含む語を grep（10 箇所）した結果:

証跡: `grep -c "位置" SPEC.md` → 10 行。

| 語 | 出現 | 指すもの |
|---|---|---|
| **選択位置** | §6.4「フォルダ展開からの復帰」**のみ** | Escape で復元される `selected`（＝本件と同一概念） |
| 編集位置 | §4.8「マウス操作」 | 入力欄のテキストキャレット |
| ウィンドウ位置 / 移動位置 / 保存位置 | §8.2「ウィンドウ位置」群・§8.5 | 窓の座標 |
| 任意位置 | §4.1「検索方式」 | 部分一致の一致箇所 |
| 縦位置補正 | §11「as-built」 | フォント |
| `{path}` の位置 | §18.2「設定構造」 | 引数展開 |

**判定**: 概念の衝突は**無い**。「選択位置」は SPEC 内で選択インデックスを指す**唯一の語**であり、他の「位置」はすべて修飾語で限定されている。新語（「選択インデックス」「選択カーソル」等）は造語になるうえ、「カーソル」は §6.1 が既に「左カーソルキー」でキー名として使っており 3 つ目の意味を作る。

**残る危険は語ではなく隣接**である: §6.4 の「選択位置（復元される）」と §6.1 の「選択位置（先頭へ戻る）」が並ぶと矛盾に読める。**§1.1 の限定（Escape 復帰は適用外）を文中に明示すれば解消する**。関連語の使い分けも既存に従う——項目そのものは「選択中アイテム」（§6.1 既出）、行の描画は「選択行」（§7.2「選択行色」）。

---

## 4. 深掘り C — テストの本数と粒度

### 4.1 既存カバレッジ（重複を作らないための数え上げ）

証跡: `grep -c "#\[test\]" src-tauri/src/egui_shell/search_state.rs` → **42 本**（うち `grep -c "fn rows_generation_"` → **8 本**が世代群）。

| 既存テスト | 押さえている範囲 | 本件での扱い |
|---|---|---|
| `parent_dir_drive_and_unc_roots` | **自由関数** `compute_parent_dir` のルート終端 | 再実装しない |
| `navigate_folder_bumps_gen_and_clears_filter` | 1 ホップ・gen・filter クリア | **選択も current_dir の上昇も見ていない** |
| `enter_folder_saves_view_and_switches_kind` | frame 退避・kind・filter・gen・query | `move_selection(1)` しておきながら **`selected()` を assert していない**（未固定） |
| `set_results_clamps_selection` / `tmp_probe_zero_arrival_after_nonzero_selection` | 到着側のクランプ・0 件到着 | 再実装しない |
| `escape_folder_restores_then_hides` | §6.4 の復元（`selected()==2`） | **§6.1 の例外の錨**として引用する（書き直さない） |
| `rows_generation_*`（8 本） | 行差し替え世代の両方向 | `navigate_folder` は**この群に名前が無い** |

### 4.2 推奨: 新規 4 本（+ 任意 1 行）

各テストは**名前だけで中身が言い当てられる**こと、**名前に含まれないものを assert しない**ことを条件にした。

1. **`navigate_folder_climbs_one_level_each_time`**
   `enter_folder("C:\\Toolbox\\ghost-launcher")` → `parent_dir()` → `navigate_folder` を **2 回連続**。`folder_current_dir()` が `C:\Toolbox` → `C:\` と 1 段ずつ上がることだけを assert する。**issue が主張した症状（2 回目で同じ親に留まる）の直接の回帰**。filter クリアと token 単調性は既存 `navigate_folder_bumps_gen_and_clears_filter` の担当ゆえ入れない（名前に無いものを assert しない）。
2. **`parent_dir_requires_folder_frame_and_stops_at_roots`**
   メソッド経由の 3 ケース: `folder == None` で `None`（通常検索モードで ← が別分岐へ落ちる根拠）、`C:\` で `None`、`\\srv\share` で `None`。自由関数版の既存テストとは**入力の型が違う**（frame 経由）ので重複ではない。
3. **`forward_folder_navigation_selects_top_row`**
   `move_selection` で非 0 にしてから `enter_folder` / `navigate_folder` を各々呼び、`selected()==0`。§6.1 as-built の本体。**名前を「forward」とするのは、`on_escape` の復元がこの規則の外にあることを名前の側から明示するため**（クラス名で括る）。
4. **`parent_dir_is_independent_of_row_content`**
   `navigate_folder` 後に (a) `set_results(vec![])`（空フォルダ）、(b) `set_results(vec![error_row])`（§6.6）の 2 ケースで、`view_kind()==Folder` かつ `parent_dir()==Some(..)` かつ `selected()==0`。**「行が無くても・エラー行でも ← で上がれる」＝上昇は `current_dir` 由来**を固定する。

**任意 1 行**: `rows_generation_is_stable_on_selection_change` 群に `navigate_folder` を名指しで加える（`s.navigate_folder(...)` 後に世代が不変であること）。群の doc は「`self.results` へ代入するメソッド」を母集団と宣言しており `navigate_folder` は母集団外だが、`development-principles.md` §8「全称条件だけの検査は集合が縮んだときに空振りする。守りたい要素は名指しする」に従うなら名指しする価値がある（この不変が破れると ← の押下フレームが世代交代扱いになり、#714 のアニメーション要件が壊れる）。

**採らなかった案**:
- 非同期 2 段（押下時は旧行のまま）専用テスト: `set_results` を呼ばなければ `results()` が変わらないのは `navigate_folder` の実装を読めば自明で、**テストは「driver が到着まで `set_results` を呼ばない」ことを何も証明しない**（driver は純粋核の外）。テストではなく SPEC 文と本書で扱う。
- `set_folder_filter` の選択リセット: §6.3 の話であり、§6.1 の明文化を §6.3 まで広げる判断が先。広げるなら 3. に 1 assert を足すのではなく `folder_filter_input_selects_top_row` を別に立てる（名前と中身の一致）。

### 4.3 **落とし穴と受容残余（省略不可）**

- **`navigate_folder` は `folder == None` でも黙って通る**——`current_dir` の書き換えだけが no-op で、filter クリア・`selected = 0`・`folder_gen += 1` は実行される。`enter_folder` を忘れたテストは**緑になりながら何も証明しない**。全テストで先に `enter_folder` すること。
- **駆動側（`←` → 純粋核）の結合は純粋核テストでは固定できない。** `launcher_controller.rs` に `#[cfg(test)]` は無い（証跡: `egui_shell/` の 14 ファイル中 11 がテストモジュールを持ち、持たない 3 つは `launcher_controller.rs` / `mod.rs` / `results_window.rs`）。`on_nav_keys` は `app_handle` と `egui::Context` を要求する。テスト側に `if let Some(p) = parent_dir() { navigate_folder(p) }` を書き写すヘルパを置くと、**検証されるのは自分で書いた driver の複製**であって driver ではない（false green）。取りうる道は 2 つ:
  - **(a) 一次側だけ固定し、残余を明記する**: 「← が `parent_dir` + `navigate_folder` を呼ぶという結合は、実機の協働トレース計測（← 7 打鍵・`C:\Toolbox\ghost-launcher` → `C:\Toolbox`・行数 1 → 133）で確認済みであり、自動回帰は無い」と書く。追加の検査トリガーを引かない。
  - **(b) 階梯を 1 段上げる**: `SearchState::navigate_to_parent(&mut self) -> Option<u64>`（`parent_dir` + `navigate_folder` の合成）を新設し、**同じ変更で** driver の呼び出し点を移す。合成が純粋核テストで直接固定でき、`-D warnings` 下で新 API を使わなければ `dead_code` で落ちる（移行漏れ検出器）。旧経路を残すと導出が 2 箇所になるので必ず 1 タスクに束ねる。`AGENTS.md`「関数・型を新規定義」トリガーが発火し `/dry-check` + 呼び出し元 grep が要る。
  - **推奨は (a)**。issue の前提は実測で否定されており、結合に現存する欠陥の証拠は無い。(b) は「回帰から守る」という受け入れ条件 3 に対しては上位だが、症状の無い箇所に API を 1 本増やす（YAGNI）。**どちらを採るにせよ、受容残余の 1 行を計画に残すこと**——残さないと「← を回帰から守った」という主張が実際のカバレッジより強くなる。

---

## 5. 変更集合（ファイル + シンボル）

| # | ファイル | シンボル / 位置 | 変更 |
|---|---|---|---|
| 1 | `SPEC.md` | **§6.1「基本動作」**の末尾に追記 | 前進方向の置換後の選択位置を as-built で明文化。**§6.x の新設はしない**（新設すると以降の子番号・`## 7.` 以降の参照整合の確認が要る・`.claude/rules/spec.md`） |
| 2 | `src-tauri/src/egui_shell/search_state.rs` | `mod tests` に 4 本追加（§4.2）。既存 `rows_generation_*` 群へ任意 1 行 | テストのみ。**製品コードは変更しない**（受け入れ条件 1 は既に満たされている） |
| 3 | （(b) を採る場合のみ） | `SearchState::navigate_to_parent` 新設 + `launcher_controller.rs::on_nav_keys` の `ViewKind::Folder` 分岐を移行 | §4.3 参照 |

**変更しないと明示すべきもの**（「変更なし」の根拠を列挙で裏付ける）:
- `launcher_controller.rs`（受け入れ条件 1 は実測で満たされている・#714 の変更も無関係と issue 自身が切り分け済み）
- `SPEC.md` §6.2（ルート終端）/ §6.4（復帰）/ §6.6（列挙失敗）/ §8.6（遷移図）——いずれも本件で**偽にならない**。§6.4 と §6.6 は §6.1 の新文が参照するだけ。
- `snotra-core/`（folder 列挙・ソートの挙動は本件の対象外）

### 5.1 SPEC §6.1 の文案（案 1・settled-state に限定）

```
- 左カーソルキー:
  - 展開中: 親フォルダ内容で候補を置換（ルート到達時は無反応——候補・選択位置とも変化しない）
  - 通常検索モード: 選択中アイテムの親ディレクトリを展開してフォルダ展開モードに遷移
- 候補を置換したあとの選択位置は、列挙結果が反映された時点で先頭行である（as-built・#743）。
  対象は上記 3 経路（→ の展開・展開中の ← ・通常検索モードの ←）で、`Escape` による復帰は
  置換ではなく復元であり、この規則の適用外である（§6.4）。列挙結果が 0 件のときも選択位置は
  先頭のままだが、それが指す行は無い（候補が空のときの表示は §4.5・§4.7）。
```

**既存行への追記（「候補・選択位置とも変化しない」）は削除ではなく新規記述であり、測ってから書く**（`development-principles.md`「撤去（消す変更）の作法」）。実測済み: ルートでは `compute_parent_dir` が `None` を返し `on_nav_keys` の `if let Some(parent)` が成立しないため、`navigate_folder` が呼ばれず `folder_gen` / `selected` / `folder_filter` のいずれも変化しない（本書 §1.3）。

### 5.2 文案（案 2・過渡状態も書く場合）

案 1 に次の 1 文を足す。**足すなら「選択は即時・行は到着後」と「その窓の残りの規律の正本」を必ず対で書く**（半端に書くと「その窓では他に何が成り立つのか」を宙に浮かせる）:

```
  選択位置が先頭へ戻るのはキー押下の時点で、候補行の差し替えは列挙結果の到着時である
  （到着までは直前のフォルダの行が残る。この窓での起動抑止は search_state.rs の
  `folder_load_pending` を正本とする）。
```

**判定基準**: 「dead/slow UNC 共有で ← を押したとき、この文は真か」。案 1 は真（settled state しか主張しない）。「置換後は先頭行」とだけ書く案は**偽**（ロード中は古いリストの先頭を指している）。案 2 は真で情報量が多いが、§6 に非同期列挙という概念を初めて持ち込む。**推奨は案 1**（issue の受け入れ条件 2 は「展開後の選択位置の仕様」であり、過渡状態は要求されていない）。ただし案 2 の過渡状態は**本書 §1.2 の内容として計画に残す**——issue の誤診断がここから生まれた可能性が高く、次に同じ報告が来たときの一次資料になる。

---

## 6. 見落とされやすい間接参照

1. **§8.6 の遷移図が既に ← を持っている**（`FolderExpansionMode --> FolderExpansionMode: ArrowLeft [parent exists]`）。§6.1 に**遷移を再記述してはならない**——書けるのは選択位置だけ。遷移図側は変更不要（新しい辺も新しいガードも生じない）。
2. **←/→ を無効化する条件が §18.5 と §19.7 に散在している**（ツール選択中・インスタントコマンドモード中）。§6.1 に「常に」「すべての置換で」と書くと**書いた瞬間に偽**になる。案 1 は「上記 3 経路」と限定してこれを避けている。
3. **`§6.4` の「選択位置」との隣接**（→ §3）。§6.1 に例外の明示が無いと、2 つの節が矛盾して読める。
4. **`§6.3` の「文字入力時は現在フォルダ内で絞り込み」は、エラー行表示中だけ偽である**（`run_search` の Folder 分岐が `folder_error` を filter 非適用で `set_results` する）。本件の範囲外だが、§6.1 の新文が §6.3 の隣に立つため、レビューで指摘されうる。**直さず記録**する。
5. **`effective_result_limit()` と §4.5 の「最大表示件数」は別物**。フォルダ列挙は `FolderListContext.max_results = effective_result_limit()` で `filter_sorted` の `take(max)` に掛かり、§4.5 の最大表示件数は**窓の高さ**を決める（実測の 133 行がこの差を示している）。SPEC の新文で件数に言及しないこと（言及すると 2 つを混同する）。
6. **`rows_generation` と scroll の関係**（→ §1.2）。← の押下フレームは「選択だけ変わり世代は不変」＝**アニメーション付き**スクロール。`navigate_folder` がうっかり `rows_generation` を進めるよう変更されると #714 の要件が壊れる。任意 1 行のテストはここを守る。
7. **`record_folder_expansion` の非対称**: → は記録し、展開中の ← は記録せず（`navigateFolderUp 相当・Finding #1`）、通常検索からの ← は記録する。§4.4・§5.1 は「フォルダ展開回数」としか書いておらず**どの方向が記録されるかを定めていない**。本件と同じ §6/§5 近傍の as-built ギャップだが、**選択の話とは独立**なので範囲外に置く（記録のみ）。
8. **`docs/architecture.md` の**「フォルダ展開は『開始時スナップショットを保持し、`Escape` で一括復帰』モデル」——§6.4 側の事実の要約。§6.1 に前進方向の規則を足しても**偽にならない**（更新不要）。`snotra-core/CLAUDE.md` のフォルダ記述は列挙・命名の話で無関係。
9. **コード側の doc コメント**: `enter_folder` / `navigate_folder` の rustdoc は「選択を 0 にする」と書いていない（実装行だけがある）。SPEC を intent の正本にするなら追記は不要（`development-principles.md`「ドキュメントとコードの分担」）。ただし**新テストのコメントが SPEC §6.1 を名前で参照する**ようにしておくと、片方だけ動いたときに気づける。
10. **`folder_load_pending` の doc が過渡状態の唯一の正本である**。案 2 を採るなら SPEC からここを指す。採らないなら SPEC は過渡状態に触れないので参照も不要。

---

## 7. 走らせるべき検査（`AGENTS.md`「条件別チェック」表への当たり判定）

### 当たる

| トリガー | 参照先 | 本件での中身 |
|---|---|---|
| Rust ファイル変更 | `docs/build-commands.md` カテゴリ A | `cargo test` / `cargo clippy`。**PostToolUse hook が自動実行**（`*.rs` は `selectChecks` の割り当てあり → 沈黙 = 合格） |
| ガバナンス文書（`*.md`）変更 | カテゴリ F = **`npm run governance:check`** | **手動実行が必須**。`SPEC.md` の編集では PostToolUse が沈黙するが、それは「何も走らなかった」の意味である（ルート `CLAUDE.md` 明記）。SPEC 番号・参照実在はここで初めて検査される |
| `SPEC.md` を読む/触る | `.claude/rules/spec.md`（自動配送） | ①as-built を記述する（実装を確認してから書く＝本書 §1 が実施済み）②セクション番号整合（**§6.1 への追記なら番号は動かない**。新 §6.x を作る場合のみ子番号と `## 7.` 以降を確認） |
| 文書に事実の写しを増やす変更 | 正本を 1 か所に定め他は参照へ | §4.5 / §4.7（空のときの窓）・§6.4（復帰）・§6.6（失敗）・§8.6（遷移）はいずれも正本が別にある。**参照で書く** |
| 対称ペア（→ / ←）を変更 | `/symmetric-check`（弱く当たる） | 変更するのは記述だが、選択規則を ← だけに書くと → 側が宙に浮く。案 1 は 3 経路をまとめて括ることでこれを満たす |
| （(b) を採る場合のみ）関数を新規定義 | 呼び出し元 grep + `/dry-check` | `navigate_to_parent` 新設時。旧経路を残さず 1 タスクに束ねる |

### 当たらない（理由を明示する）

- **`/state-check`**: UI モード・ガード条件を変更しない。§8.6 の遷移辺（`ArrowLeft [parent exists]`）は既存のまま。§6.1 は選択位置しか足さない。
- **`/race-check`**: 非同期の窓を**記述する**だけで、worker spawn・channel・drain・listener・共有状態のいずれも変更しない（`spawn_folder_load` / `poll_async` は無改変）。
- **`/cache-check`**: `folder_cache` の再利用ロジックを変更しない。
- **`/persistence-check`**: 永続形式・キー形式に触れない。
- **カテゴリ C（`smoke:startup` / `smoke:egui`）**: 窓生成・表示順・ホットキー・スラッシュコマンド・IPC ルートのいずれも変えない。
- **カテゴリ D（目視 GUI スモーク）**: スタイル・レイアウト・テキスト表示に影響しない。**← の実機挙動は既に協働トレースで実測済み**であり、これ以上の実機確認は要求されない。
- **面積 ratchet / `AREA_BUDGET`**: 母集団は常時ロード面（`CLAUDE.md` + `AGENTS.md` + skill の `description`）であり、**`SPEC.md` は母集団外**。増分は課税されない。
- **`/plan-review` Step 2b**: 本書がその実施物である。
- **モジュール索引の更新**（ファイル追加/削除トリガー）: ファイルを増減しない。

---

## 8. 計画側で決める必要がある論点（判断は保留し、選択肢と基準だけ置く）

1. **SPEC 文案 1 か 2 か**（過渡状態を書くか）→ 基準は §5.2 の「dead UNC で真か」＋「§6 に非同期概念を持ち込む是非」。推奨は案 1。
2. **§6.3（フォルダ内フィルタ打鍵）まで明文化を広げるか** → 広げるなら SPEC 文と 5 本目のテストを対で足す。狭いままにするなら本書の §1.1 表を計画に引用して「意図的に範囲外」と記録する。
3. **駆動側の結合をどう扱うか** → §4.3 の (a) / (b)。推奨は (a) + 受容残余の明記。
4. **範囲外項目の受け皿**: 「results 窓の高さの仕様変更」「フォルダ展開中に現在フォルダを UI に出す」に対応する **OPEN issue は存在しない**（`gh issue list --state all` の最新は #831、`フォルダ` / `results 窓 高さ` の検索でも #743 / #532 / #738 / #755 / #757 しか出ない）。「別 issue へ切り出し済み」という前提は**現時点の GitHub 上では成立していない**。SPEC に番号を書くと `governance:check` の参照実在検査に対して存在しない参照を作ることになるため、**番号を書かない**か、**先に issue を作る**かのどちらかを決める必要がある。

---

## 9. 記録のみ（本件では直さない隣接の as-built ギャップ）

- §6.3 の絞り込み記述が、列挙失敗のエラー行表示中だけ偽（→ §6-4）。
- §4.4 / §5.1 が「フォルダ展開回数」の記録方向（→ / ← の非対称）を定めていない（→ §6-7）。
- 通常検索モードの毎打鍵 `reset_selection`（選択が先頭へ戻る）が SPEC のどこにも無い（§4 に相当する記述なし）。
- 空フォルダでは `results` 窓ごと消えるため、フォルダ展開モードにいる視覚的手がかりが無い（範囲外項目「現在フォルダを UI に出す」と同根）。
