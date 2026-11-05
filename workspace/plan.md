# 計画: #1399 opener のパス条件を NFC で照合する

## 目的と受け入れ条件

`find_matching_tools` の照合で、パス条件・拡張子とエントリのパスの両辺に合成（NFC）を通し、正準等価な綴りを同一視する。

- AC1: NFC のパス条件が NFD のパスに当たる。NFD の条件が NFC のパスにも当たる（`folder:` と `ext:…:<path>` の両方）
- AC2: 境界の判定が合成後も保たれる（`c:\ゲーム` は NFD の `c:\ゲーム123\x` に当たらない）
- AC3: 具体度は合成後の長さで比べる。浅い NFD の条件（`c:\ゲ`、濁点の分だけバイトが長い）が、深い NFC の条件（`c:\ゲ\a`）に勝たない
- AC4: 拡張子の比較も両辺を合成する。NFD で書いた拡張子のルールは、NFD のパスにも NFC のパスにも当たる（今 NFD どうしで当たっているものを外さない）
- AC5: 合成を飛ばすかは各辺が自分で決める。条件が ASCII でも、パス側は合成する（U+037E → `;`）
- AC6: 互換分解（NFKC）はしない（全角英字の条件は半角のパスに当たらない）
- AC7: `opener_specificity_order` も同じ「合成後の長さ」を使い、正準等価な 2 つの条件は同じ具体度になる

## 変更するファイルとシンボル

- `SPEC.md` §18.3「パス条件のマッチング」: 正準等価（NFC）を同一視すること、互換分解はしないことを足す。具体度の「パス条件の長さ」を「照合用に正規化した後の長さ」と書き分ける
- `snotra-core/src/opener.rs`
  - 新設（private）: `match_key(s: &str) -> String` — 小文字化 → `/` を `\` に → `query::compose_for_path_match`。照合用の正規化を 1 つにまとめる（パス・パス条件・拡張子が同じ関数を通る）
  - `find_matching_tools`: `path_lower` / `cond_lower` / 拡張子を `match_key` から作る。具体度は `cond_key.len()`。境界の判定は合成後の文字列で行う（今の形のまま、長さの出所だけが合成後になる）
  - `opener_specificity_order`: `path_len` を `match_key(cond).len()` にする
  - テスト（`mod tests`）: AC1〜AC7
- `snotra-core/src/query.rs` の `compose_for_path_match` の doc: 通す場所として opener のパス条件の照合（`SPEC.md` §18.3）を足す。「識別用のキーには当てない」の対応として、opener では保存される `target` に当てないことも書く

触らないもの:
- `normalize_opener_target`（保存される綴り）と `config/paths.rs` の `normalize_scan_path_key`（スキャンパスの識別にも使う）。#1394 と同じく、照合の直前でだけ合成する
- 呼び出し元（`src-tauri` の 3 か所）。`find_matching_tools` の中で完結する

## 実装の順序

1. SPEC.md §18.3 を直す
2. `match_key` を no-op（小文字化と区切りだけ・合成なし）で置き、AC のテストを書いて Red を確かめる
3. `match_key` に合成を入れて Green
4. `opener_specificity_order` を直す（AC7 のテストを先に Red）
5. `compose_for_path_match` の doc

## 不変条件と異常系

- 合成の順序は「小文字化 → 区切り → 合成」で両辺同じ。敵対的調査が Python で全スカラー値を総当たりし、`NFC(lower(c)) == NFC(lower(NFD(c)))` の不一致 0 件を確かめた
- 合成はパス条件の**抽出の後**に当てる（`extract_ext_part` は `after_ext.len() - path_cond.len() - 1` で位置を出す。ターゲットを先に合成すると壊れる）
- `find_matching_tools` は起動 1 回につき 1 回だけ呼ばれる。確保の費用は問わない

## テスト方針と検証コマンド

- `cargo test -p snotra-core opener`（編集中）
- 権威ある検証は `/implement` の `verify:tree`（カテゴリ A・B。`SPEC.md` とコードの変更）
- 変異注入は `/implement` の委譲先が行う（候補: パス側の合成を外す / 条件側の合成を外す / 拡張子の合成を外す / 具体度を生の長さに戻す / `opener_specificity_order` を戻す / 「条件が ASCII なら合成を飛ばす」近道 / NFKC に差し替え）

## 射程の外（記録のみ）

- ルール同士の等値（`normalize_openers` の重複の畳み込み、設定 UI の `r.target == target`）では、正準等価な 2 つのルールが別物のままになる。直すには、保存される `target` へ合成を入れる必要がある（`config.toml` の書き戻しが変わる）。同じフォルダを NFC と NFD で 2 回書いたときにしか起きないので、今回は入れない
- 実環境の `config.toml` の opener は ASCII の 1 件だけである。この修正で、今の開発機の挙動は変わらない。AC の裏付けはユニットテストになる

## フェーズの作業項目

- [ ] SPEC.md §18.3 を更新
- [ ] `match_key` を no-op で置き、AC1〜AC6 のテストを書いて Red を確かめる
- [ ] `match_key` に合成を入れ、Green
- [ ] AC7 のテストを書いて Red → `opener_specificity_order` を直して Green
- [ ] `compose_for_path_match` の doc を更新
- [ ] `cargo fmt` / `cargo clippy -p snotra-core` / `cargo test -p snotra-core` が通る

## 未確定（実装前に潰す）

- [x] パス条件に NFD が入る経路 — 敵対的調査で確定。パス条件の欄は素のテキスト入力で、フォルダ選択は無い（`snotra-settings/src/tabs/opener.rs:266`）。NFD が入るのは、エクスプローラーなどからの貼り付けだけ。research.md の「フォルダ選択ダイアログ」は誤り（292 行は実行ファイルの選択）
- [x] 具体度を合成後の長さにするか — する。入れ子のルールで、浅い NFD の条件が深い NFC の条件に勝ちうる（敵対的調査）。`opener_specificity_order` も同じ関数にそろえる
- [x] 拡張子も合成するか — する。パスだけ合成すると、NFD の拡張子のルールが外れる退行になる
- [x] 保存形式（`normalize_opener_target`）で合成するか — しない。上の「射程の外」に理由

## 人間レビュー

- [x] 承認済み — 2026-11-05 / 問い: "`workspace/plan.md` に注釈をいただくか、このままでよろしければご承認をお願いいたします。" / 回答: "ok /implement"

## セルフレビュー

- リスク: 通常（永続形式・並行性・状態遷移に触れない。照合の純関数 1 つと SPEC の 1 項）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 3 件を反映（フォルダ選択の前提の誤り / 具体度の影響の過小評価 / 拡張子の退行）
- 未検証: Rust の `to_lowercase` と Python の Unicode 版の差（AC のテストで代表例を通す）
- 散文の全件列挙: 未実施（通常リスク）
- 照合 5 項: 全要件に作業項目あり / 境界は AC2・AC5・AC6 / 新しい状態なし / 既存の `compose_for_path_match` を再利用 / 不変条件（合成の順序・抽出の後）はテストと変異注入で検知
- check スキル: `/symmetric-check`（両辺の合成の対称）は実装時に行う。`/dry-check` は `match_key` の新設で該当（照合用の正規化が opener 内で 1 つになる）
