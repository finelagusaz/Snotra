# Retrospective — #671/#673 egui 窓所有と配送経路（PR A: smoke カバレッジ / A′: ResultsWindow / B: read_visual / C: platform-event 解体 / D: ctx 複製の解消）

## よかったこと

### 当初案を実装 0 行で取り下げ、否定の知識を spec に残した

サイクルの入口では「runtime に `WindowKind{Primary, Subordinate}` と `EguiWindowHandle{show, hide, set_topmost, wake}` を入れて両窓の可視性を runtime 管理下へ移す」案（I-1）で進めるつもりだった。5 レンズのレビューと実測がこれを覆した——`EguiWindow.visible` は製品コードで**恒真 true**（`hide_window()` / `close_window()` の製品呼び出し元がゼロ）ゆえ、I-1 が「構造的に閉じる」と称した空白窓は今日発生しない。さらに `window.hide()` の今日の安全性は**それが private な 1 行であること**に由来しており、`hide()` を public API にすると括り付いた 4 副作用（世代 bump・位置保存・`main_visible=false`・working set trim）が付いてこない。

**「構造で守る」を掲げた案が、実際には既にある構造的保証を規約へ格下げしていた。** 撤回の判断と根拠を spec §2 に否定の知識として置いたので、次に同じ案を思いついた人は同じ道を辿らない。

### compile-fail を移行漏れ検出器にする形が、grep 探索を不要にした

PR D は `attach` の戻り値型を変え、`EguiShellState` の 2 フィールドを消し、`wake_view` を改名した。その瞬間にコンパイラが移行点を**10 箇所すべて**列挙した（wake 呼び出し 7 + 各 view の `setup` 2 + `main.rs`）。探す作業が消え、「探し漏れたかもしれない」という不確かさも消えた。

これは #671 本文が提案していた `EguiRuntime::wake(label)` 案を**採らなかった**ことの利益でもある。ラベル文字列で窓を引く API なら、綴り間違いも移行漏れも**沈黙**する。spec 決定 8 が typed handle を選び、issue の当初案を上書きしていたのが効いた。

### 一次資料が、正しい前提の「誤った理由」を暴いた

PR D は managed state の manage 位置を窓生成の後へ移す。その根拠として「tauri の setup フックはイベントループより前に走るから」と書きかけたが、`tauri-2.11.4/src/app.rs` を読むと **setup は `RuntimeRunEvent::Ready` の arm、すなわちイベントループの中**で呼ばれていた。前提（フレームは走らない）は正しく、理由が誤っていた。

実測（trace の seq 比較）でも前提の真偽は確かめられたが、**それは誤った理由を通してしまう**。ソースを読んだことで正しい根拠（ポンプ停止中ゆえ plugin の `on_event` が回らない／そもそもこの時点で pending な状態を立てる setter が存在しない）に置き換わり、`src-tauri/CLAUDE.md` に接地した記述を残せた。

### 「閉じたもの / 閉じていないもの」を並記する型を作った

全称表現の過剰は A・A′・B の 3 サイクル連続で計 6 件出ていた。D では**限定の並記を成果物の型にした**——PR 本文・実装コメント・計画書のいずれにも「閉じた 2 点」と「閉じていない 3 点」（worker が持つ一時的な Context clone は残る／`try_state` の `Option` は残る／活性化前の wake は活性化を待つ）を書いた。結果、「`Destroyed` を越える**長寿命の** Context clone が無くなる」という書ける主張と、「join が常に走る」という書けない主張を、レビューを待たずに自分で切り分けられた。

---

## 伸びしろ

### spec が「他のすべての前提条件」と宣言した網が、CI では一度も走っていなかった

PR A は「results 窓の自動検証はゼロ」を出発点に smoke を拡張し、spec §4 は「**A は他のすべての前提条件である**」と明記して A を先頭に置いた。しかし CI ログを遡ると、A・B・C・D の **4 run すべてで results の検証が skip されていた**（`NOTE: results window coverage was SKIPPED`）。同じ job の前段（startup smoke の 5 起動）がランナーに `config.toml` を作り、後段の `-SeedConfig` が「既存 config は上書きしない」設計ゆえ seed を諦める——**job が自分で自分の前提を壊していた。**

どちらのスクリプトにもバグは無い。順序の組み合わせだけで網が消え、しかも `Smoke` job は success で、skip は緑のログに埋もれる。**「緑」は「検査が走った」の証拠ではない**という #497 の教訓（`selectChecks` に載っていないファイルの沈黙は合格ではない）が、CI 側に同じ形で再発していた。

自分の側の失敗はもう一段手前にある——D の検証を「CI に委ねる」と決めたとき、**委ねる先が何を実行するかを、委ねる前に確認しなかった**。委譲は対象の中身を見ないための手段ではない。教訓は `docs/build-commands.md`「PR 上の実行責任」へ、機構化（skip を失敗にする）は #686 へ置いた。

### 消した非対称が荷重を持っていた——しかも trace は「効いた」と言い続けた

A′ は「results の hide 経路が 2 つあり、可視フラグを更新するのは片方だけ」という非対称を型で消した。実機で「Escape で main を閉じても results が残る」が再現した。**PR A′ 以前にこれを防いでいたのは、まさにその非対称だった**——view-local のフラグは `hide_egui_main` から到達できず stale な true のまま残り、結果として show を skip していた。意図されない保護だったが、実効的な保護だった。

さらに悪いのは、`smoke:egui` の presence 検査がこの回帰を**素通りさせた**ことである。orphan でも `egui_results:hide` は出る。「操作を要求した」ログは「操作が効いた」ことを意味しない。教訓は 2 箇所に配置済み——AGENTS.md 条件別チェック「重複した読み・冗長に見える状態を束ねる/消す」と `src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」。

### 計画に自分で書いた制約を、実装で自分が破った

PR B の計画には「guard 内で行うのは hex parse と算術と `&str` 比較まで。I/O や重い確保を足さない」と自分で書いた。その実装で `VisualConfig::default()` を毎フレーム guard 内に確保していた（レビューが検出）。**計画の制約は、書いた時点では守られていない**——実装中に読み返す対象である。

今回は独立レビューが捕まえたので機構は機能した（明示的に失敗するものに見張りは要らない）。記録として残す。

### 機構で閉じられない誤りが 1 つ残った——同型ハンドルの取り違え

PR D で waker と窓の対応は `create()` 内の 2 つの `attach` 呼び出しの**順序**で決まり、2 つの handle は**同じ型**である。取り違えても compile もテストも smoke も通る（results は毎フレーム再駆動されるため正常に見え、症状は「config を変えても次の打鍵まで main が再描画されない」だけ）。

`MainWaker` / `ResultsWaker` の newtype を検討したが**閉じない**——分岐点は「どちらの `attach` の戻り値か」であり、包む時点では両方まだ同じ型である。機構が届かないため、**取り違えたときに一方だけが壊れる観測を 1 つ決める**方針に切り替えた（実機での config 外部変更 → main の即時再描画）。この検査の型は `/symmetric-check` の Step 2c として配置した。
