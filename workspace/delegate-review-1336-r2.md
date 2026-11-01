# review-1336 r2（fix-forward の再測定）

anchor sha: 03d98761162a8455acfd795c139c3a86e9553ad1（`verify-tree.mjs align` の出力・exit 0）。前巡 42b77e01 からの差分は `.rs` 5 本（launch.rs / layout.rs / results_window.rs / window_coordinator.rs / state.rs）と workspace 3 本。変異の再注入はしていない（I5 は不変・指示どおり）。

## (1) 前巡の指摘の解消（見つけたときと同じ方法で測った）

| 指摘 | 判定 | 測り方と根拠 |
|---|---|---|
| M-1 read_config doc に射程・害が無い | 解消 | `state.rs:114-116` に「この規則に条件は付かない（#1123）——どのスレッドの読みも用途も問わない」「検索 worker は `engine.search` の間 `Mutex<Engine>` を握る→フレーム（show 経路なら表示そのもの）を止める」。`search_flow.rs:44`（射程・頻度ではない）・`activation.rs:348`・`window_coordinator.rs:206`（表示そのもの）の 3 本とも、指し先が言われた内容を持つようになった |
| L-1 「docである」 | 解消 | `layout.rs:453` は「doc である」 |
| L-2 hide_egui_main の自己言及 | 解消 | doc が「main に対する `.hide()` はこの関数の中の 1 か所だけ」に変わった。`.hide()` / `get_window("main")` の grep をやり直し、main への `.hide()` は `window_coordinator.rs:522` の 1 件だけ（`results_window.rs:377` は results ウィンドウ）であることを確かめた |
| L-3 rules の節見出し | 未対応（裁定どおり） | — |
| L-4 X10 の禁止と ADR 指し | 解消 | `results_window.rs:27-29` に禁止文と `ADR-results-gate-single-point` が入った。ADR ファイル `docs/adr/ADR-results-gate-single-point.md` は実在する |
| L-5 E2 の宣言なしの削除 | 解消 | 台帳の E 行に追記あり |
| L-6 hide_request の計画とのずれ | **未反映** | 「台帳で扱う」とのことだが、`git grep hide_request 03d98761 -- workspace/pr-material.md` は 0 件。PR 本文を書くときに入れるなら問題ない（Low） |
| L-7 A1 の理由 | 解消 | 台帳の文が「`--lib` は即時に `no library targets found` で失敗する・カテゴリ A の必須行は `--lib` 無し」に変わった。`docs/build-commands.md:23` の必須行が `cargo test -p snotra` であることを確かめた |
| W-1 launch_item_core の COM | 解消 | コメントが「COM 未初期化とみなしてここで STA に初期化する（既に MTA のスレッドでは STA にならない——その経路から呼ぶならスレッドを分ける）」に変わった。`CoInitializeEx` の意味論（MTA 済みなら RPC_E_CHANGED_MODE）と合っている |
| W-2 X19 | 解消 | 台帳に「ハンドラ自体が消滅済みゆえ 5」 |
| W-3 G-heading-refs の例示 | 範囲外（裁定どおり） | — |

## (2) 修正差分への同じ枠組み（新しい主張とコードの一致・逆向き監査）

Critical / High / Medium: なし

### Low
- L-6（再掲）: 上の表のとおり、台帳にまだ入っていない

### ⚠️
- W-4 `state.rs:114` の「**この規則**に条件は付かない」は、指す規則が文脈で揺れる。直前の段落は「`&AppHandle` しか持たない呼び出し元は `egui_shell::read_config` を使う」なので、素直に読めば「config は read_config を通す（engine 錠を経ない）」になり、意図どおりである。ただし同じ doc の :92-106 には「純 CPU だけ」という別の規則があり、:106 は「規則に反しない」と書いて後者を指している。読み違えても害は小さい（どちらの規則も条件を持たない）。偽にする手順: doc を上から読み、:114 の「この規則」を「純 CPU だけ」と読む余地が無いことを示す
- W-5 `window_coordinator.rs:491-492` の「#880 サイクル段 2 時点で…1 か所だけ」は時点を限った主張で、現時点の grep でも真である。将来、呼び出し点が増えたら偽になる形だが、時点を名乗っているので許容範囲と判断した（確信は中程度）

### 逆向き監査（差分が消した行）
- 消えた行は 5 本。そのうち `launch.rs:194` の旧文と `window_coordinator.rs:491-492` の旧文は、上の新しい文で置き換えられた。`results_window.rs:27` の「（閉包の射程は…）」は新しい括弧に保たれている。`layout.rs:453` は空白だけの変更。命題の脱落は無い
- 新しく書いた命題（`ADR-results-gate-single-point` の実在・「要るのは食い違いを作れるときだけ」・MTA の意味論・worker の錠保持・main への `.hide()` が 1 か所）は、いずれもコードか実在ファイルで確かめた

## 作業ツリー
- 追跡ファイルの差分は 0。未追跡は `.delegate/` だけ
