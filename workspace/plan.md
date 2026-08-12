# plan — issue #1057 パスクエリの全走査コスト

**ブランチ**: `perf/path-query-forward-pass` / **起点**: `d631e0b`
**一次証拠**: `workspace/research.md` ＋ Phase 0（下記）＋ scratchpad の
`phase0-fuzzy-result.txt` / `ablation-{B,C,D}-fuzzy.patch` / `instrumentation-fuzzy.patch`

## 目的

パスクエリ（`has_path_sep`）の 1 打鍵あたりのコストを下げる。とくに `c:\` が 60fps の
1 フレーム（16,700 µs）を超える状態を解消する。

**この目的は達成可能である**（Phase 0 で実測）——案 B 単体で `c:\` の p50 が
20,208 → 15,737 µs へ落ち、フレーム境界を跨ぐ。ただし max 20,234 は依然はみ出すので、
**「常に落ちる」から「たまに落ちる」への移動**であることを受け入れ条件に明記する。

## 受け入れ条件

1. `tests/path_query_cost.rs` の**製品レベル** 6 経路で対のレイテンシ実測（同日・同セッション・
   **同一機**・**drift 対照つき**——全 revert 後に A を測り直し、効果がばらつきを上回ることを示す）
2. **`c:\` の p50 が 16,700 µs を下回る**（max は下回らなくてよい・上記のとおり）
3. **区切り無しのクエリ（`users`）と `recent_history` が退行していない**
4. **検索結果が集合・順序とも 1 件も変わらない**（実 index 全件で現行実装と突き合わせ）
5. `cargo test -p snotra-core` 全数 green ／ `cargo doc` 通過

---

## Phase 0（完了）— 内訳の実測

**計測**: 2026-08-12 / `d631e0b` / release / **開発機 GPD WIN MINI**（Ryzen 7 8840U・23.8 GB）/
実 `index.bin` v7・312,108 件のコピー / `SNOTRA_CONFIG_DIR` を temp へ向け **`normal_mode = "fuzzy"`**
（他キーは 1 バイトも変えず・diff で実測）。ゲート充足: `entries=312108` / `cache_hit=true` /
`scan_ms=0` / `cache_save_ms=0`（**全走査も旧版昇格も走っていない**）。実 `index.bin` と実
`config.toml` は前後で不変。ablation は全て revert 済み（diff は scratchpad に保存）。

### **最初の Phase 0 は無効だった（記録として残す）**

初回は実 config の `normal_mode = "substring"` のまま測り、案 B の ablation は
`mode == SearchMode::Fuzzy` の枝を編集していた。**その枝は一度も実行されておらず、
「案 B は効かない」は何も測っていなかった。** `PERFORMANCE.md` が既に記録している
「計器が測る枝と、変更が触る枝が同じか先に確かめる」の逐語的な再演である。
新規インストールの既定は Fuzzy（`config.rs:200-201`）。

### A 側（Fuzzy・p50 µs）と 4 構成

| クエリ | (A) 素 | (B) name 除去 | (C) 床 | (D) find 除去 |
|---|---:|---:|---:|---:|
| `users`（対照） | 879 | 874 | 887 | 870 |
| `c:\` | 20,208 | **15,737** | 6,376 | 10,031 |
| `c:\users` | 14,142 | 10,970 | 5,811 | 8,131 |
| `c:\users\` | 13,689 | 10,374 | 5,267 | 7,601 |
| `\program files\` | 11,947 | 8,234 | 4,717 | 7,247 |
| `\zzz-no-such-path\` | 11,656 | 7,832 | 4,712 | 7,916 |

**(C)(D) は他 5 経路で results=0 になり top-k と tie-break を丸ごと失う。** ゆえに
引き算してよいのは 4 構成すべてが 0 件の `\zzz-no-such-path\` だけである。

### `\zzz-no-such-path\` の 4 成分（p50 µs）

| 成分 | 額 | 導出 |
|---|---:|---|
| ループ素のオーバーヘッド | 888 | |
| **name の Fuzzy スコアリング**（`Utf32String` 確保 + `fuzzy_match`） | **3,824** | A − B |
| **正規化キーの組み立て**（`with_normalized_key`） | **3,204** | D − C |
| **`find(pq)`** | **3,740** | A − D |
| 合計 | 11,656 = (A) | |

**⚠️ 加法性は独立検算ではない。** 4 測定値と 4 未知数のちょうど決定系ゆえ代数的恒等式で、
過剰決定の関係式が 1 本も無い。傍証は min 側で分解し直しても成分の帯（3,200〜3,800）と
順位が保たれること。**個々の値は ±数百 µs と見る。**

### drift 対照（全 revert 後に A を再測定）

A 側のばらつきは p50 で最大 **993 µs**（`c:\users\`）、典型は数百 µs。案 B の削減は
3,172〜4,471 µs で **3.2〜4.5 倍**。最小差の `c:\users`（-3,172）に最大ドリフト（+993）を
逆向きに乗せてなお -2,179 µs 残る。**符号が反転する余地は無い。**

⚠️ `c:\users` と `c:\users\` の大小が A と A' で入れ替わった。**この 2 経路の差は
ばらつきと同程度であり、「区切りを 1 文字足すとどちらへ動くか」をこの計測から読んではならない。**

### 計器の限界（**この計画の数字すべてに掛かる**）

`sorted_by_path` は 3 値あり、実運用点だけ反転する（probe で実測）:

| | 値 |
|---|---|
| on-disk の旗（`index.bin`） | true |
| `PathStore::build` の測り直し | true |
| **実運用点（実起動）** | **false** |

機序: `include_path_env = true` で `scan_path_env` が 100 件返し、`main.rs:207-210` →
`IndexTree::extend_with_roots`（`index_tree.rs:697`）が `*sorted_by_path = false` を
**無条件で**実行する。

**`measure_path_query_frame_cost` は PATH 併合を 1 行も行わない**ので、ハーネスの中は
`true` 側に居る。`cmp_paths` は false のとき両辺のフルパスを組み立ててから比較し、
`c:\` は全件同スコアで tie-break が総当たりで発火する（`path_store.rs` の実装コメントが明示）。

→ **本計画の数字は実運用より tie-break が安い側で取られている。実運用の `c:\` は
今回の (A) より重い可能性が高く、どれだけ重いかは未測定である。**

---

## 実装する案 — **案 B のみ。案 A は別 issue へ切り出す**

初回 Phase 0 で二択として立てたのは誤りだった。両者は排他ではなく相補的で、両方効かせれば
床は 888 µs になる。**しかし本計画は案 B だけを実装する。**

| | 案 B（**本計画**） | 案 A（**`#1059` へ切り出し済み**） |
|---|---|---|
| 内容 | パスクエリのとき name/file_name の Fuzzy スコアリングを行わない | 親→子へ照合状態を伝播する前向き 1 パス |
| 実測の額 | **-3,824 µs**（`\zzz` p50・実測） | find 3,740 + 非マッチ分の組み立て 3,204（**上限**） |
| 差分 | `score_one_entry` の分岐 1 つ + 構築時の bool 1 つ | 正規化 SSOT・並列性の作り替え |
| 検証状態 | **実測済み**（drift 対照つき） | **未実測**（新実装のコストは 0 と仮定した引き算） |

**切り出す理由**: 案 B 単体で本計画の数値目標（`c:\` の p50 < 16,700 µs）に到達している
（20,208 → 15,737 µs・実測）。案 A は天井が「新実装のコストを 0 と仮定した引き算」であり、
逐次化による並列度 16 → 1 の転落を相殺できるかも未評価である。同じ PR に載せると、
**実測済みで小さい案 B の成果が、案 A の不確実性に人質を取られる。**

### 案 B を厳密に安全にする形

**ビットマスクは使えない**——`char_bitmask`（`query.rs:45-55`）は `a-z` と `0-9` だけを写し、
`\` `/` `¥` を捨てる（`_ => {}`）。ゆえに区切り文字でエントリを弾けない。

代わりに**構築時に bool を 1 つ測って持つ**（`sorted_by_path` と同じ形——契約ではなく実測）:

- `any_name_has_path_sep`（表示名のいずれかが `\` `/` `¥` を含むか）
- 実運用点の実測は **0 件 / 312,108 件**（3 種とも 0）
- false のとき、**区切りを含む needle は名前に部分列として存在しえない**——ゆえに
  Fuzzy の name スコアリングを飛ばすのは**証明として結果不変**である
- true のときは現行どおり（外れた入力は遅い経路を通るだけ）

**⚠️ ガードは `norm_query` 側に置くこと。** `has_path_sep` は**生クエリ**を見る
（`query_plan.rs:90-93`）が、Fuzzy の needle は `normalize_query` を通った `norm_query` である。
`¥`(U+00A5) が `nucleo_normalize` で畳まれると両者がずれ、`has_path_sep` を条件にすると
**`¥` 入りクエリで name マッチを失う**。`norm_query` が区切りを含むかで判定する。

### 案 B には検知器が無い（実測）

案 B の ablation は **`cargo test -p snotra-core` 561 件と clippy を両方通過した**。
(C)(D) は 11 件落としている。**既存スイートは「Fuzzy で name マッチがパス区切り入りクエリに
当たる」形を持っていない**（`path_match_fuzzy_mode_skips_bitmask_prefilter` は name が
当たらないケースなので該当しない）。**先に Red になるテストを足す。**

---

## 変更ファイル一覧と対象シンボル

### Phase 1（案 B）

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/search/scoring.rs` | `score_one_entry`（:320） | name/file_name スコアリングのガード |
| `snotra-core/src/search/query_plan.rs` | `QueryPlan`（:28-） | `norm_query` が区切りを含むかの派生を足す |
| `snotra-core/src/search/build.rs` | 構築（`assemble`） | `any_name_has_path_sep` を測って持つ |
| `snotra-core/src/search/tests/path.rs` | 新規 | Red テスト（name マッチ × 区切り入りクエリ）＋ 順序不変 |

Phase 2 は無い（案 A は別 issue）。

## 不変条件と異常系

- スコア階層 Prefix > Substring > Kana > **Path** > Fuzzy を壊さない（`mod score_tier` の
  `const _` がコンパイル時に強制）。**案 B は name/file_name のスコアを落とすので、
  「name マッチが Path マッチに勝つ」順序が保たれることを検証で見る**
- `incremental cache` のパスクエリ無効ガード（`search.rs:199`）を変えない
- **`any_name_has_path_sep` は契約ではなく実測で持つ**（`sorted_by_path` と同じ形）。
  外れた入力（区切りを含む表示名）は現行の経路を通るだけで、結果は変わらない
- **ガードは `norm_query` 側に置く**。`has_path_sep` は生クエリを見る（`query_plan.rs:90-93`）が
  Fuzzy の needle は `norm_query` である。`¥`(U+00A5) が `nucleo_normalize` で畳まれると
  両者がずれ、`has_path_sep` を条件にすると `¥` 入りクエリで name マッチを失う

## `#1059`（案 A）へ引き継いだ事実（本計画の射程外・記録として残す）

以下はすべて `#1059` の本文へ転記済みである。

- **`PathCursor::append`（`path_store.rs:383`）は `normalize_entry_key_into` を呼ばない**——
  `push_segment` ＋ 範囲限定 `make_ascii_lowercase` という別実装で、バイト一致はコード共有では
  なく**テストだけ**が保証している（`path_store_cursor_matches_normalize_entry_key_over_real_index`）。
  「規則の正本を通せ」と言うとき、通し先は 2 つある
- 前向きパスの前提（添字の昇順 = 親→子）は PATH 併合後も成立する。**ただし根拠は `from_parts`
  ではない**——`extend_with_roots`（`index_tree.rs:668-698`）は `from_parts` を呼ばず、追加
  エントリを**常に `parent = NO_PARENT`（根）として積む**という構造による（`build` 内の
  `resolve_one` も `pi >= i` を弾く・`index_tree.rs:818`）。
  なお `incremental cache` のパスクエリ無効ガードが「添字が 0..len() の連続昇順」を支えている
  （`candidate_indices` が全件になるのはこのガードのため・`search.rs:275`）
- **非 ASCII の `char::to_lowercase()` は長さを変えうる**（ß → ss）。自動機は**正規化後**の
  文字列に対して遷移する必要があり、`byte_pos` の勘定もそこに乗る

## テスト方針と検証コマンド

```
cargo test -p snotra-core
cargo test --release -p snotra-core --test path_query_cost -- --ignored --nocapture --test-threads=1
cargo test --release -p snotra-core bench_ -- --ignored --nocapture
cargo doc --workspace --no-deps --document-private-items
```

- 実 index 全件で結果**と順序**を現行実装と突き合わせる（`*_over_real_index` の自動スキップ型）
- **変異注入で検知器が対象を見ていることを確かめる**
- **ablation / 計器の diff は revert の前に scratchpad へ保存する**（再検証可能性のため・
  今回の敵対レビューの運用指摘）

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**（結果不変が受け入れ条件 4）
- `PERFORMANCE.md`: **必要**。**計測機を明記する**（開発機は 2 台あり doc は区別していない）。
  `sorted_by_path` の 3 値とハーネスの限界も記す
- issue #1057: 機序の訂正（`PathCursor` は既にセグメント単位）と Phase 0 の内訳をコメント

## 作業項目

### Phase 1 — 案 B

- [ ] Red テストを足す（Fuzzy・区切り入りクエリで name マッチが成立する形）。落ちることを確認
- [ ] `any_name_has_path_sep` を構築時に測って持つ
- [ ] `norm_query` が区切りを含むかの派生を `QueryPlan` へ足す（**`has_path_sep` ではない**）
- [ ] `score_one_entry` にガードを入れる
- [ ] 変異注入で新テストが対象を見ていることを確かめる
- [ ] 実 index 全件で結果**と順序**が変わらないことを確かめる
- [ ] 対のレイテンシ実測（6 経路・**drift 対照つき**）／`users`・`recent_history` の非退行
- [ ] `cargo test -p snotra-core` 全数 green ／ `cargo doc` 通過
- [ ] `PERFORMANCE.md` へ実測を書く（計測機を明記）
- [ ] issue #1057 へ機序の訂正と内訳をコメントする

## 未確定（実装前に潰す）

- [x] **製品コストの内訳** — 解消（Fuzzy で再測定・上表）
- [x] **案 B は効くか** — 解消。**効く**（-3,824 µs・drift 対照つき）。初回の「効かない」は
      substring モードで Fuzzy の枝を編集していた測定環境の誤り
- [x] **表示名が区切り文字を含みうるか** — 解消。**0 件 / 312,108 件**（`\` `/` `¥` とも）
- [x] **`sorted_by_path` の実値** — 解消。**実運用点は false**（ハーネスは true）
- [x] **案 A の 3 点**（照合状態の設計・並列度 16 → 1 の相殺・部分木スキップの可否）—
      **本計画の対象外へ移した**（2026-08-12 のユーザー判断で `#1059` へ切り出し済み）
- [x] **ハーネスに PATH 併合を足して `sorted_by_path = false` を再現するか** —
      **`#1059` が前提として抱える**（同 issue「4. 計器が実運用点を再現していない」）。
      案 B の削減は tie-break と独立な per-entry コストなので、この判断は案 B の実装を
      ブロックしない

## セルフレビュー

- リスク: **中**（結果不変が受け入れ条件。案 A を外したので並列性の変更は本計画に無い）
- レビュー: **4 枠のマルチパースペクティブレビュー実施済み**（パフォーマンス / 責務分担 /
  一貫性 / 敵対的・各 sonnet・出力は scratchpad の `review-*.txt`）。**計 29 件**の所見のうち
  採用 12 件を反映した。壊れなかったのは 2 点のみ（issue の機序訂正・添字昇順の前提）
- エージェント数: 5（レビュー 4 ＋ Phase 0 再測定 1）
- 要対処: 案 A の切り出しにより、責務分担レビューの重大 2 件（照合状態の型・所有者・寿命が
  未決定／`PathCursor` の chain-miss との相互作用が未記述）は**本計画の射程から外れた**——
  どちらも案 A 固有の指摘であり、切り出し先の issue が引き継ぐ
- 未検証: ハーネスが `sorted_by_path = true` 側で測っていること（案 B の判断には効かないが、
  **`PERFORMANCE.md` へ書くときに限界として明記する**）

## 人間レビュー

- [x] 承認済み — 2026-08-12 / 問い: "**計画の承認**——`workspace/plan.md` へ注釈を入れるか、
      承認いただければ Step 6（workspace のコミット・push）へ進みます" ／ "**案 A の扱い**——
      案 B 単体で目的の数値目標（`c:\` の p50 < 16,700 µs）には届いています。案 A を Phase 2 として
      同じ issue に残すか、**別 issue へ切り出す**か。" ／
      回答: "1 承認 / 2. 案Aは切り出し"
