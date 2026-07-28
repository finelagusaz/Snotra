# ADR-stage3-split-rule-and-module-naming: 段 3 の分割規則は ADR-window-coordinator-split-rule を継承し、新モジュールを `font_stack.rs` と命名する

#666（段 3）で `egui_shell/view.rs`（1869 行）を `launcher_controller.rs` / `view.rs` / `font_stack.rs` の 3 つへ分けた。ここに残すのは、その過程で却下した選択肢である。決定の全文と分類表は `docs/superpowers/specs/2026-07-27-666-launcher-controller-main-view-design.md`。

## 文脈

issue は「責務に応じて分割する」とだけ書き、**モジュール割りを一切指定していなかった**（ADR-window-coordinator-split-rule の「帰結」がそう記録している）。母集団は 68 項目（フィールド 19・メソッド 25・自由関数 6・型/静的 9・`EguiView` impl 2・テスト 7）で、段 1 の 11 関数より桁が 1 つ大きい。段 1 は「線が 5 つの異なる原理で引かれており、衝突時にどちらが勝つか書かれていない」という MECE レビューの指摘で計画が落ちている。

## 決定

1. **規則 R(段 3) を 4 条項で置き、68 項目を例外ゼロで分類してから着手する**。条項 1（守る不変条件が属する層）／条項 2（両層に消費者を持つものは依存の向きが許す側）／条項 3（どちらの向きでも到達できない消費者を持つものは独立モジュール）／**条項 4（ADR-window-coordinator-split-rule の規則 R をそのまま継承する）**
2. **新モジュールは `font.rs` ではなく `font_stack.rs`** とする
3. **`view.rs` は残置し、`main_view.rs` へ改名しない**。型名も `SearchWindowView` のまま
4. **`update()` は 1 関数のまま残す**。動かすのは副作用ではなく入力の読み 4 件だけ
5. `LauncherController` の状態は `pub(super) fn state(&self) -> &SearchState` の**参照 1 本**で view へ通す

## 検討した代替案と却下理由

### 1. 規則 R(段 3) を条項 1〜3 だけで置く（ADR-window-coordinator-split-rule を継承しない）

却下した。条項 1〜3 では**フォント内部 7 項目**（`font_covers_cjk` / `font_definitions` / `resolve_font_family` / `jp_font_bytes` と静的 3 件。いずれも唯一の消費者が `configure_japanese_font`）の行き先が決まらない。条項 1 だけを当てると「描画面層 → `view.rs`」になり、**`configure_japanese_font` だけが `font_stack.rs` へ行く**不合理が生じる。

ADR-window-coordinator-split-rule の規則 R（「移設する項目がその中でしか使わないヘルパーは一緒に運ぶ。複数のモジュールから消費されるものは残す」）を条項 4 として明記すると 7 項目が一意に決まる。**同じ規則で段 1 と段 3 の両方が説明できることを保つ**のが継承の目的である——段ごとに別の規則を立てると、次に線を引く人はどちらを当てるかを選べてしまう。

この欠落はレビューが検出した。**規則を「例外ゼロ」と主張したら、実際に全項目へ当てて検算するまで信じない。**

### 2. 新モジュールを `font.rs` と命名する

却下した。`snotra-settings/src/font.rs` が既に存在する（`git ls-files` 実測）。`governance:check` の G-module-index は **basename 包含方式で wrong-directory 検出を意図的に放棄している**（`scripts/governance-check.mjs` の当該コメント）ため、同じ basename を 2 crate へ置くと、**索引から落ちても別 crate の同名ファイルで検査が満たされる**——この変更に対する唯一の機構的ゲートが両 crate で盲になる。

`font_stack.rs` は責務（user_font 先頭 + jp_font fallback という**スタックの組み立て**）をより正確に言い当ててもいる。

**一般則**: 機構が「意図的に弱めた」と明記している箇所は、その弱さを踏む名前を選ばない。弱化の記録は免罪符ではなく、**踏まない責任がこちら側にあることの通知**である。

### 3. `view.rs` を `main_view.rs` へ改名し、型も `MainView` にする

issue のたたき台は `MainView` と書く。却下した。

`view.rs` はこのモジュールで既に「main 窓の view」を意味する（対の `results_view.rs` があるため曖昧さが無い）。改名すると `.rs` 内の `view.rs` 参照 47 件（`results_view.rs` を指すものを除いて 37 件・実測）と `docs/architecture.md` 4 箇所・`PERFORMANCE.md` 3 箇所が**挙動と無関係に**動く。分割そのものが既にこの母集団の一部を動かすため、改名は**同じ母集団を二度動かす**ことになる。

とくに `snotra-egui-runtime/src/repaint.rs` の `"view.rs"` は**テストの fixture リテラル**であり、一括置換すると **assert は緑のまま fixture だけ壊れる**。踏む機会を倍にする理由が無い。

### 4. `view.rs` を残さず 2 ファイルを新設して全量を移す

却下した。`EguiView` を実装する型は `runtime.attach` へ move される 1 つでなければならず、その型は結局どちらかのファイルに住む。両方を新設すると `view.rs` の削除 + 新設 2 件で `git log --follow` の追跡が切れる。**残せるものは残す。**

### 5. `update()` を分割する

却下した。確定事実 3（issue で確定）のとおり、**最長の順序制約が関数の全長にわたる**——`SearchState::reset()` の `rows_generation` bump（冒頭）が末尾のクリック照合（#699）と結ばれている。「冒頭の消費群を 1 つの関数へ抽出する」形は、その制約を関数の外へ落とす。独立導出も同じ結論に達した。

### 6. 全域 `Effect` enum を導入する／`Option` 群を ADT へ置き換える

issue が確定事実 6・7 で却下済み。順序制約が `Vec<Effect>` の並びへ移るだけで variant と dispatcher が肥大する。型を導入する基準は「副作用であること」ではなく「**呼び出し側が処理を忘れると不変条件が破れること**」に置く（`EscapeOutcome` が成功例）。

### 7. `state: SearchState` を `pub(super)` にして view から直接読む

却下した。アクセサを 10 本以上書く手間は避けられるが、view が `&mut` を得られる形になり「遷移は controller だけが起こす」が規約に落ちる。

**代わりに `pub(super) fn state(&self) -> &SearchState` の 1 本にした**——`SearchState` の read メソッドは全て `&self` で mutator は `&mut self` を要るため、**共有参照 1 本で読みを全て通し、変更を型で不能にできる**。アクセサの数と表現力は独立に決められる。

### 8. `on_nav_keys` へ `&PreWidgetInput` を渡す（同型 4 連 bool を避ける）

却下した。`PreWidgetInput` は `view.rs` の型であり、渡すと `launcher_controller` が view に依存して**依存の一方向性が破れる**。`right` / `left` の取り違えを型が捕まえないのは受容する残余で、呼び出し点が 1 つであることが唯一の防壁である。

## 帰結

- **規則 R(段 3) は 3 段すべてを ADR-window-coordinator-split-rule と同じ論法で説明する。** 次に `egui_shell` へ線を引くときはこの 4 条項に当てる
- **`font_stack.rs` の命名は機構の弱点（G-module-index の basename 方式）を避けた結果である。** 「なぜ `font.rs` でないのか」を将来問われたらここを引く
- 却下 2 の一般則（機構が弱いと明記している箇所を踏む名前を選ばない）は、`governance-check.mjs` が同種の弱化を他にも持つ場合に再利用できる
