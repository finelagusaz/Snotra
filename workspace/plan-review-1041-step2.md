# 計画レビュー（独立 1 体）: #1041 並行性と状態 / 記録の読み書き

対象 issue: #1041。計画: `workspace/plan.md`。裁定（永続 JSONL・進捗表示・フォルダ数・分母は直近の完走行・残り時間なし）は争点にしない。

## 要対処

### R1. `save: None` の分岐を `rebuild_and_save` が知る手段が計画に無い
- 計画（決定 5・`RebuildStats.save: Option<Duration>`）は「保存先が引けない分岐は `save: None`」と書く。だが分岐は `save_cache_sorted` の内側（`snotra-core/src/indexer/cache.rs:593-599`、`Config::config_dir()` が `None` なら `IndexMaterial::from_tree` を返す）にあり、返り値は `IndexMaterial` だけ。`rebuild_and_save`（同 656-665）は分岐を観測できない。
- 直し方は 3 択で、計画はどれも選んでいない: (a) `save_cache_sorted` の返り値に「書いたか」を足す、(b) `rebuild_and_save` で `Config::config_dir()` を二重に引く（二重解決・SNOTRA_CONFIG_DIR の読みが 2 回）、(c) `save` を `Duration` にして `None` をやめる。
- 推奨は (c)。`drain_index` はそもそも `config_dir()` が `None` なら 1 行も書かない（計画「不変条件と異常系」）ので、`save_ms: null` は記録に載る経路で到達不能（推測ではなく、同じ関数 `Config::config_dir()` が両方の根）。到達不能な `null` のために core の返り値と純粋核の `null` テスト（計画のテスト表 3 行目）を持つのは過剰。`null` と `0` を分ける不変条件は `cpu_ms`（取得失敗）で足りる。残すなら (a)。
- 計測区間の定義も書く: save は `derive_columns` を含む（`cache.rs:621`）。cache-miss の `cache_save`（同 471-478）と同じ範囲。

### R2. `prune_in` / `last_completed_dirs_in` の「読めない行」が UTF-8 でない行を扱えない書き方になりやすい
- 計画は「読めない行も 1 行と数える」「`last_completed_dirs(body: &str, …)`」。`fs::read_to_string` / `BufRead::lines()` は 1 バイトでも非 UTF-8 があると全体（または行）が `Err` になる。書き込み途中の kill で壊れた末尾は ASCII でも、`SNOTRA_CONFIG_DIR` 下を手編集した場合や部分書きの切断は非 UTF-8 になりうる（推測）。そのとき早期 return すると剪定が永久に走らず（ファイルが 200 行を超えて増え続ける）、分母も永久に出ない。計画のテスト「読めない行が混ざっても続く」は UTF-8 の壊れた行（`{`）で書かれると上の経路を通らない。
- 計画で直す箇所: 「書き手と読み手」に「`fs::read` でバイト列を読み、`b'\n'` で分割、行ごとに `from_utf8` / `serde_json::from_slice`」と書く。`last_completed_dirs` は `&[u8]` か、`String::from_utf8_lossy` を通した body を取る。テストに非 UTF-8 バイト（例 `0xFF`）を含む行を足す（「落とすべき変異」: `read_to_string` で全体失敗）。

### R3. 末尾が改行で終わらない（kill で切れた）ファイルへの追記が次の `start` を壊す
- 受け入れ条件 2 は「プロセスが終わったら `start` だけが残る」を中心に据える。`append_in` が `"{line}\n"` を 1 回の `write_all` で書く保証が計画に無い。`write!` を 2 回に分ける・`writeln!` を BufWriter 経由にする等で途中切れが生じると、次の追記が同じ行の後ろへ連結され、新しい `start` 行が読めない行になる（= 未完走の記録が消える。扱いは「壊れた行」だが、未完走が読める、という受け入れ条件 2 の目的が崩れる）。
- 直し方: `append_in` は行末 `\n` 込みの 1 バッファを `OpenOptions::append(true)` で 1 回の `write_all`。加えて、テストに「末尾が `\n` で終わらないファイルへ追記しても新しい行が単独で読める」を足すか、書く前に末尾が `\n` かを見て足す方針を決める（どちらかを計画に書く）。
- `prune_in` の書き直し方式も未定義: 直接 truncate してから書くと、書き直しの途中で終了すると全行を失い、分母が消える。tmp→rename（`BinFile::save` と同じ形）にするか、計画の「2 プロセス同時に剪定→行が失われうる（受容）」に「剪定中の終了で全行を失いうる」を足して受容を明示する。どちらでも構築は変わらないので受容でよいが、書かないと実装者が決める。

### R4. `ScanProgress.dirs` を `pub` にし、`state.rs` の書き手の規約を破る
- `state.rs:28-31` の規約は「書くのは `new` / `try_begin_index_build` / `finish_index_build` だけ」で、フィールドは private。計画の `pub dirs: AtomicUsize` と `AppState.scan_progress`（pub）は、`drain_index` 以外からも `dirs.store` できる形で、同じ規約の反例になる（`expected` だけ private では非対称）。
- 計画で直す: `ScanProgress` は両フィールド private。書き口は `begin_iteration(expected: Option<usize>) -> &AtomicUsize`（`dirs` を 0 にして、`scan_all` へ渡す参照を返す）と、読み口 `read() -> (usize, Option<usize>)`、戻し口 `reset()`（`finish_index_build` だけが呼ぶ）。「`dirs` を `store(0)` できるのは begin と finish だけ」を doc とテストに置く。
- 併せて計画の「`AppState` の構築子（`test_state` を含む）へ初期値」は不正確。`AppState::new` 経由（`state.rs:57-66`）が唯一の外部経路で、`test_state`（`state.rs:150`・`commands/system.rs:36`）は `AppState::new` を呼ぶだけ。直す箇所は `new` の 1 か所。

### R5. 再描画の根拠が規約の字面に合っていない。計画の言葉を直す
- `src-tauri/CLAUDE.md:75`「再要求してよいのは時間経過で解消する不成立だけ」。計画（決定 9）は「構築中案内が出ている限りフォルダ数は時間とともに進むので、時間経過で変わる状態の再要求にあたる」と書く。これは「解消する不成立」ではなく「別スレッドが書く値のポーリング」であり、字面には合わない。永久スピン（`ZERO`）は避けているので結果は安全だが、実装者・レビューアが規約への適合を誤読する。
- 直し方: 決定 9 に「例外であり、理由は (1) 進捗値は worker が書き、wake が値を運ばない前提（計画の不変条件）を崩さないため、(2) 固定間隔 500 ms で `ZERO` にならない、(3) 止まる条件は構築完了（`indexing` が偽になるフレームで arm を抜ける）で、`INDEXING_COMPLETE` の wake が最後のフレームを起こす」と書く。race-check c の問い（状態を変えたあと次のフレームを起こすものがあるか）への回答は「`dirs` を書く側は wake せず、読む側の 500 ms 再要求が担う」。
- 「止まる経路」の確認結果: `indexing_raw` が偽、`is_results` が偽、`launching`/`notice` が優先（`overlay_kind` の優先順は indexing > launching > notice、`view.rs:909`）のいずれでも arm を通らず再要求は止まる。ただし**Launching が Indexing を覆っている間は再要求が止まり**、Launching が終わるフレームでは別の契機（起動完了の wake）で Indexing arm に戻って再開する、という経路になる。この前提（起動完了が必ず 1 フレームを起こす）を計画に 1 行書く（`launcher_controller` の既存挙動に依存する推測。未検証）。
- `request_repaint_after` はフレーム到来を約束しない（`src-tauri/CLAUDE.md:74`・#711）。hidden→show を跨ぐと予約は失効しうるが、show のフレームが最新を読んで再度予約するので、この計画では自己回復する（計画の記述どおりで正しい）。

### R6. `FrameScanProgress` の読み方が `indexing` との整合を保証しない箇所の言い切り
- 計画（決定 10）「`indexing` との読みの間に構築が終わっても、`finish_index_build` が進捗を 0 に戻すので従来の文言へ落ちるだけ」は正しいが、逆向きが書かれていない: `indexing` が偽（`overlay` なし）なのに progress だけ非 0 を読んでも使われない（arm を通らない）ので無害。さらに**反復の境目**（drain 先頭が `expected` → `dirs=0` の順で 2 つの `store` を撃つ間に、フレームが `dirs`（前反復の最終値）と `expected`（新）を混ぜて読む）で「前反復の N / 新しい M」が 1 フレーム出る。`dirs > M` なら「フォルダ数だけ」へ落ちる（計画の分岐）ので表示は崩れないが、決定 10 の「窓は無くなる」は偽。
- 直し方: 先頭の 2 つの書き込みの順序を `dirs = 0` → `expected = M` に固定する（前反復の N が新 M と組になる窓を無くす）。前反復→新反復で `dirs` が 0 に戻るので、読み手が `dirs=0`（旧文言）か新しい組を見る。決定 10 の文言を「書く順序を固定したので、混ざった組は出ない」へ直す。完全な原子性は不要（SeqCst 不要・Relaxed でよい理由も doc に 1 行）。

## 軽微

### M1. 反復の途中は `dirs` が凍って「N / 約 N」に張り付く
- 走査後の整列・保存・構築の数秒は `dirs` が最終値のまま、500 ms ごとの再要求は無意味に回り続ける（計画は受容と書いている）。見た目は「98,001 / 約 98,000」でなく `dirs == m` なら「98,000 / 約 98,000」が出る。`dirs <= m` の境界（等しいとき）を `strings.rs` のテストに入れる（計画のテストは `dirs <= m` の 3 分岐だけで境界値を名指していない）。

### M2. `new_sid()`（pid + エポックミリ秒）は同一プロセス内の連続反復で衝突しうる
- 空の scan 設定の短い構築や、テストで 2 反復が同じミリ秒に入れば、`sid` が同じで 2 組が区別できない（推測。製品の実走査は秒単位）。process 内の `AtomicU64` を足して `{pid}-{ms}-{n}` にするのが最小。`sid` で対にする受け入れ条件 1 の根拠なので、1 行で直せるうちに直す。

### M3. 純粋核の外の「順序」がテストされない
- 計画のテスト表は純粋核と `*_in` 単体を覆うが、肝の順序（剪定→分母の読み→`start`→走査→end、および `start` が走査の前）は `drain_index` の本体にしか無く、`indexing.rs` のテストは `top_level_fn_body` のソース文字列検査の形（`indexing.rs:319-331`）。そこへ寄せると「文字列の並び」の検査になる。
- 直し方: `scan_log.rs` に `begin_in(dir, scan_id, roots) -> (Sid, Option<usize>)`（剪定→読み→start 追記）を置き、`dir` 注入で「start が書かれた時点で、読まれた分母は start 自身を含まない過去の `end` 由来」「剪定後の読み」を単体で測る。`drain_index` はそれを `Config::config_dir()` の解決と一緒に呼ぶだけにする。実 `%APPDATA%` に触れない（`snotra-core/CLAUDE.md`「開発ルール」の `*_in` 注入形に合う）。
- 追加で、`start` を走査の前に書く順序そのものの検知器を置くなら、「呼び忘れを再現する変更で落ちる」までを確かめる（`AGENTS.md` の `Option`/フラグの出所の項）。

### M4. 計画の `scan_directory_with_extensions` は引数が 7 つになる
- 現状 6 引数（`scan.rs:156-163`）。+1 で 7。clippy の `too_many_arguments` は 8 以上で鳴くので通るが、`Dedup`（`scan.rs:86-`）はすでに「再帰の呼び出し点で組を崩せない」ために束ねた構造体。`dirs: &AtomicUsize` を `Dedup` の隣（走査状態の構造体）に入れるほうが、再帰の全呼び出しで渡し忘れが型で消える。ただし `Dedup` の責務（重複排除）に混ぜるのは別概念なので、別の `ScanCounters` 構造体にするか 7 引数のままかを決めて計画に書く。
- ホットループの確保: `fetch_add(Relaxed)` は確保しない。`read_dir` 成功直後（`scan.rs:164`）の 1 回は妥当。

### M5. `scan_id` の分母の保守的な落ち方は妥当だが、余計に落ちる
- `scan_identity_hash`（`cache.rs:300-309`）は `extensions` と `include_folders` も入れる。フォルダ数（`read_dir` の成功数）は拡張子にも `include_folders` にも依存しない（`scan.rs:164` はそれらより前）ので、`extensions` を編集しただけで分母が消える。害は「出ない」だけで構築は変わらないが、計画の言う「走査対象の同一性」は走査したフォルダの集合の同一性より狭い。受容するなら 1 行書く。
- ハッシュのプロセス間安定性: `DefaultHasher::new()` は固定キーの SipHash で、同一バイナリ・同一入力なら再起動を跨いで同値（`index.bin` の `config_hash` 照合が同じ前提・`cache.rs:295-299`）。Rust のツールチェイン更新で変わるのは計画の記述どおり。記録は 16 桁の 16 進文字列で持つので、JSON の数値精度（u64 が f64 を超える）の罠も避けられる（計画は `{:016x}`・正しい）。
- `scan_identity_hash` の doc（`cache.rs:316-320`）の書き換え: 「消費者が戻る日のために残す」の段落も消費者が実在するので直す。計画は「現在どこからも呼ばれない」を消すと書いているが、`apply_rescanned_index` 前提の段落全体が古くなる。

### M6. `end` が書かれない反復の分母の扱いは計画どおりで正しい
- `end` 行だけを分母に使うので、panic（unwind）や kill で `start` だけが残る反復は分母に入らない。ただし**走査は完走して `end` を書く前に落ちた**ケース（整列・保存中の終了）も `start` のみで、分母の更新が 1 回遅れる。受容でよい。

### M7. `unattributed` の中身を doc に書く
- `build_ms` が測るのは `build_index_from_material` だけ。`complete_index_drain` の engine ロック待ち＋スワップ＋re-diff、`with_index_write_lock` の取得待ち（`cache.rs:1059-1064`）は `unattributed` に入る（`on_enter` の同期検索が engine ロックを握る間は待つ・`view.rs:1124` の記述）。「unattributed が大きい = 計測外の別の遅さ」ではなくロック待ちの可能性が高い、と `docs/build-commands.md` の読み方に 1 行。

### M8. `status_row_present` と高さ
- 文言が長くなっても行の有無（`notify.rs:53-60`）と高さ（`metrics.toast_height`・`view.rs:957-`）は変わらない。`status_row_present` に進捗を渡さないこと（渡すと show 経路 `window_coordinator.rs:347-359` と二重の読みが増え、frame-once の規約が崩れる）を計画に 1 行書く。show 経路は `read_scan_progress` を呼ばない。

## 未検証

- U1. 長い文言（「インデックス構築中... 1,234,567 / 約 1,234,567 フォルダ」）が `painter.text` で行の幅に収まるか（ja/en）。`view.rs:966-` は clip しないので溢れた分が見えない可能性。実機の目視項目へ足す。
- U2. Launching が Indexing を覆う間の再要求の停止と、その後の再開の契機（R5）。`launcher_controller` 側の起動完了 wake の存在を確認していない。
- U3. hidden 中に `request_repaint_after` の予約が発火したとき `update()` が本当に走らないか。`src-tauri/CLAUDE.md:73` の記述に依存し、実測していない。
- U4. `GetThreadTimes` の取得に必要な features: `src-tauri/Cargo.toml:35-53` に `Win32_System_Threading` はある。`GetCurrentThread` / `GetThreadTimes` が同 feature に属するかを `cargo check` で未確認。FILETIME 用の `Win32_Foundation` もある。
- U5. `scripts/smoke-egui.ps1` 等は `SNOTRA_CONFIG_DIR` を使う（`grep` で確認済み）ので `/s` を撃つ smoke が実 `%APPDATA%` に `scan-log.jsonl` を書くことはないはず。ただし `SNOTRA_CONFIG_DIR` を渡さない経路（`manual-smoke.ps1` 等）は未確認。
- U6. 実プロファイルの `dirs` の桁（`C:\` 全再帰で数十万〜数百万）での `end` 行の大きさ・500 ms の再描画コスト。実測項目に入っているが、フレーム毎のコスト（検索を再実行しないこと）は未確認。
