# 調査: #1077 — `on_enter` と `view.rs` が同一フレーム内の別時点で `indexing` を読む

対象 issue: #1077（`検討:` ラベル・rust）。出所は #1073 の敵対的レビュー（2026-08-13）の ⚠️ 所見。

## issue の要約

`AppState.indexing`（`AtomicBool`・live-read）が同一フレーム内の 2 つの別時点から読まれ、
食い違えば「画面には出ていないが Enter は起動する」あるいはその逆が原理的に成立しうる、という指摘。
**実際に食い違う窓が開くか・その症状は未測定**であることを issue 自身が明記している。

---

## 事実 1: 読み点は 2 つではない。**ソース上の読み点は 4 か所**で、うち 1 つは多数の経路から到達する

issue は「2 つの別時点」と書くが、`egui_shell` が `indexing` を読む**ソース上の点**は
次で全部である。**正本はこの grep であって、以下の散文ではない**
（`git grep -nE "self\.indexing\(\)|controller\.indexing\(\)|read_indexing\(" src-tauri/src/egui_shell/`。
経路の数を散文で数えると、呼び出し点を足すたびにこの節だけが腐る——実際に一度腐らせた。下記「敵対的調査」参照）。

| 読み点（ソース） | 消費者 | フレーム内の位置 |
|---|---|---|
| `launcher_controller.rs:778`（`run_search_with` の Results∧Plain 枝） | 行をクリアするか、worker へ送るか | **`run_search` / `run_search_with` の全呼び出し点**（`:372` `:401` `:418` `:751` `:1031` `:1097` `:1269` `:1282` `:1290` `:1298` `:1310`）から到達する。`view.rs` の `consume_external_pending`(:622) / `poll_async`(:653) / `on_input_changed`(:874) / `poll_search_debounce`(:1080) がそれぞれ経路を持つ |
| `launcher_controller.rs:1340`（`on_enter` の flush 枝） | 同期 `engine.search` を走らせるか、行をクリアするか | `view.rs:1086` |
| `view.rs:920`（`indexing_raw`） | status 行の文言（`overlay_kind`）と窓高（`status_row_present`） | `view.rs:920` |
| `view.rs:1100` | 表示ゲート（`plain_results_hidden` → `present_results` 連言③） | `view.rs:1100` |

（フレーム外にもう 1 つある: `window_coordinator.rs:288`。show 経路が畳む高さの導出用で、
`reset_pending` が同じ show で行を消すため本 issue の族には入らない。）

issue が名指すのは `on_enter` と表示ゲートの対である。`view.rs:920` と `:1100` の対は
#752 F2 が既に一度扱っており（`view.rs:914-919` のコメント: 独立に 2 回読むと status 行を
描いたフレームがバー高だけの `set_size` を撃つ）、**その是正は `:920` の内側での 1 回化に
留まり、`:1100` とは別読みのままである**。

## 事実 2: 表示ゲートと Enter の起動ガードは別の述語である

- 表示: `search_state.rs:722` `plain_results_hidden(view_kind, instant_rows, indexing) = indexing && Results && !instant_rows`
  → `layout.rs:396` `present_results` の連言③ → results 窓の hide
- 起動: `launcher_controller.rs:1355` `if !self.state.results().is_empty() { activate_or_execute(...) }`

**Enter の起動ガードは表示ゲートを一切参照しない。** `indexing` が起動側に効くのは
`on_enter` の flush 枝（`:1340`）の内側だけで、しかもその枝は
`should_flush_on_enter(Results, is_plain, is_unsettled)` が真のときしか走らない。

## 事実 3: settled ∧ indexing で「見えない行を Enter が起動する」窓が開く（race 不要）

`is_unsettled`（`search_state.rs:642`）が偽（＝ debounce 非武装・in-flight 無し・
folder 復帰の stale 無し）の間は flush 枝が走らない。このとき `state.results()` に
前回の検索結果が残っていれば、それが results 窓に**描かれていなくても** Enter は起動する。

その状態が構築できることの根拠:

- **`indexing` が true になっても誰も行をクリアしない。** `indexing-started` の listener は
  `mod.rs:466-477` の `wake_main` だけで（値を運ばない wake）、`consume_external_pending`
  （`launcher_controller.rs:1027-1032`）が反応するのは**完了時に bump される** `index_generation`
  である。SPEC §4.7 も「データと選択は保持——クリアしない」と明記する（`view.rs:1090`）
- **hide→show を挟めば `reset_pending` → `SearchState::reset`（`search_state.rs:525`）が
  `put_rows(Vec::new(), 0)` で消す**ので、窓が可視のまま `indexing` が false→true へ跨ぐ必要がある
- **その跨ぎは `auto_hide_on_focus_lost = false` の設定で確実に作れる。**
  `blur_should_hide`（`lifecycle.rs:95`）は `!focused && grace_elapsed && auto_hide` で、
  auto_hide が偽なら他アプリへフォーカスが移っても main は可視のまま残る。
  その状態で `config.toml` の `IndexInputs` 相当の値を書き換えれば
  `config_watcher.rs:161` が `start_index_build` を kick する
- **この開発機の実 `config.toml`（`%APPDATA%\Snotra\config.toml`・2026-08-11 更新）は
  `auto_hide_on_focus_lost = false` である**（実測）。かつ `[[paths.scan]]` が 4 件（`C:\` を含む）・
  `index.bin` は 17,323,918 バイトで、config の scan を書き換えれば `IndexInputs` 差分が立ち
  ビルドは体感できる長さ走る。**仮定ではなく、この環境の既定の状態で経路が開いている**
- auto_hide が既定（真）でも、drain ループの再 kick（`indexing.rs:80`）が
  「完了 → 世代 bump → 再検索が着地（settled）→ stale 残存で再び indexing=true」を作る経路が在る
  （こちらは build スレッドの stale 次第でタイミング依存）

**#1072 が塞いだのは同じ族の unsettled 側の切片だけである。**
#1072 の PR 本文は「現行は flush せず古い行を起動するが、§4.7 の表示ゲートが構築中の plain 結果を
隠すため、画面に見えていない行が Enter で起動していた。新判定では flush 枝が空クリアへ落ちて
起動しない」と書くが、これは `is_unsettled` が真のときの話であり、**settled 側は今も開いている**。

## 事実 4: issue が名指す race 自体の症状は、事実 3 より軽い

`on_enter` の読み（`:1340`）は表示ゲートの読み（`view.rs:1100`）より**前**にある。

- **false→true**（両者の間に build 開始）: `on_enter` では起動が走る。その行は**前フレームまで
  可視だった**行なので、ユーザーが見て押したものと一致する。表示ゲートで窓が隠れるのは正しい反映。
  加えて `activate_or_execute` → `start_launch` が `set_results(Vec::new())` を撃つため
  （`layout.rs:385-389` の #752 F2 注記）、連言②も同じフレームで偽になる。**実害を特定できない**
- **true→false**（両者の間に build 完了）: `on_enter` で flush 枝が空クリア（unsettled 時）または
  事実 3 の起動（settled 時）。表示ゲートでは「行が空」ゆえ結局隠れる。unsettled 側の症状は
  **Enter が 1 回飲まれる**だけで、次のフレームで世代検知が再検索する。**軽微**
- `view.rs:920` と `:1100` の食い違い: status 行の有無と results 窓の可視が 1 フレームだけ
  食い違う（表示のみ）

**ゆえに、この issue の族で実害が明確なのは「読み点の時刻の問題」ではなく
「Enter の起動ガードが表示ゲートを参照していないこと」（事実 3）である。**
両者は同じ 1 つの修正で閉じうる（下記）。

## 再利用できる既存パターン

- **純粋述語 + 名前つきの合流点**: `plain_results_hidden` / `should_flush_on_enter` /
  `present_results` / `status_row_present` はいずれも自由関数で、`search_state.rs` の
  `mod tests` から直接テストできる。`on_enter` には**テスト席が無い**
  （`launcher_controller.rs` に `mod tests` が無く `AppHandle` と engine lock を要求する。#1072 実測）
  ため、**述語へ切り出すことが唯一の測れる継ぎ目である**
- **1 フレーム 1 回読みの先例**: `view.rs:914-919`（#752 F2 の `indexing_raw`）と
  `visual.rs` の `VisualSnapshot`（`src-tauri/CLAUDE.md`「テーマ色・font・行高の読みは 1 フレーム 1 回」）
- **同じ理由で置かれた既存のガードが 1 つある**: `activate()`（`launcher_controller.rs:222-232`）冒頭の
  `folder_load_pending`（`search_state.rs:64-69`）。doc は「この窓では `results` が展開前ビューの
  残存物なので、driver は起動（Enter/クリック）を抑止する……**不可逆な起動だけを止める**」と書く。
  **#1077 が言う状態と論理的に同型である**（行は残っているが、いま画面に出ている物ではない）。
  純粋述語 + `activate` 冒頭の early return という形もそのまま踏襲できる
- **呼び出し点まで届く検知器の先例**: `indexing.rs:201` `start_index_build_invalidates_the_icon_cache`
  が `include_str!` でソーステキストを母集団に取り、**母集団が空でないことを先に assert してから**
  目的の呼び出しの実在を assert する。`on_enter` にテスト席が無い以上、
  「純粋述語のテスト」だけでは呼び出し点の脱落を捕まえられない（#1085 と同型の穴）ため、この形が要る。
  同 doc が「母集団はソーステキストだけであり呼び出しグラフは辿らない」という死角も明記している
- **変異注入で検知器が発火することまで測る**: #1072 が `|| pending_seq != 0` を外して exit 101 を実測した形

## 技術的制約

- **`consume_external_pending`（`view.rs:622`）を後ろへ動かしてはならず、そこより前へ読みを寄せると
  完了フレームがフリッカーする**（`launcher_controller.rs:1023-1026` が明記）。ゆえに
  「フレーム冒頭で 1 回読んで全消費者へ配る」形は採れない。**`view.rs:920` はそれより後**なので、
  そこから `on_enter` と表示ゲートへ配る形なら順序不変条件を壊さない
- **凍結してよいのは `indexing` だけである。** `view_kind` / `instant_rows` / `result_count` は
  `on_enter` の前後で正当に変わる（#752 F2 の読み点の非対称——③はクリック逆流の消費**前**、
  ②の材料は**後**）。これらまで frame-freeze すると `layout.rs:385-389` の規範に反する
- **`run_search_with:778` の読みは別用途であり、統合対象ではない。** 到達経路のうち
  `consume_external_pending` は上記の順序不変条件を持ち、打鍵 / trailing / 起動完了の各経路は
  **その時点で**判断するのが正しい
- `on_enter` は `&mut self` で状態を進めてから起動する。`#[must_use]` の規約
  （`src-tauri/CLAUDE.md`「処置を返す純粋核の強制」）は、新しく処置を返す関数を足すなら該当する
- SPEC §4.7 は**表示**の規則しか持たない。Enter の起動を表示ゲートへ従わせることが
  「§4.7 の記述に合わせる（バグ）」なのか「記述を変える（仕様変更）」なのかは判断が要る。
  **#1072 の先例は「改善方向であり §4.7 の記述は変えない」**（PR 本文）

## 未解決の疑問（計画の未確定欄へ引き継ぐ）

1. **事実 3 の経路を実機で 1 回踏めるか。** `auto_hide_on_focus_lost = false` + `config.toml` の
   index 入力変更で「窓が可視・行が残る・results 窓が消える・Enter で起動する」まで到達するか
2. **修正の形**: (a) Enter の起動ガードへ表示ゲートを合流させる、
   (b) 加えて `indexing` を `view.rs:920` で 1 回読んで `on_enter` と表示ゲートへ配る、
   (c) 検知器のみ、(d) クローズ
3. **SPEC 同期の要否**（上記の制約の最終項）
4. **`activate_or_execute` の他の入口（クリック逆流・`view.rs:1146`）も同じガードが要るか。**
   クリックは results 窓が可視でなければ発生しないが、`rows_generation` の照合だけが守っている
5. **ガードの置き場所**: `on_enter` の 1 か所か、`activate()` 冒頭（`folder_load_pending` の隣）か。
   後者はクリック逆流も同時に覆うが、`shift_activate` の tool 突入枝（`:559` で `results()` を直接読む）は
   `activate` を通らないため覆えない

---

## 敵対的調査（Step 3b）の結果

母集団は `research.md` の全主張。出力は `workspace/adversarial-1077.txt`。

### 壊せた項目（採用・1 件）

- **事実 1 の数え上げ「読み点は 6 つであり、その全部である」は偽だった。**
  `view.rs:653` の `poll_async` → `drain_launch` → `finish_launch` が 3 分岐すべてで `run_search()` を
  呼び（`launcher_controller.rs:372` / `:401` / `:418`）、そこも `:778` の読みへ到達する。旧表の
  どの行にも現れていなかった。「`run_search` の 3 呼び出し点」という記述も偽で、**実測 11 か所**である
  （`git grep -n "run_search()\|run_search_with("` を自分で実行して確認した）
  - **採否: 採用。** ただし直し方は「6 を 7 に書き換える」ではなく**数え上げをやめる**方へ倒した——
    経路を散文で数えれば呼び出し点が増えるたびにこの節だけが腐る（`AGENTS.md`「数え上げも同じ強さ
    である……数ではなく正本（分岐そのもの）を指す」）。事実 1 の表は**ソース上の読み点 4 か所**を
    正本の grep とともに示す形へ書き換え、到達経路は `run_search` 側の列挙へ委ねた
  - **所見に添えられた「射程」の説明（P2〜P5 の結論は変わらない）は独立に検算した**——この経路が
    増やすのは「クリアするか」の読みであって新しい起動経路ではない、という点は `finish_launch` の
    3 分岐がいずれも `run_search()` の後に起動を行わないことで確かめた。結論は変わらない

### 壊せなかった項目（反証を試みて崩れなかった）

事実 2（起動ガードが表示ゲートを参照しないこと）、事実 3 の構築可能性を支える 4 つの裏取り
（`indexing-started` listener が値を運ばない・`index_generation` は完了時のみ bump・`reset()` が
`put_rows(Vec::new(), 0)` を呼ぶ・`config_watcher` が `!indexing` ゲート無しで kick する）、
事実 4 の 2 方向の症状の非対称と `start_launch` のクリア、#1072 PR 本文の引用の逐語一致、
SPEC §4.7 が表示規則しか持たないこと、`on_enter` にテスト席が無いこと。

**`auto_hide_on_focus_lost = false` が実機の実値であること**も独立に確認された（本調査でも実測済み）。

### ⚠️（確信の持てない所見）の採否

- **クリック逆流（`view.rs:1146`）が `rows_generation` 照合しか持たない** — 未解決の疑問 4 として
  既に立っている。**採用**（計画の未確定欄で潰す）
- **`commands/system.rs:13` / `window.rs:180` は `SeqCst`、`window_coordinator::read_indexing` は
  `Relaxed` で読む** — 実害未測定。**本 issue の範囲外として記録のみ**（別 issue 候補）
- **事実 3・4 の到達可能性は実機で 1 回も踏んでいない** — 反証ではなく、調査自身が「未測定」と
  書いていた事項の再確認。**未確定の疑問 1 のまま**
- **`launcher_controller.rs:1023-1026` の happens-before の論拠が過剰な可能性** — 既存コードの
  コメントであって本調査の主張ではない。**範囲外として記録のみ**
