# research: issue #1059 — パスクエリの残り（キー組み立てと find）

## issue の要約

`#1057`（= PR `#1061` でマージ済み）が「パスクエリのとき name/file_name の Fuzzy スコアリングを
行わない」で -3,824 µs を取った。**本 issue はその後に残るコストを扱う。**

issue が挙げる残り 2 成分（`#1057` 前の分解・±数百 µs）:

| 成分 | 額 (µs) |
|---|---:|
| ループ素のオーバーヘッド | 888 |
| 正規化キーの組み立て（`PathCursor::normalized`） | 3,204 |
| `find(pq)` | 3,740 |

手段の候補は「`parent < i` を使った前向き 1 パスの KMP 状態伝播」（**未実測**）。
着手前に潰すべき 4 点（issue 本文が「本体」と呼ぶもの）は下の「未解決の疑問」に写す。

## 現状の実測（2026-08-12）

- HEAD `e1b62eb`（`#1057` = `#1061` マージ済みの main から切ったブランチ・実装差分ゼロ）
- 機体 **G1617-01（GPD WIN MINI）/ 23.8 GB / 16 論理コア** — issue の表と**同一機**
- release / `SNOTRA_CONFIG_DIR` を scratchpad の複製へ / 実 `index.bin` v7 のコピー・**312,108 件**
- config は実 `config.toml` の複製で `normal_mode` の 1 行だけ `substring` → `fuzzy`
  （`Compare-Object` で他の行が 1 つも動いていないことを実測）
- 計器・コマンドとも**未変更**（`docs/build-commands.md` の SSOT 行そのまま）

`measure_path_query_frame_cost` を 3 回。各行は 2 回暖機 + 20 標本の min / p50 / max（µs）:

| query | results | min (1/2/3) | **p50 (1/2/3)** | max (1/2/3) |
|---|---:|---|---|---|
| `users`（区切り無し） | 200 | 768 / 847 / 870 | **955 / 1,218 / 1,098** | 1,606 / 2,855 / 2,157 |
| `c:\` | 200 | 12,764 / 12,958 / 13,293 | **15,525 / 15,869 / 15,506** | 22,420 / 19,670 / 18,642 |
| `c:\users` | 200 | 9,866 / 9,434 / 9,453 | **10,940 / 10,300 / 10,935** | 13,747 / 16,330 / 14,306 |
| `c:\users\` | 200 | 9,794 / 9,027 / 9,258 | **10,679 / 10,612 / 10,677** | 13,394 / 13,755 / 13,979 |
| `\program files\` | 200 | 7,162 / 7,329 / 7,401 | **7,590 / 7,597 / 8,140** | 9,683 / 9,527 / 9,996 |
| `\zzz-no-such-path\` | 0 | 7,385 / 7,256 / 7,316 | **7,762 / 8,731 / 8,293** | 10,737 / 11,915 / 10,952 |

参考（同一セッション・1 回目の実行で同時に取得）:

- `measure_recent_history_cost`: `recent_limit = 8` / 返した件数 5 / **最小 5.0 ms**
  （受け入れ条件「`recent_history` が退行していない」の**参考値**。受け入れは同日・同セッションの
  対で測ることを要求するので、**この 2026-08-12 の値を後日の B 側と突き合わせてはならない**
  ——実装時に A 側を測り直す）
- `measure_path_query_sweep_cost`（走査だけを切り出した写し・判定には使わない）:
  `\zzz-no-such-path\` で 保持 2.0 ms / 導出:素 35.2 ms / 導出:ASCII 5.2 ms

## 検算 — issue の予測と一致した

issue の予測は「`#1057` 適用後に `\zzz-no-such-path\` は 11,656 − 3,824 = **7,832 µs**」。
実測 p50 は **7,762 / 8,731 / 8,293**、min は **7,256〜7,385**。1 回目の p50 は予測と 70 µs 差で、
issue の誤差帯（±数百 µs）の内側にある。**`#1057` の効果と残額の見積もりはどちらも成立している。**

- ただし `zzz` の p50 はばらつきが大きい（7,762〜8,731・約 12%）。min は 129 µs 幅で安定するので、
  **この行の対比較では min を併記しないと 1 ms 級の差を読み違える。**
- ばらつきが小さい行（`c:\users\`: p50 幅 67 µs / `c:\`: 363 µs）と混ぜて語らないこと。

## 実測から出た、issue 本文に無い事実

### 1. `c:\` は「本 issue の対象を全部消しても」1 フレームに収まらない可能性がある

`c:\` の p50 15.5 ms と `zzz` の p50 7.8 ms の差 **約 7.7 ms** は、本 issue の対象（組み立て + find）
**ではない**。ただし**この差はマッチ後コストの下限であって、その値ではない**——`find` は一致で
打ち切るため、両行が同じ `find` を払っていないからである:

- `c:\` は全 312,108 件がバイト 0 で一致する（3 バイト比較で終わる）。**find はほぼ 0。**
- `zzz` は 0 件ヒットゆえ全件の全長（平均 119.3 B）を走査して失敗する。**issue の分解の
  find = 3,740 µs は、この zzz 行で測られた額である。**
- 傍証は同じ表の中にある。`\program files\`（200 件返す）の p50 は zzz とほぼ同じ 7.6〜8.1 ms
  ——**「結果が返るか」ではなく「何件が一致するか」がコストを決めている。**

→ issue の分解を借りた推定では、`c:\` のマッチ後コストは
15,525 − 888（ループ素）− 3,204（組み立て）− ≒0（find）≈ **11.4 ms**（issue と同じく ±数百 µs）。
その正体は 312,108 件ぶんの履歴照合・ヒープ push・tie-break（`cmp_paths`）である。

→ 仮に本 issue の対象を丸ごと消しても `c:\` には**約 11 ms** が残る。**`c:\` の max は 3 回とも
18,642〜22,420 µs で 1 フレーム（16,700 µs）を超えている。**

`#1061` のコミット題「`c:\` が 1 フレームの内側へ」は **p50 については正しい**（15.5 ms < 16.7 ms）が、
max では超える。本 issue の受け入れを書くときに「1 フレーム内」を p50 と max のどちらで言うか決める。

### 2. 論点 4（計器が実運用点を再現していない）は、この開発機で事実である

実 `config.toml` は **`include_path_env = true`**（`Config::default()` は `false`）。
`index_tree.rs:761` の `*sorted_by_path = false;` は `extend_with_roots` の中で**無条件**に走る
（`self` の網羅分解の直後・条件分岐なしを実測）。ゆえにこの機の実起動は `false` 側に居り、
ハーネス（PATH 併合を 1 行も行わない）は `true` 側に居る。

`c:\` は 200 件が全件同スコアで tie-break が総当たりになる行なので、**上の 7.7 ms は実運用より
安い側の値である。** 一方 `zzz`（0 件）はこの影響を受けない——本 issue の対象額の測定は健全。

## 関連ファイル・シンボル（すべて grep で実在を確認済み）

| 位置 | 役割 |
|---|---|
| `snotra-core/src/search/scoring.rs:461` | `if score.is_none() && plan.path_query.is_none() { return None; }` — パスクエリ時は全件が通過する早期 return |
| `snotra-core/src/search/scoring.rs:466` | `with_normalized_key(&self.entries, i, ...)` — 組み立ての入口 |
| `snotra-core/src/search/scoring.rs:471` | `key.find(pq)` — 本 issue の 3,740 µs |
| `snotra-core/src/search/scoring.rs:79` | `with_normalized_key` — 正規化キーを得る**唯一の経路**（doc に「`f` の中から再呼び出し禁止」） |
| `snotra-core/src/search/path_store.rs:405` | `with_cursor` — thread-local `CURSOR` から `PathCursor::normalized` を呼ぶ |
| `snotra-core/src/search/path_store.rs:432` | `push_segment` — `/`→`\` だけ直して追記。ASCII は一括、非 ASCII はその場で小文字化 |
| `snotra-core/src/search/path_store.rs:421` | `CMP_BUFS` — tie-break（`cmp_paths`）が両辺を同時に持つため 2 本 |
| `snotra-core/src/index_tree.rs:761` | `*sorted_by_path = false;`（`extend_with_roots`・無条件） |
| `snotra-core/src/indexer.rs` の `normalize_entry_key_into` | 正規化規則の**正本**。セグメント単位で書き起こしてはならない |
| `snotra-core/tests/path_query_cost.rs:184` | `measure_path_query_frame_cost`（判定に使う製品レベルの計器） |

## 再現手順（受け入れの「同日・同一機・drift 対照」で再実行するための正本）

```powershell
$sp = "<scratchpad>"
$dst = "$sp/snotra-config-1059"
New-Item -ItemType Directory -Force $dst
Copy-Item "$env:APPDATA/Snotra/config.toml","$env:APPDATA/Snotra/index.bin","$env:APPDATA/Snotra/history.bin" $dst -Force
(Get-Content "$dst/config.toml" -Raw).Replace('normal_mode = "substring"','normal_mode = "fuzzy"') | Set-Content -NoNewline "$dst/config.toml"
Compare-Object (Get-Content "$env:APPDATA/Snotra/config.toml") (Get-Content "$dst/config.toml")   # 1 行だけであることを確認
$env:SNOTRA_CONFIG_DIR = $dst
cargo test --release -p snotra-core --test path_query_cost -- --ignored --nocapture --test-threads=1
```

生の出力は `baseline-run1.txt` / `baseline-run2.txt` / `baseline-run3.txt`（scratchpad）。

**`index.bin` を複製へ向けるのは環境再現のためだけではない**——`load_or_scan_with_stats` は旧版を
読むとその場で昇格保存するので、実ファイルを測定に晒すと版が書き換わる（ハーネスの `//!`）。

## 未解決の疑問（issue が「本体」と呼ぶ 4 点 + 実測で増えた 1 点）

1. **照合状態の設計** — 型（KMP 失敗関数の添字か一致済み文字数か）・所有者・寿命・確保/破棄・
   `PathCursor` の chain-miss での再構築規則。既存 `PathCursor` の「鎖が乱れても結果は変わらず
   速さだけ落ちる」性質を継承する設計か。
2. **並列度 16 → 1 の転落を相殺できるか** — 6,944 µs は**並列実行時の**額。逐次 1 パスで置き換えて
   勝てるかは未測定。**先に最小実装で単価を測り、勝たなければその実測を以てクローズする。**
3. **マッチ不能な部分木を丸ごと飛ばせるか** — 飛ばせないなら 0 件クエリでも 312,108 件ぶんの
   状態遷移を 1 コアで払う。
4. **計器へ PATH 併合を足して実運用側を再現するか** — 計器の変更であり過去の測定との比較可能性を
   壊す。**上の「実測から出た事実 2」でこの機の実運用点が `false` 側であることは確定した**ので、
   残る判断は「再現する / しない / 別の計器を足す」の選択のみ。
5. **（実測で増えた）`c:\` の残り 7.7 ms をこの issue の射程に含めるか** — 本 issue の対象を
   全部消しても残る額であり、正体はマッチ後（スコアリング・履歴照合・top-k・tie-break）。
   射程外とするなら、受け入れの「1 フレーム」の主張をどう書くかだけ決めればよい。

## 正規化の SSOT（着手時の制約・issue 本文より）

- 規則の正本は `normalize_entry_key_into`。**セグメント単位の正規化を書き起こしてはならない。**
- ただし現行の `PathCursor` は正本を呼ばず `push_segment` + 範囲限定 `make_ascii_lowercase` の
  別実装で、バイト一致は**テスト**（`path_store_cursor_matches_normalize_entry_key_over_real_index`）
  だけが保証している。自動機がどちらの経路に乗るかで「唯一の経路」の意味が変わる。
- 非 ASCII の `char::to_lowercase()` は長さを変えうる（ß → ss）。自動機は**正規化後**の文字列に
  対して遷移する必要があり、`byte_pos`（`PATH_BASE - min(byte_pos, 500)`）の勘定もそこに乗る。
