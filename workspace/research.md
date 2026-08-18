# 調査: issue #1123 — config の live-read 規範から例外を無くす

対象 issue: #1123「検討: config の live-read 規範から例外を無くす（残る 3 か所も `read_config` へ寄せる）」
ブランチ: `chore/config-live-read-drop-exception`

## 1. issue の要約

#1076 が config live-read 条項の例外を「走るスレッド」で切り直し、例外として 3 か所を残した。
本 issue は **その 3 か所も engine 錠の外へ出し、規範から「例外」という装置そのものを無くすか** を問う。

- 得るもの: 統治のみ（条項が縮む・スレッドの推論が読者から消える・`ADR-config-read-exception-discriminator` の案 A〜F が比較対象を失う）
- 得ないもの: 性能（3 か所とも egui フレームの外にあり、誰のフレームも止めていない）
- 費用: `resolve_opener` は錠の取り方を変えるので `/race-check` の対象（issue 本文は「錠を替える」と書くが、3b の P3 が示したとおり正確には**外側の `Mutex<Engine>` を外すだけ**で、内側の `RwLock<Config>` は現状と同一である）。かつ**規範＝セーフティネットの変更**ゆえルート `CLAUDE.md`「最重要ルール 2」により合意が要る

issue 本文の自己訂正コメント（本人）で、「#1122 の機構化が `#[allow]` 無しで済む」は取り下げ済み。実際は **`#[expect]` が 4 件 → 1 件**であり 0 にはならない（`config_watcher` は手続きの理由で射程外＝本 issue の対象外）。

## 2. 現況（一次証拠・すべて本サイクルで実読）

### 2.1 移設対象の 3 か所

| 位置 | 現在の形 | 走るスレッド | 呼び出し元（全件） |
|---|---|---|---|
| `src-tauri/src/commands/launch.rs:103` `resolve_opener` | `engine.lock()` → `config()` → `find_matching_tools` → 即 return | platform（Win32 メッセージループ） | `launch_item_with_state`（同ファイル :117）のみ。さらにその唯一の呼び出し元は `platform/tray.rs:76` |
| `src-tauri/src/commands/launch.rs:158` `resolve_all_openers` | 同上（`Vec` を返す） | platform | `platform/tray.rs:441`（`show_recent_history_menu`）のみ |
| `src-tauri/src/commands/icon.rs:7` `ensure_icon_cache_loaded_if_enabled` | `engine.lock()` → `config()` → `(show_icons, cap)` を取り出して即解放 → `icons.lock()` | icon worker（`results_view.rs:206` の `std::thread::spawn`） | `commands/icon.rs:54` `load_icon_pngs` のみ。その呼び出し元は `results_view.rs:214`（spawn したスレッドの中）のみ |

- **3 か所とも engine 錠を config 読みのためだけに取っている**（実読で確認）。`resolve_opener` は読んだ直後に return し、履歴の錠は `record_and_save` が別に取り直す。`ensure_icon_cache_loaded_if_enabled` が次に取るのは別の Mutex（`IconCacheState`）である。
- **3 か所とも `&AppState` / `&State<AppState>` を手に持っている。**
- `launch_item_with_state` / `launch_with_tool_with_state` / `launch_default_with_state` の呼び出し元は `platform/tray.rs` の 3 行だけであり、いずれも `app_handle.state::<AppState>()` から `&state` を作っている。

### 2.2 `#[expect(clippy::disallowed_methods, …)]` は 4 件（実測 grep）

`src-tauri/src/commands/icon.rs:18` / `launch.rs:108` / `launch.rs:163` / `config_watcher.rs:88`。
`egui_shell/view.rs:516` の `#[allow]` は群 1（egui global style）であり別勘定。

### 2.3 config の読み口と書き口

- 読み: `src-tauri/src/egui_shell/mod.rs:423` `read_config(app: &AppHandle, read, fallback)`。中身は `app.try_state::<AppState>()` → `read(&s.config.read().unwrap())`。
  **製品コードで `AppState.config` の read guard を取るのはこの 1 行だけである**（`grep -rn "config\.read()" src-tauri/src/` の結果は本関数と `state.rs` の `#[cfg(test)]` 2 行のみ）。
- 書き: `Engine::update_config`（`snotra-core/src/engine.rs:246`）が `*self.config.write().unwrap() = config` の 1 本。呼ぶのは `config_watcher.rs:146` の 1 か所。
- `AppState.config` と `Engine.config` は**同じ `Arc`**（`Engine::config_handle`・`state.rs` の doc）。

### 2.4 移設後の形の先例が既にある

`egui_shell/launcher_controller.rs:734` `resolve_tools` が #1076 で移設済みで、**移設後の `resolve_opener` とほぼ同型**である。

```rust
crate::egui_shell::read_config(
    &self.app_handle,
    |cfg| find_matching_tools(path, is_folder, &cfg.openers).to_vec(),
    Vec::new,
)
```

その doc が「`find_matching_tools` は錠も I/O も取らない純 CPU なので `read_config` の『read の中で lock を取る操作を書かない』に反しない——**その純粋性がこの形の前提である**」と明記している。同じ契約が移設後の launch.rs 2 か所へそのまま効く。

## 3. 技術的制約

### 3.1 【本件唯一の実危険】read guard を I/O 越しに握らせない

`resolve_opener` / `resolve_all_openers` は `std::path::Path::new(path).is_dir()` を呼ぶ。
**死んだ UNC パスでは SMB タイムアウトまで最大 21 秒塞がる**（#524 実測・当該関数の doc）。現行コードは `is_dir` を engine 錠の**前**に置くことでこれを避けている。

移設は**外側の `Mutex<Engine>` を外すだけ**で、内側の `RwLock<Config>` は `read_config` が今も取っているのと同じ錠である（3b の P3）。それでも**この規律は消えない、むしろ宛先が変わって厳しくなる**:

- **実装非依存の理由**: `is_dir` 越しに read guard を握れば、`config_watcher` の `update_config`（write）は**その guard が落ちるまで必ず待つ**（`RwLock` の定義そのもの）。設定の適用が最大 21 秒止まる。これだけで禁じるに足り、公平性の議論を要しない。
- **さらに悪化しうる形（未実測・確信度は低い）**: `std::sync::RwLock` は公平性ポリシーを文書で保証しておらず、writer starvation を避ける実装（Windows の `SRWLOCK` は writer-preference）では**待ち writer が後続 reader を塞ぎうる**。その場合は UI フレームの `read_config` まで詰まり、#1032 が直したバグクラスの再演になる。**この一節は禁止の根拠には使わない**——上の実装非依存の理由が単独で成立する（3b の ⚠️ を受けて分離した）。
- ゆえに doc コメントの理由は「engine ロックを跨いで I/O しない」から「**config の read guard を跨いで I/O しない**」へ**書き換えて運ぶ**。理由の宛先が変わるだけで、規律そのものは同一。
- `read_config` のクロージャ形はこれを構造で助ける（guard がクロージャの外へ出ない）が、**クロージャの中に `is_dir` を書けば同じ穴が開く**——構造は「guard を持ち出せない」を保証するだけで「中で I/O しない」は保証しない。これは条項の既存文（「`read_config` の中で lock を取る操作を書かないこと」）が既に持つ責務であり、新設ではない。

### 3.2 icon の 2 値は単一 guard で原子的に読む

現在 `(show_icons, cap)` は単一 engine 錠内で読む。移設後も**単一の read guard**で読む（tuple のまま）。`IconCache::load` の I/O は guard の外に残す。

### 3.3 錠の入れ子は増えない（deadlock なし）

- `ensure_icon_cache_loaded_if_enabled`: config guard を**解放してから** `icons.lock()` を取る（現行の順序を維持）。config guard と icon lock は入れ子にならない。
- `config_watcher::apply_config_change`: `update_config`（engine 錠 → config write）を**解放してから** `drop_icon_cache`（icon lock）を撃つ（`config_watcher.rs:145-158`）。こちらも入れ子でない。
- ゆえに両方向の入れ子が無く、順序制約は発生しない。

### 3.4 既知の受容残余は形が変わらない

`config_watcher.rs:150-156` のコメントが「`update_config` の直前に真を読んだ worker は、この破棄の後に挿入しうる（`ensure_…` は config 読みと icon lock を**別々に取る**）。これは受容する残余」と明記している。

移設後もこの窓は**同一の形で残る**（大きさも変わらない）: 移設前は engine `Mutex` が worker の読みと writer を排他し、移設後は config `RwLock` が排他する。どちらでも worker は「旧か新のどちらか」を原子的に見る。窓の原因は排他の欠如ではなく**読みと icon lock を別々に取ること**であり、そこは touch しない。

### 3.5 `#[expect]` の自壊は `-D warnings` の下でだけ赤くなる

`clippy.toml` 群 3 のコメント（2026-08-18 実測）:「`#[expect]` の不履行検知は `-D warnings` に依存する。…`cargo check` では診断そのものが評価されない（rustc は `clippy::` ツール lint の expectation を見ない）」。
ゆえに 3 件の `#[expect]` 削除は**移設と同一コミット**で行う。移行漏れの検知器として働くのは PostToolUse hook / CI の clippy 経路だけである。

### 3.6 群 3 の撤去条件は発火しない（カナリアは生き残る）

`clippy.toml` 群 3 は「最後の `#[expect]` が消える変更は、同じコミットでこの群のエントリと `REQUIRED_DISALLOWED_METHODS` の行を消し、`Engine::config` を `pub(crate)` へ落とすこと」を撤去条件に持つ。
**本件では `config_watcher.rs:88` の 1 件が残るので、この条件は発火しない**——群 3 のエントリも `REQUIRED_DISALLOWED_METHODS` も `Engine::config` の可視性もそのまま。issue コメントの「4 件 → 1 件」と一致する。

## 4. 条項の書き換え（`src-tauri/CLAUDE.md:57`）

**issue の「条項は 1 文になる」は近似である。** 正確には「否定 1 文＋射程外の 1 文＋既存の規律」が残る。この差を明記しておかないと、書き換えのときに偽の全称を作る（memory `universal-claim-fix-regenerates-itself` の型）。

### 4.1 死ぬ文（弁別子という装置に依存しているもの）

1. 「**例外は『イベントループスレッドを止めない場所での読み』である**」＋ worker（icon・folder）と platform スレッドの列挙
2. 「**弁別子はディレクトリでも頻度でもなく、走るスレッドである**」＋ 動機と判定の分離（「ユーザーが待っているか」を判定に使うな）
3. 「**どこで走るかは呼び出し元を辿って決めること**」（同じ関数が両方から呼ばれれば分類が変わる）
4. 「**列挙で覚えない**」＋ 非自明な 3 経路の解説（`on_event_loop` はインラインでも post でもイベントループスレッド／tao の window-event リスナー／`app.listen` は emit 元で決まる）
5. `get_instant_commands` が「`commands/` に在りながらフレームの中」だった説明（場所で切っていた頃の穴）
6. 「**hotkey の経路が先例である**」（`hotkey_toggle` が `on_event_loop` の中で `read_config` から読む）
7. 「**どこで走るかを呼び出し元から判定する責務は読者に残る**——機構が要求するのは分類の記録であって、正しい分類ではない」

### 4.1b 意味が変わる文（3b の P4 で発見した落ち・2 文）

この 2 文は §4.1 にも §4.2 にも入っていなかった。**どちらも「消える」でも「そのまま残る」でもない。**

8. **条項冒頭の全称文**「**config の live-read は `egui_shell::read_config` を通す。`engine.lock()` を経てはならない**」
   → **残り、かつ意味が強化される。** 現状この全称は §4.1-1 の「例外」節に打ち消されて限定的にしか成り立たないが、移設後は文字通り無条件に成り立つ（`config_watcher` の読みは**射程外**であって例外ではないので、全称性を損なわない）。
   **ただし文言は変える**——案 2（§5）で読み口が 2 つの名前になるため、`egui_shell::read_config` の逐語ではなく「`read_config` を通す（`&AppHandle` を持つなら `egui_shell::read_config`、`&AppState` を持つなら `AppState::read_config`）」の形にする。**ここが本件で条項が最も意味を変える 1 文である。**

9. 末尾の非太字文「却下した弁別子（頻度・ディレクトリ・…・外延の列挙）は `ADR-config-read-exception-discriminator`」
   → **残す。文言だけ書き換える。** 根拠は 2 つ。(a) 弁別子という装置を**置かないと決めた**以上、「かつて何で切ろうとして、なぜ全部駄目だったか」は否定の知識として現役である——装置が消えたことは引用を不要にせず、むしろ引用の意味を変える。(b) `ADR-adr-frozen-history` が残すと決めた**実在の辺**（生きた層 → ADR の短縮引用）を維持する。消すと同 ADR の被引用は `clippy.toml:91` の 1 件だけになる。
   文言は「却下した弁別子は…」から「**例外を切る弁別子を探した経緯と、案 A〜G をすべて却下した理由（この条項が例外を置かなくなった前史）は `ADR-config-read-exception-discriminator`**」へ。

### 4.2 生き残る文

1. 害の説明（worker が `engine.search` の間じゅう `Mutex<Engine>` を握る・40〜95 ms・`read_window_width` 43,939 µs・`PERFORMANCE.md` へのポインタ）
2. 「**`read_config` の中で lock を取る操作を書かないこと**」（§3.1 で理由の宛先が広がる）
3. 「**射程は読みだけである**」——書き込みは `update_config` の 1 本で engine 錠の内側に残す
4. 「**`config_watcher` が適用の前に読む旧 config は射程外である**」＋ 外れる理由（手続きであってスレッドではない）＋「`update_config` と同じ錠の内側で取る」を根拠にするな
5. 機構（#1122）の説明——`clippy.toml` の禁止と `#[expect]` の使い方
6. 「**規範は機構より広い**」＋ `clippy.toml` 群 3 コメントへのポインタ

**注意**: 4.2-4 は「例外」ではなく**射程外**である。弁別子（スレッド）を消しても射程外の 1 文は残るので、条項は「例外ゼロ」だが「無条件 1 文」ではない。

### 4.3 弁別子に言及する地点の全列挙

**訂正**: 本節の初稿は「弁別子の言い換えを持つ生きた文書は 0 件」と書いていたが、**これは `--include=*.md` だけで測った偽の全称だった**（3b もこの穴を突いていない——渡した命題 P5 自身が「文書」と書いており、走査範囲を md へ狭めていた）。`.rs` の doc コメントに**実物の言い換えが 1 件**ある。memory `grep-exclusion-drops-more-than-intended` の型を自分で踏んだ。

母集団の取り直し: `grep -rn "弁別子|条項の例外|#1032 条項|イベントループスレッドを止めない" --include=*.rs --include=*.toml --include=*.mjs --include=*.ps1 .` ＋ 既存の `.md` 走査。

**(a) 言い換えを持つ＝内容の書き換えが要る（1 件）**

- `src-tauri/src/commands/instant.rs:28-31`:
  「#1032 条項の**例外が名指すのは egui フレームの外で行う読み**（icon worker・folder worker・tray スレッド）であって `commands/` というディレクトリではない——**弁別子はフレームを止めるかである**」
  → 移設後、名指す例外が存在しなくなるので端的に偽になる。
  → **さらにこの文は現時点でも条項とずれている**: 条項の弁別子は「走るスレッド」であり、「フレームを止めるか」は `ADR-config-read-exception-discriminator` の**案 E（却下済み）**の言い方である。「ここに言い換えを置かない」という他所の規律が守られておらず、腐る前から誤っていた。書き換えでこの誤りごと消す。

**(b) ポインタだが「例外」「弁別子」の語が腐る＝語の追随（5 件）**

- `docs/architecture.md:231` 「例外の弁別子と射程は…当該条項が正本——ここに言い換えを置かない」
- `src-tauri/clippy.toml:89` 「規範の全文・害・例外の弁別子は…当該条項が正本である」
- `src-tauri/clippy.toml:90` 「却下した弁別子と、型で表せない理由（弁別子が否定形の述語だからである）は ADR…の案 A〜G」→ **歴史への参照なので残すが、現在形から過去形へ**
- `src-tauri/clippy.toml:145` 「この群も注釈も弁別子も要らなくなる」（撤去条件の末尾）
- `src-tauri/src/egui_shell/launcher_controller.rs:494` / `:765` 「射程と例外は／射程と例外の定義は…条項が正本」

**(c) 腐らない（変更不要）**

- `src-tauri/src/egui_shell/window_coordinator.rs:191` — 害の説明とポインタのみで、例外にも弁別子にも触れない。
- `PERFORMANCE.md:560` — #1032 の A/B 実測の記録。歴史であり、当時の読み口の名前を記す。
- `docs/adr/ADR-config-read-exception-discriminator.md` — **凍結**（`ADR-adr-frozen-history`）。編集しない。同 ADR の「検討していない代替」節が「#1123 で評価する」と書いているが、凍結の契約がこれに優先する（答えの置き場は §7-1 で決める）。
- `.superpowers/sdd/**` — `.gitignore` 済み（3b が実測）でガバナンス検査の母集団外。過去サイクルの計画・報告。

### 4.4 検知器の射程: `AppState::read_config` を足しても穴は開かない（実測）

`src-tauri/src/egui_shell/launcher_controller.rs:1910` のソーステキスト検査
`for forbidden in ["read_visible_rows(", "read_config("]` は、起動の入口 3 本に帰属する `read_config(` の出現を禁じる。

**案 2 で増える綴りは `state.read_config(` であり、needle `read_config(` を部分文字列として含む**ので、この検査は取りこぼさない。加えて同検査の doc が「受け入れ条件はこの帰属の規則そのものであって、対象外の件数ではない——件数を書くと正当な読みを 1 つ足すたびにこの散文だけが黙って腐る」と明記しており、ヘルパーが増えても散文は腐らない。
（`launcher_controller` は `&AppState` ではなく `AppHandle` を持つので、そもそも新しい綴りが出る見込みは無い。上は「出ても捕まる」の確認である。）

## 5. 設計判断: 読み口をどうするか

3 か所は `&AppState` を持ち、`egui_shell::read_config` は `&AppHandle` を要求する。3 案がある。

- **案 1: `state.config.read().unwrap()` を直接書く**（issue 本文が想定している形）
  - 利点: 追加の口を作らない。
  - 欠点: **製品コードの read guard 取得点が 1 → 4 になる**（§2.3）。かつ条項の肯定側「`read_config` を通す」が全称でなくなり、条項は否定形だけになる。guard の寿命をクロージャで縛る構造も失う（§3.1 の規律が純粋な文書契約へ落ちる）。
- **案 2: `AppState::read_config(&self, read: impl FnOnce(&Config) -> T) -> T` を足し、`egui_shell::read_config` をその委譲にする**（推奨）
  - 利点: guard 取得点は 1 のまま。クロージャ形で guard の持ち出しを**構造で**禁じる（`prefer-structural-over-documented-contract`）。条項の肯定側「read_config を通す」が全称のまま残る。`egui_shell::read_config` は「`AppState` 不在の面倒を見る」責務だけに縮む。
  - 欠点: 公開面が 1 つ増える（約 10 行）。名前が 2 つになる（`AppHandle` を持つなら `egui_shell::read_config`、`&AppState` を持つなら `AppState::read_config`）。
- **案 3: 3 か所の引数を `&AppHandle` へ替えて既存 `read_config` を使う**
  - 欠点: `resolve_opener` 系は `&AppState` を要求する公開 API で、tray 側の呼び出しも `&state` を渡している。シグネチャ変更が波及し、かつ `try_state` の fallback（理論経路）を新しく背負う。得るものが無い。

**採る案は 2。** 根拠は (a) §3.1 の規律を文書契約から構造へ落とせること、(b) guard 取得点を 1 に保つことが §2.3 の実測（現状 1 点）の維持であること、(c) 条項の肯定側を全称のまま残せること。

## 6. 影響ファイル一覧

| ファイル | 変更 |
|---|---|
| `src-tauri/src/state.rs` | `AppState::read_config` を新設（案 2） |
| `src-tauri/src/egui_shell/mod.rs` | `read_config` を `AppState::read_config` への委譲へ。doc の分担を書き直す |
| `src-tauri/src/commands/launch.rs` | `resolve_opener` / `resolve_all_openers` を移設・`#[expect]` 2 件削除・doc の錠の名前を書き換え |
| `src-tauri/src/commands/icon.rs` | `ensure_icon_cache_loaded_if_enabled` を移設・`#[expect]` 1 件削除・doc 書き換え |
| `src-tauri/src/commands/instant.rs` | doc の**言い換え**を書き換え（§4.3-a・唯一の内容変更） |
| `src-tauri/CLAUDE.md` | 条項の書き換え（§4.1・4.1b・4.2） |
| `src-tauri/clippy.toml` | 群 3 コメント 3 か所の語を追随（:89 / :90 / :145。機構の内容は不変） |
| `src-tauri/src/config_watcher.rs` | `#[expect]` の reason 文言追随（「弁別子が他の例外と違う」→ 例外という装置が無くなるため） |
| `src-tauri/src/egui_shell/launcher_controller.rs` | doc 2 か所（:494 / :765）の「例外」の語を追随 |
| `docs/architecture.md:231` | 「例外の弁別子と射程は」→ 射程だけを指す形へ |

**変更しない**: `SPEC.md`（挙動不変）、`PERFORMANCE.md`（性能不変・測らない）、`docs/adr/ADR-config-read-exception-discriminator.md`（凍結）、`snotra-core/`（`Engine::config` の可視性・`update_config` の位置は不変）。

## 7. 未解決の疑問（計画の未確定欄へ送るもの）

1. **新しい ADR を書くか。** 否定の知識（案 1・案 3 の却下、および「例外を残す」の却下）は生じている。一方 `docs/adr/` は「否定の知識が生じた決定のみ」であり、既存 ADR が代替案の景色を凍結保持している。→ 計画で判定する。
2. `config_watcher.rs:88` の `#[expect]` reason は「弁別子が他の例外と違う——スレッドではなく手続きゆえの射程外である」と書いており、**弁別子が消えると「他の例外」が存在しなくなる**。文言の追随が要るか。
3. `/plan-review` を `--deep` で回すか（ガバナンス文書＝条項の圧縮に当たる）。

## 8. 適用されるチェック（`AGENTS.md`「条件別チェック」）

- **`/race-check`**: 共有状態の読みの錠の取り方を変える（外側の `Mutex<Engine>` を外し、`RwLock<Config>` の read guard だけにする）。§3.1〜3.4 が主論点。
- **`.claude/rules/safety-nets.md`**: 規範文書の変更。自動配送されないので**手動参照済み**（本調査で全文読了）。
- **`npm run governance:check`**: ガバナンス文書（`src-tauri/CLAUDE.md`）と `.rs` の見出し参照を変更する。
- **`/dry-check`**: 関数（`AppState::read_config`）を新規定義する。
- **`.claude/rules/src-tauri.md`**: `.rs` 編集で自動配送。
- **非該当**: `/persistence-check`（永続形式不変）、`/state-check`（UI モード不変）、`/symmetric-check`（対称ペアの新設なし。ただし guard の生成/解放は §3.2〜3.3 で明示する）。

## 0. 【射程の変更・2026-08-18】4 か所へ拡大し、機構を lint からコンパイラへ移す

**ユーザーの裁定により、issue #1123 が「別勘定」として明示的に射程外としていた `config_watcher` の読みも移す。**
条件は「**例外がなくなってコードで自明になるなら**」（逐語）。3 か所だけの案では自明にならない残りが 3 つある旨を提示したうえで、最大値の枝が選ばれた。

以下、本ファイルの §1〜§9 は**3 か所案**として書かれている。技術的事実（§2 の現況・§3 の錠の分析・§4 の条項の逐条分割）はそのまま有効で、**射程と機構の設計だけがこの節で上書きされる**。

### 0.1 何が変わるか

| | 3 か所案（§1〜§9） | **4 か所案（採用）** |
|---|---|---|
| 移設する読み | `resolve_opener` / `resolve_all_openers` / `ensure_icon_cache_loaded_if_enabled` | ＋ `config_watcher` の旧 config 読み |
| `#[expect]` | 4 件 → 1 件 | **4 件 → 0 件** |
| `clippy.toml` 群 3 | 残す（語だけ追随） | **エントリごと削除** |
| `REQUIRED_DISALLOWED_METHODS` | 残す | **`Engine::config` の行を削除** |
| `Engine::config` | `pub` のまま | **`#[cfg(test)]` で閉じる**（下記 0.3） |
| 規範を守るもの | clippy の lint | **コンパイルエラー** |
| 条項に残る「例外」的な文 | 射程外 1 文（`config_watcher`） | **ゼロ**——分類そのものが不要になる |

`clippy.toml` 群 3 の**撤去条件が発火する**（3 か所案では発火しなかった）。同ファイルが逐語でこう書いている: 「**最後の `#[expect]` が消える変更**は、同じコミットでこの群のエントリと `REQUIRED_DISALLOWED_METHODS` の行を消し、`Engine::config` を `pub(crate)` へ落とすこと——そこから先は lint ではなく**コンパイルエラー**が規範を守り、この群も注釈も弁別子も要らなくなる。合図はマージ済みの事象（最後の注釈が消えること）であって、issue の開閉ではない。」

### 0.2 `Engine::config` の呼び出し元（自分で測った）

`grep -rn "\.config()" --include=*.rs .` ＋ `grep -rn "Engine::config\b" --include=*.rs .`（UFCS は 0 件）:

- `snotra-core/src/engine.rs:476` / `:485` / `:590` — **すべて `#[cfg(test)] mod tests`（332 行から）の中**
- `src-tauri/src/commands/icon.rs:21` / `launch.rs:111` / `launch.rs:166` / `config_watcher.rs:91` — 移設対象の 4 件

→ **移設後、製品コードからの呼び出しはゼロ**になる。`snotra-core/tests/`（別 crate）にも無い。

### 0.3 `pub(crate)` ではなく `#[cfg(test)]` を採る（撤去条件の文言を修正して適用する）

**`pub(crate)` へ落とすと `dead_code` で赤くなる。** 移設後の唯一の読み手は snotra-core 自身のテストであり、lib ターゲット（`cargo clippy --workspace --all-targets` が非テストでもビルドする）では未使用になる。`-D warnings` の下でこれは error である。撤去条件を書いた時点ではこの帰結が織り込まれていない。

**`#[cfg(test)]` が正しく、しかも同じリポジトリに先例がある。** `snotra-core/src/search.rs:501-502`:

```rust
/// **読み手は crate 内の検知器だけなので `#[cfg(test)]` で閉じる。**
/// 計測ハーネス（別 crate）が読むのは `Engine::sorted_by_path` であってこちらではない
/// ——製品から届く綴りを増やさないほうが、禁止を 1 つ足すより強い。
#[cfg(test)]
pub(crate) fn sorted_prefix_len(&self) -> usize {
```

`#[cfg(test)]` は `pub(crate)` より**強い**——製品ビルドにそのメソッドが**存在しない**ので、`src-tauri` からは綴ることすらできず、違反は型エラーですらなく**未定義メソッド**になる。`dead_code` も立たない（テストビルドでのみ存在し、そこでは使われている）。

### 0.4 4 か所目（`config_watcher`）の移設が安全である根拠

- **原子性は今も無い。** 現行 `let old_config = state.engine.lock().unwrap().config().clone();` は `.clone()` の一時値なので**文末で guard も engine 錠も落ちる**。書き込み（`update_config`）は同じ関数の後段で engine 錠を取り直す。読みと書きの間の窓は移設前から在り、移設で**広がりも縮みもしない**。
- **値は同じ。** 条項自身が「`AppState.config` は同じ `Arc` なので**そちらから読んでも値は同じ**であり、射程外なのは分類の判断であって構造上の制約ではない」と明記している。
- **`&AppState` は既に手にある**（`config_watcher.rs:87` の `let state = app.state::<AppState>();`）。
- **read guard の中で `Config` 全体を clone する**ことになるが、(a) 現行も engine 錠の内側で同じ clone をしており、(b) read guard は共有なので他の読み手を止めず、(c) 唯一の writer は自分自身である。**guard 内 I/O ではない**（純粋な確保）。

### 0.5 射程外という分類そのものが不要になる

3 か所案では `config_watcher` の読みを「読みではなく適用手続きの一部」として**射程外**に置く 1 文が残った。4 か所案ではその読みも `read_config` を通るので、**差別的な扱いが 1 つも無くなり、分類する必要が消える**。条項に残るのは:

1. 「config の読みは `read_config` を通す。`engine.lock()` を経てはならない」（**無条件**）
2. 害の説明と `PERFORMANCE.md` へのポインタ
3. 「`read_config` の中で lock も I/O も取らない」
4. 「射程は読みだけである」——書き込みは `update_config` の 1 本で engine 錠の内側に残る
5. 機構: `Engine::config` は `#[cfg(test)]` ゆえ製品から**呼べない**（lint ではなくコンパイラ）
6. 「規範は機構より広い」——`Engine` の他メソッド（`search` / `recent_history` / `begin_index_drain`）を錠越しに呼ぶ形と、`engine.lock()` 越しに `config_handle()` を取り直す形は依然として同じだけ待つ（**受容する残余**。これは 4 か所案でも消えない）
7. **`ADR-config-read-exception-discriminator` への短縮引用**——「例外を切る弁別子を探した経緯と、案 A〜G をすべて却下した理由（この条項が例外を置かなくなった前史）」として残す。
   **これは装飾ではない**: 独立導出 v2 が実測したとおり、生きた層から同 ADR への引用は**全リポジトリで 2 件しかなく（本条項と `clippy.toml:90`）、4 か所案ではその両方が消える**。`G-adr-citations` は実在の一方向検査なので**孤立は沈黙する**。条項に 1 件残せば生きた辺が保たれる（新 ADR からの ADR → ADR 引用も別に張る）。

### 0.6 検査側の追随（実測）

- `scripts/governance/checks/G-clippy-disallowed.mjs:66` の `REQUIRED_DISALLOWED_METHODS` から `Engine::config` の行を削除する。
- `scripts/governance/checks/G-clippy-disallowed.test.mjs:34-35` に**同じパスの緑 fixture がある**。カナリアから消しても fixture は上位集合なので緑のままだが、**実在しない群を指す死んだ行になる**ので同じコミットで消す。fixture の doc が言う「群を跨いで持つ」性質は、群 1（egui 7 件）と群 2（`sorted_by_path`）で保たれる。
- **これらはセーフティネットの変更である**（`.claude/rules/safety-nets.md`）。ただし `clippy.toml` 群 3 の撤去条件が**自ら手順を指定している**ので、新規の判断ではなく既定の手順の実行である。

## 9. 敵対的調査（Step 3b）の結果

サブエージェント 1 体（`general-purpose` / `model: sonnet`）。全文は `workspace/adversarial-1123.txt`。
壊せた 1 件・壊せなかった 5 件・⚠️ 2 件。

### 壊せた（採用・反映済み）

- **P4（§4 の 2 分割に落ちが無い）** ❌
  条項本文を `**…**` で機械分割すると太字節は **25 個**あり、そのうち **2 文がどちらのリストにも入っていなかった**——(1) 条項冒頭の全称文、(2) 末尾の ADR 参照文。
  とくに (1) は**本件が最も意味を変える文**である（例外に打ち消されていた全称が無条件になる）。§5 で「肯定側が全称のまま残る」と書きながら、逐条リストに載せていなかった。
  → **採用。§4.1b を新設し、2 文の運命と書き換え後の文言を確定した。**
  → 機序の裁定: レビュアが添えた「太字 = 守る指示という `src-tauri/CLAUDE.md` 自身の記法規約に従えば同格」という説明も、当該ファイル冒頭の記法規約を自分で読んで確認した。採るのは所見だが、この機序も成立している。

### 壊せなかった（宣言）

- **P1**（3 か所は config 読みのためだけに engine 錠を取る） ✅ — 3 関数とその上位呼び出し元を全数 grep。`src-tauri/` に `tests/` は無く、`#[cfg(test)]` もこの 3 関数を呼ばない。
- **P2**（icon の 2 値は単一 guard で原子的・`icon_cache_cap()` は純 CPU） ✅ — `snotra-core` 側の実装を実読し、錠も I/O も無いことを確認。
- **P3**（新しい deadlock も優先度逆転も生じない） ✅（コア）— **`AppState.config` と `Engine.config` は同じ `Arc<RwLock<Config>>`** なので、移設は錠の**種類を替えるのではなく、外側の `Mutex<Engine>` ラッパーを外すだけ**である。新しい lock オブジェクトは 1 つも増えない。read/write の全地点も独立に列挙し §2.3 と一致。
  → **この指摘で §3 の見出しの表現を正す**: 「錠を替える」ではなく「**外側の `Mutex` を外す**」が正確である。
- **P5**（弁別子の言い換えを持つ生きた文書は 0 件） ✅ — 複数綴りで独立に grep。加えて `.superpowers/sdd/` が `.gitignore` 済みでガバナンス検査の母集団外であることを実測。
- **P6**（群 3 の撤去条件は発火しない） ✅ — 撤去条件の主語（「最後の `#[expect]`」）が本件で満たされないことを逐語で確認。`#[expect]` 4 件も独立に数え直し。
- **測定環境の疑い** — 実 `config.toml` を読み、`openers` が非空・`show_icons=true` であることを確認（＝ 3 か所の読みは実際に意味を持つ枝を通る）。config の書き手が `config_watcher.rs:146` の 1 本だけであることも独立確認。反証なし。

### ⚠️（確信が持てないもの・両方とも扱いを決めた）

- **`std::sync::RwLock` の公平性の一般化**（§3.1）— 「待ち writer が後続 reader を塞ぎうる」は Rust std が文書で保証しておらず、独立の実測もしていない。
  → **採用し、§3.1 を分割した。** 禁止の根拠は「read guard を握れば writer は必ずその間待つ」という**実装非依存の理由**に置き換え、公平性の話は「さらに悪化しうる形（未実測）」として根拠から外した。所見を狭めるのではなく、**狭める側の前提を測っていないので、結論を前提の弱い方へ載せ替えた**（memory `narrowing-a-finding-rests-on-an-unverified-premise` の型を避ける）。
- **P4 の付随所見（末尾 ADR 参照文）の重大度** — レビュア自身が確信を持てないとした。
  → **主エージェントが裁定した**（§4.1b-9）。重大度は「軽微だが判定が要る」で、残す方向で確定。
