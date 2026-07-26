# plan-review L2: 窓の状態と純粋核（`results_window.rs` / `layout.rs`）

対象: #749 plan.md の Phase 1（`layout.rs` に `size_delta_exceeds` 追加）/ Phase 2（`results_window.rs` にデルタガードを内包）。

## 問題なし

1. **`set_size` の呼び出し元は 1 か所のみ（plan.md の前提が正しい）**
   `grep -n "results\.(show|hide|set_topmost|set_size|set_position|scale_factor)\("` の結果:
   - `results.set_size(width, applied_height);` — `src-tauri/src/egui_shell/view.rs:861`（1 件のみ）
   - `results.show()` — `view.rs:867`（1 件）
   - `results.hide()` — `view.rs:826` と `mod.rs:487`（2 経路。research.md の記述と一致）
   - `results.set_topmost(...)` — `commands/window.rs:99,145`（設定サイドカー監視の 2 箇所）
   - `results.set_position(...)` — `mod.rs:596`（`position_results_below_main` 内の 1 か所）
   - `results.scale_factor()` — `mod.rs:616`（`results_available_height` 内の 1 か所）
   `set_size` が唯一 1 経路であることは実測どおりで、「自己ガード化しても外から見た挙動は同一」という plan.md の根拠は成立している。

2. **`last_results_height` / `last_results_width` の全参照を数え上げ済み、plan.md の記載と一致**
   `grep -n "last_results_height|last_results_width"`（`view.rs`）:
   - 宣言: `287`（height）/ `291`（width）
   - 初期化: `317`–`318`（いずれも `0.0`）
   - 比較（デルタガード本体）: `858`–`859`
   - 更新: `862`–`863`
   - reset-on-show での 0 復帰: `1194`–`1195`
   plan.md 「変更しないと決めたもの」「Phase 2」が挙げる箇所（287/291・317-318・858-864・1194-1195）と過不足なく一致。plan.md が挙げていない読み書きは無い。

3. **`commands/window.rs` のポーリングスレッドは `last_size` を触らない**
   `commands/window.rs:96-100`（`launch_settings_process` 本体・呼び出しスレッド）と `:142-146`（`std::thread::spawn` 内のポーリングスレッド）はどちらも `set_topmost` のみを呼ぶ。`last_size` の読み書き経路は `view.rs` の `drive_results_window`（`set_size` 内で新設）と reset-on-show ブロック（`reset_size_guard`）の 2 つで、どちらも `SearchWindowView::update()` 内、つまり同一（イベントループ）スレッドに閉じる。ポーリングスレッドとの競合は無い。plan.md「新たに導入する状態の異常系」の記載どおり。

4. **`Send + Sync` 要件は既存構造から自明に満たされる**
   `ResultsWindow` は既に `tauri::Window` フィールドを持ち managed state（`app.manage`）として登録されている（`mod.rs:214,274`）ため、既存フィールドで `Send + Sync` は成立済み。`Mutex<(f64, f64)>` は `f64` が `Send` なので追加しても `Send + Sync` を破らない。ロックは新設 1 個（`last_size`）のみで、`set_size` / `reset_size_guard` の呼び出し内で取得・解放が閉じる（他ロックとネストしない）ため、lock 順序の懸念はない（`visible: AtomicBool` はロックではないため相互作用しない）。

5. **「デルタガードは correctness ではなく性能用」という根拠は既存コードの記述と一致**
   - `view.rs:285-286`: 「こちらは冗長な `set_size` を避ける性能上のガードであり概念が別（#671 spec 決定 2）」
   - `view.rs:1188-1193`: 「これは冗長な `set_size` を避ける性能上のガードであり、可視性のような correctness のフラグではない」
   plan.md Phase 2 の `last_size` doc 案（「性能上のガードで、correctness のフラグではない」）はこの既存記述をそのまま移設したものであり、根拠は成立している。

6. **reset の置き場（view の reset-on-show ブロックに残す）の理由付けは実コードで成立する**
   - `show_egui_main`（`mod.rs:366-451`）は `reset_pending.store(true, ...)` するだけ（`mod.rs:374`）で**消費しない**。呼び出し元はホットキー listener（`main.rs:427-433` 付近、`app_handle.listen(HOTKEY_PRESSED, ...)` のコールバック）で、`src-tauri/CLAUDE.md`「Win32 メッセージ配送の注意」により listener は emit 元スレッド上で同期実行される——ホットキーは `platform/` の Win32 メッセージループスレッドが emit するため、`show_egui_main` はそのスレッドで走る
   - `reset_pending` の**消費**は `view.rs:1169-1196`（`SearchWindowView::update()` 内の reset-on-show ブロック）で `swap(false, ...)` により行われる。`update()` は egui のフレーム駆動（main のイベントループスレッド）で呼ばれ、`drive_results_window` の呼び出し（`view.rs:1838`）も同じ `update()` 呼び出しの後段にある
   - したがって reset 消費と `drive_results_window` は**同一スレッド・同一関数呼び出し内**で順序が保証される。`show_egui_main` へ `reset_size_guard` 呼び出しを移すと、ホットキースレッドから（`drive_results_window` が走っているかもしれない）main のイベントループスレッドの状態を非同期に触ることになり、plan.md が言う「スレッド同一性という前提を崩す」は正確な指摘

7. **`set_position` を無ガードのまま残すことは `set_size` の自己ガード化と矛盾しない**
   `mod.rs:569-573` のコメント「デルタガードは持たない(set_position は同値でも安価・ガードは update 側の責務)」および「呼び出し元は 2 つ——main の update()(通常の毎フレーム従属)と main の Moved リスナー」が明記済み。`position_results_below_main` は `Moved` リスナーとフレーム駆動 driver の共用単一点であるため意図的に無ガードであり、`set_size` だけを自己ガード化する非対称は既存設計判断（#646 PR2 決定 10）の延長で、新たな矛盾を生まない。

8. **新しい `Mutex` の異常系は plan.md に明記されている**
   plan.md「新たに導入する状態の異常系」節が次を個別に記述: `reset_size_guard` 未呼び出し時の実害（無し、性能劣化のみ）／他スレッドからの並行アクセス（無い、根拠は上記 3）／lock poisoning（`lock().unwrap()`、`panic="abort"` ゆえ到達しない）／`ResultsWindow` が managed でない場合（`try_state` が `None`、早期 return）。`.claude/rules/src-tauri.md`「状態フラグを true にしたら false に戻す経路とセットで設計する」の一般則にも反しない（この Mutex はフラグではなく直近値のキャッシュ）。

9. **`size_delta_exceeds` の許容値・比較式は現行の手書きガードと厳密に同一**
   plan.md: `(next.0 - prev.0).abs() > 0.5 || (next.1 - prev.1).abs() > 0.5`
   現行 `view.rs:858-859`: `(applied_height - self.last_results_height).abs() > 0.5 || (width - self.last_results_width).abs() > 0.5`
   演算子（`>`）・`abs()` の有無・定数 `0.5` が完全一致。OR 結合のため 2 成分の割り当て順（`(height, width)` か `(width, height)`）は結果に影響しない（対称式）。境界テスト名の記述（0.5 は false・0.51 は true・-0.6 は true）も同じ演算で検算済み。

## 軽微な懸念

1. **`size_delta_exceeds(prev, next)` のタプルの成分順序が明示されていない**
   plan.md は `(f64, f64)` を「幅・高さ」のどちらの順とするか Phase 1 のコード片にもコメントにも書いていない。前項のとおり関数自体は順序に依存しないため correctness には影響しないが、`ResultsWindow::set_size(&self, width: f64, height: f64)`（既存シグネチャ・`results_window.rs:126`）との対応と `last_size: Mutex<(f64, f64)>` のフィールド doc をどちらの順で書くかが実装者の裁量に委ねられており、レビュー時に「意図した順か」を確認しづらい。実装時に `last_size` の doc コメントへ `(width, height)` 等と明記することを推奨。

2. **`set_size` の新しい戻り値 `bool` が呼び出し側で使われない**
   plan.md Phase 2 は `set_size(&self, width, height) -> bool` へ変更し `show()` / `hide()` と同型の idiom に揃えるとするが、Phase 2 の `view.rs` 変更点（「手書きガード（858-864）を `results.set_size(width, applied_height);` の 1 行へ」）は戻り値を使わない。`show()` / `hide()` の `bool` は呼び出し側の `trace` 条件分岐に使われているのに対し、`set_size` の `bool` は本 PR時点で消費者がいない。実害はない（`#[must_use]` は付与されない設計であるため警告も出ない）が、「戻り値」を足す設計上の理由が plan.md 内で説明されていない（trace 用途を将来足す下地なのか、単に idiom を揃えるためだけなのか）。

## 要対処

1. **`layout::size_delta_exceeds` が `results_window.rs::set_size` から呼ばれる、と plan.md のどこにも明記されていない**
   plan.md 全文を検索すると `size_delta_exceeds` は Phase 1（定義・テスト名）と「追加テスト」節にのみ現れ、Phase 2（`results_window.rs` の変更内容）の記述には一度も登場しない（`grep -n "size_delta_exceeds" workspace/plan.md` の結果: 17, 39, 46, 47, 202 行のみ、いずれも Phase 1 か「追加テスト」節）。Phase 2 は「`set_size` を自己ガード化する」とだけ書き、実装が `layout::size_delta_exceeds(prev, next)` を呼ぶのか、現行の手書き比較式（`view.rs:858-859`）をそのまま `results_window.rs` 内へ複製するのかを指定していない。

   plan.md の overview は「デルタガードの判定式を純粋核へ出すこと（→ 本 PR で唯一得られる自動カバレッジ）」を本 PR で意味のある判断の 1 つとして掲げている。しかし Phase 2 の記述だけを読んで実装すると、`results_window.rs::set_size` 内に**独立した比較式**を書いてしまい得る——その場合:
   - `layout.rs` のテストは「production で実行されている式」ではなく「孤立した同値の式」を検証するだけになり、overview が謳う「唯一得られる自動カバレッジ」が事実上成立しなくなる
   - 同じ判定式が `layout.rs`（テスト対象）と `results_window.rs`（実行対象）の 2 か所に手書きで存在することになり、`AGENTS.md`「条件別チェック」の「関数・型を新規定義／改名／導入」行が明示する「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」に反する（`/dry-check` 対象の重複）

   **対処案**: Phase 2 の `results_window.rs` 変更点に「`set_size` の自己ガードは `crate::egui_shell::layout::size_delta_exceeds(prev, (width, height))` を呼ぶ（比較式を `results_window.rs` 内で書き直さない）」という 1 行を明記する。`layout` モジュールは `mod.rs` で `mod layout;`（非 `pub`）だが、`results_window.rs` も `egui_shell` の子モジュールであるため既存の `view.rs` の呼び出し（`crate::egui_shell::layout::present_results(...)` 等、`mod.rs` での re-export なし）と同じパスで到達可能（実測: `mod.rs:6` `mod layout;` は re-export なしで `view.rs` から直接参照されている）。

## 未検証（理由）

- **`cargo test -p snotra` によるテスト実測**: 本レビューは静的読解（grep・ソース読解）のみで、`layout.rs` への `size_delta_exceeds` 追加や `results_window.rs` の自己ガード化を実装した上での `cargo check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` は実行していない（plan.md はまだコード化されていないため実行不能）。特に「要対処 1」で指摘した『production 側で `size_delta_exceeds` を呼ばない実装になった場合に dead_code lint が落ちるか』は理論的検討に留まる——`cargo clippy --all-targets` はテストターゲットも含めてコンパイルするため、`#[cfg(test)] mod tests` が `size_delta_exceeds` を呼ぶ限り `dead_code` では落ちない（テスト参照が「使用」とみなされる）と推測されるが、実行して確認していない。ゆえに「要対処 1」の実害は**コンパイルエラーではなく、設計意図（重複排除・テストの実効性）の毀損**である点を明記しておく
- **`show()` / `hide()` の `bool` を将来 `set_size` の trace 用途に転用する計画があるか**: plan.md・research.md・issue #749 本文のいずれにも記載が無く、issue のコメント全文や関連 PR（#752/#756）のうち本 PR に無関係な部分までは確認していない。「軽微な懸念 2」の意図が意図的な下地作りなのか未使用のオーバーヘッドなのかは判断材料が無い
- **`Mutex<(f64, f64)>` 導入後の `cargo doc --workspace --no-deps --document-private-items` の実際の出力**: plan.md テスト方針カテゴリ A に含まれるが、doc コメントの rustdoc 記法（リンク切れ・intra-doc link 等）が壊れないかは実際に生成して確認していない

## チェックリスト（観点 1〜11）

| # | 観点 | 結果 |
|---|---|---|
| 1 | `set_size` 呼び出し元が 1 か所のみか | 問題なし（grep で実測・view.rs:861 のみ） |
| 2 | `last_results_height/width` の全参照の数え上げ | 問題なし（plan.md の記載と過不足なく一致） |
| 3 | `commands/window.rs` ポーリングスレッドとの `Send+Sync`・lock 順序・poisoning | 問題なし（ポーリングスレッドは `last_size` 非接触、ロックは新設 1 個のみでネストなし） |
| 4 | `layout.rs` への `pub fn` 追加と re-export・`dead_code`・新 API 導入と呼び出し移行の一体化 | **要対処**（呼び出し点の明記が plan.md に無い。§要対処 1） |
| 5 | 「デルタガードは性能用・correctness ではない」の根拠 | 問題なし（既存コメントと一致） |
| 6 | reset の置き場（view 側に残す）とスレッド同一性の論拠 | 問題なし（実コードで show_egui_main と update() が別スレッドであることを確認） |
| 7 | `set_size` 自己ガード化と `set_position` 無ガードの非対称の整合性 | 問題なし（既存設計判断の延長と確認） |
| 8 | 新 `Mutex` の異常系記述の有無 | 問題なし（plan.md に明記済み） |
| 9 | `size_delta_exceeds` の許容値・式が現行手書きガードと厳密に同一か | 問題なし（式・定数・演算子が完全一致） |
| 10 | `size_delta_exceeds` の切り出しは YAGNI か | 問題なし（ADR-0007 の既存パターン＝純粋算術だけを核へ出す手法と整合。ただし価値の実現は要対処 1 の解消に依存） |
| 11 | 未検証項目の明記 | 上記「未検証（理由）」節に記載 |
