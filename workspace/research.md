# 調査: #1399 opener の find_matching_tools がパス条件を NFC で照合しない

## issue の要約

`snotra-core/src/opener.rs` の `find_matching_tools` は、ルールのパス条件（`folder:<path>` / `ext:<exts>:<path>`）と、起動するエントリのパスを、小文字化と `/` → `\` の置換だけで前方一致させる。Unicode の合成（NFC）を通さない。そのため、正準等価な NFC と NFD が混ざると、ルールが当たらず、ShellExecuteW のフォールバックで開かれる。#1394（PR #1398）では、検索のパスマッチに `query::compose_for_path_match` を当てた。この経路は射程の外として残った。

## 関連するファイル・シンボル

- `snotra-core/src/opener.rs`
  - `find_matching_tools(path, is_folder, rules) -> &[OpenerTool]`（照合の本体）
    - `path_lower = path.to_lowercase().replace('/', "\\")`
    - `path_ext` は `path_lower` の最後の `.` 以降
    - パス条件: `cond_lower = cond.to_lowercase().replace('/', "\\")` → `path_lower.starts_with(&cond_lower)` → 境界の判定（`path_lower.as_bytes().get(cond_lower.len())` が `\` / `/` / 末尾か）
    - 拡張子: `ext_part.split(',')` の各要素を `to_lowercase` し、`.` を補って `path_ext ==` で比べる
    - 具体度: `path_cond.map_or(0, |c| c.len())`（**生の条件のバイト長**）
  - `opener_specificity_order(target)`: 設定の並び（`normalize_openers` のソート）に使う。これも `p.len()`（生のバイト長）
  - `normalize_opener_target`: 保存時の正規化。パス条件は `config::normalize_scan_path_key`（trim・`/`→`\`・小文字化・末尾 `\` 除去）を通す。NFC は通さない
  - `extract_path_condition` / `extract_ext_part` / `split_ext_and_path`
- 呼び出し元（`find_matching_tools`）:
  - `src-tauri/src/commands/launch.rs:107`・`:158`
  - `src-tauri/src/egui_shell/launcher_controller/activation.rs:574`
  - どれも `path` は索引のエントリのパス（`target_path`）か履歴のパスをそのまま渡す（OS が返した綴り。NFD がありうる）
- `normalize_openers` の呼び出し元: `snotra-core/src/config/migrate.rs:75`（`apply_migrations`）・`snotra-settings/src/app.rs:225`
- `snotra-core/src/query.rs:46` の `compose_for_path_match(buf, s) -> &str`（`pub(crate)`。合成済みなら借用のまま返す）
- `SPEC.md` §18.3「パス条件のマッチング: パスが条件のプレフィックスで始まるかで判定。ケースインセンシティブ。パス区切り文字 `/` と `\` は同一視」/ 具体度 = パス条件の長さ

## 再利用できる既存のパターン

- #1394: 照合の直前に、両辺へ `compose_for_path_match` を当てる。識別用のキー（保存される綴り）には当てない。合成は小文字化・区切りの統一の**後**に置く（`normalize_entry_key` → 合成の順）
- 互換分解（NFKC）はしない（#1394 の検知器 `path_match_does_not_apply_compatibility_decomposition` と同じ扱い）

## 技術的な制約

- `find_matching_tools` は起動 1 回につき 1 回呼ばれる（毎打鍵の全件走査ではない）。確保の費用は問題にならない
- 戻り値は `&'a [OpenerTool]`（`rules` の借用）。合成した一時文字列は関数内に閉じる
- 境界の判定は `cond_lower.len()` のバイト位置で `path_lower` を引く。合成でバイト長が変わるので、**両辺とも合成後の文字列で**長さと位置を取らないと、境界がずれる
- 具体度を生の長さのままにすると、正準等価な 2 つの条件（NFC と NFD）は NFD の方が長く、より具体的と扱われる。SPEC の「同具体度なら定義順」が効かなくなる
- 拡張子もパスから切り出している。パスだけを合成し、ルールの拡張子を合成しないと、NFD で書かれた拡張子のルールが NFD のパスに当たらなくなる（今は当たる）＝退行
- `normalize_opener_target`（保存される綴り）で合成すると、`config.toml` の書き戻しが変わる（保存形式の意味の変化）。#1394 の「識別用には当てない」に揃えるなら、照合の直前で合成する

## 未解決の疑問

1. パス条件に NFD が入る経路はあるか。設定 UI のテキスト入力は IME 由来で NFC になるはず。フォルダ選択ダイアログ（`rfd::FileDialog`・`snotra-settings/src/tabs/opener.rs:292`）は OS の綴りを返すので、NFD のフォルダを選べば NFD が入る。逆に、NFD のフォルダを NFC で手打ちした場合がこの issue の症状になる
2. 具体度を合成後の長さにするか（`find_matching_tools` と `opener_specificity_order` を揃えるか）
3. 拡張子の比較も合成するか
4. 小文字化と合成の順序。#1394 は小文字化 → 合成。両辺で同じ順なら結果は一致する

## 敵対的調査の反映（workspace/adversarial-1399.txt）

- 採: 「フォルダ選択ダイアログが NFD を返す」は誤り。パス条件の欄は素のテキスト入力（`snotra-settings/src/tabs/opener.rs:266`）で、292 行は実行ファイルの選択。NFD が入るのは貼り付けだけ（コードで確認）
- 採: 具体度の影響は同具体度のタイブレークに留まらない。入れ子のルールで、浅い NFD の条件が深い NFC の条件に生のバイト長で勝ちうる。合成後の長さで比べる
- 採: `opener_specificity_order` が効くのはタイブレークと設定 UI の表示順だけ。それでも具体度の定義を 1 つに保つため、同じ関数にそろえる
- 採（記録のみ）: ルール同士の等値（`normalize_openers` の重複の畳み込み・設定 UI の `r.target == target`）は照合時の合成では直らない。射程の外とする（plan.md）
- 採: 拡張子の退行（P3）は成立する。拡張子も合成する
- 採: 合成は条件の抽出の後に当てる（`extract_ext_part` の長さ計算）
- 採（⚠️ 付き）: 小文字化 → 合成の順で、正準等価な入力は一致する（Python で全スカラー値・不一致 0）。Rust の Unicode 版との差は未検証なので、AC のテストで代表例を通す
- 実環境: `config.toml` の opener は `folder:c:\workspace` の 1 件（ASCII）。今の開発機の挙動は変わらない
- 壊せなかった: P1（照合は `find_matching_tools` だけ。トレイ履歴も同じ関数に届く）、P2（呼び出し元はどれも生のパスを渡す）
