# plan: #663 — /race-check スキルを egui worker 並行モデルへ全面改訂する

**この計画は合意ゲートの提示物である。** エージェント設定（skill）の変更ゆえ、ルート `CLAUDE.md` 最重要ルール 3 と issue #663 本文により、**本計画への合意を得るまで `.claude/skills/race-check/SKILL.md` を編集しない**。`/start-issue` は本ファイルのコミットで停止する。

---

## 0. 要合意事項（実装前に決める 3 点）

### 決定 1 — トリガー述語（最大の設計判断）

現行の述語「**async 関数を新規追加・変更したとき**」は、対象コードベースで**発火しない**（`.await` は src-tauri 全体で 2 箇所・research.md §3.5）。放置すればスキルは呼ばれないまま残る。

| 案 | 述語 | 評価 |
|---|---|---|
| **A（推奨）** | 「**worker スレッド（`std::thread::spawn` / `async_runtime::spawn`）・channel・フレーム drain・窓をまたぐ共有状態を追加/変更したとき、および async 関数を追加/変更したとき**」 | 現行の 4 経路すべてで発火し、updater の async も拾う。語が長いのは述語が実体を写しているため |
| B | A から async 節を落とす | updater / install の 2 箇所が視界から落ちる。短くなるが穴が開く |
| C | 現状維持（async のみ） | 発火しないスキルが残る＝ issue の目的を達しない |

**推奨は A。** 理由: `.await` は消滅していないので削除ではなく**降格**が正しく（advisor 指摘）、かつ「呼ばれない述語」を作らないため。

**述語には「Tauri listener / emit の追加・変更」も含める**（plan-review Step 2 の実測による追加）——`app.listen` のコールバックは**emit 元スレッド上で同期実行される**ため、listener の追加は Win32 メッセージループスレッドや config_watcher スレッドから UI 状態を触るコードの追加と同義である。channel を持たないので「worker + channel」だけの述語では発火しない。

### 決定 4 — `.claude/rules/src-tauri.md` へトリガー行を足すか（**要合意・plan-review Step 2b の指摘**）

`.claude/rules/` は**対象ファイルを触ると自動配送される唯一の機構的起動経路**である。`.claude/rules/snotra-core.md` と `snotra-core-search.md` は `/cache-check`・`/persistence-check` への行を「トリガー → 検査」節に持つ（前例あり）が、**`src-tauri.md` には race-check の行が無い**（実測）。改訂後スキルの守備範囲は `src-tauri/src/egui_shell/` そのものであるため、**行が無い限りスキルは「エージェントが思い出したときだけ」起動する**。

**推奨: 足す（1 行・写像の SSOT は `AGENTS.md` ゆえ rules 側は参照の形にする）。** ただしこれも `.claude/` 配下＝エージェント設定の変更ゆえ合意対象に含める。

**面積 ratchet の制約（実測・`npm run governance:check` の evidence 行）**: 常時ロード（ルート `CLAUDE.md` + `AGENTS.md`）は **216/216 行で余裕ゼロ**。ゆえに決定 1 の述語差し替えは**行数を増やさない置換**でなければならない（1 行の文言を長くするのは可・行を増やすのは不可）。rules は **150/173 行**で 23 行の余裕があり、決定 4 の 1 行追加は通る。

### スコープ外の明示宣言（無言で放置しない）

- **`.claude/agents/code-reviewer.md`（2d のリソースライフサイクル節・Phase 3 の「SolidJS 固有」）に同種の SU7 残骸がある**（実測）。スキル名を含まないため `race-check` の grep では出ず、概念でしか拾えない。**本 issue のスコープ外とし、follow-up issue を立てる**（`docs/superpowers/specs/2026-07-25-...` が「#663 のとおり参照しない」と明示宣言した前例に倣う）
- **`docs/superpowers/specs/` 配下の `/race-check` 言及**（3 ファイル）は本 issue 完了で陳腐化するが、**編集しない**——#589 で非規範化された歴史資料であり、`governance-check` の参照検査の母集団からも除外されている（実測）。**陳腐化を受容したと明記する**

### 決定 2 — 適用範囲

**`src-tauri`（`egui_shell` 中心）を主対象とし、`snotra-settings` の worker（`tabs/backup.rs`・`tabs/common.rs` の `std::thread::spawn` + rfd）も対象に含める。** 機構カテゴリ（後述）は crate 非依存に書けるため範囲を広げてもスキル本文は増えない。**含めない**と、設定バイナリ側の worker 追加時に検査が空白になる。

### 決定 3 — 例示の耐久性（旧スキルの死因への対処）

旧スキルは消えた識別子（`dispatchQueryInput`・`searchLane`）をハードコードして死んだ。同じ脆さを再生産しないため:

- **Step の見出し・判定軸は「機構カテゴリ」で書く**（世代 token / チャネル所有権 / 重複 spawn ガード / wake 義務 / hide 跨ぎ / 同一フレーム live-read）
- **現行シンボルは 1 カテゴリにつき最大 1 件、「例（2026-07 時点）」と明示ラベルを付けて併記する**
- **位置は `file:line` で書かない。シンボル名 + grep レシピで書く**（`.claude/rules/src-tauri.md` の #588 規律を踏襲）
- **事実（wake 義務・hidden 中 `update()` 不走行）はスキルに写さず `src-tauri/CLAUDE.md` の節名で参照する**（AGENTS.md「正本を 1 か所に定め他は参照へ」）

---

## 1. 変更ファイル一覧

| ファイル | 変更内容 | 決定 1 が A/B のとき | C のとき |
|---|---|---|---|
| `.claude/skills/race-check/SKILL.md` | **全面改訂**（下記 §2）。冒頭の #532 注記（現行 11 行目）を**削除**——改訂後に残すとスキルが自己矛盾する | 必須 | 必須 |
| `AGENTS.md`「条件別チェック」表の `async 関数を追加/変更` 行 | 述語を決定 1 の文言へ | 必須 | 不要 |
| ルート `CLAUDE.md`「利用できるスキル」表の `/race-check` 行 | 説明文 + 呼び出し例を egui 実例へ差し替え | 必須 | 一部 |
| `.claude/skills/start-issue/SKILL.md` Step 5a の表 | トリガー条件を述語へ揃える | 必須 | 不要 |
| `.claude/rules/src-tauri.md`「トリガー → 検査」節 | **決定 4 が「足す」なら** 1 行追加（自動配送による起動経路の確保） | 決定 4 次第 | — |
| `.claude/skills/retrospective/SKILL.md` | **変更しない**（スキル名の列挙のみで述語を書いていない・実測済み） | — | — |
| `.claude/skills/implement/SKILL.md` | **変更しない**（`AGENTS.md` の表へ委譲しており述語を複製していない・実測済み） | — | — |

コード（`*.rs`）・`SPEC.md` の変更は**無い**（挙動変更を伴わない・SPEC 同期不要）。

## 2. 改訂後 SKILL.md の構成（案）

frontmatter: `description` を決定 1 の述語へ、`argument-hint` を egui 実例へ（例: `'spawn_folder_load: FolderMsg を folder_rx へ送る worker'`）。`allowed-tools` は `Read`/`Grep`/`Glob` のまま（変更しない）。

| 節 | 内容 | 旧スキルとの対応 |
|---|---|---|
| 背景 | 「`await` の前後で世界が変わる」→「**送信から drain されるまでの窓で世界が変わる**」へ主題を置換。実行文脈が 2 種（イベントループ / worker）しかないこと、UI 状態に触れるのはイベントループだけ、という土台を先に置く | 「背景」を置換 |
| Step 1 — 並行境界の列挙 | `await` 地点の列挙を **「境界の列挙」** へ置換。境界 = ①worker spawn 地点 ②channel send 地点 ③drain 地点 ④managed state（`Mutex`/`Atomic`）の書き / 読み ⑤**Tauri `listen` コールバック**（emit 元スレッドで同期実行される＝実質 worker・plan-review で発見） ⑥**channel を経由しない worker**（設定サイドカー監視・背景再スキャン等が managed state / 窓 API を直接叩く） ⑦`.await` 地点（**降格して残す**） | Step 1 を置換 |
| Step 2 — 各境界の staleness 機構の同定 | 代表 4 型（**世代 token / チャネル所有権 / 重複 spawn ガード / level-triggered**）のどれを採るか。判定軸は「channel が view 寿命の共有か per-request か」——共有なら token が要り、per-request なら所有権 drop で足りる。**「4 型で網羅」とは書かない**——実在する 5 型目（アプリ全体スコープの単調カウンタをフレーム毎に diff する型）が既にあり、全称主張は嘘になる。加えて**世代カウンタは読み側ではなく書き側の全サイト列挙が義務**（「総入れ替えするすべての地点が世代を進めたか」——読み側の一致確認だけでは漏れる） | Step 2 を置換 |
| Step 3 — 窓の間に起きうる事象の列挙 | 送信〜drain の間に到達しうる状態変更経路（打鍵・Escape・reset-on-show・hide・config 適用・index 完了・クリック逆流）。旧 Step 3 の「経路 → トリガー → 影響する状態」表の骨格は保ち、中身を egui 経路へ入れ替える | Step 3 を置換（骨格流用） |
| Step 4 — 5 観点での検証 | **4a wake 義務**（状態を変えた後に**次フレームを起こす者がいるか**——回数ではなく到達性。自窓 Context を持つ場所は `ctx.request_repaint()`、外部スレッド・別窓は `WindowWaker`。**managed state / 外部ハンドルに `egui::Context` の clone を置いていないか**もここ——#671 PR D で worker の join が止まった実在の破れ）／**4b staleness 適用**（token 一致・所有権 drop・重複 spawn ガード・世代の**書き側全サイト**）／**4c hide 跨ぎ**（`request_repaint_after` は可視中のみ。**「reset-on-show でクリアされるか、されない理由が書かれているか」**——意図的な非クリアが実在するため「クリアされること」を要件にすると誤検出になる。**view-local だけでなく managed state も列挙対象**）／**4d 順序不変条件**（drain と reset の前後・格納と wake の前後・`main_visible` と窓操作の前後）／**4e 同一フレーム live-read**（config を後段で読み直さない・snapshot を `self.` へ持たない・**逆にフレームを跨いでキャッシュして hot-reload を殺していないか**） | 旧 4a〜4d（入力ガード / staleness / ローカルキャプチャ / 再入）を置換。**旧 4c（`let` の `const` キャプチャ）は Rust の所有権と `move` クロージャが構造的に処理するため概念ごと消滅**し、空いた枠に 4a（wake 義務）が入る。**再入ガードは 4b の「チャネル所有権 = single-flight」に吸収**（`launching.is_some()` 拒否がその実体。解除経路は enum の**全アームを尽くしたか**という形で問う） |
| Step 5 — 判定マトリクス | 外形は現行のまま（境界ごとに 5 観点 + 総合判定） | 流用 |
| 出力 | 「根拠の規律」（全判定に `file:シンボル` か grep 結果を付ける・未確認は `[要確認]`）はそのまま保つ | 流用 |

**注記の扱い**: 現行 11 行目の #532 読み替え注記は削除する（本改訂がその follow-up 本体であるため）。

## 3. 実装順序

- **Phase 0（合意ゲート）**: 本計画のレビューと決定 1〜3 の合意。以降は合意後にのみ着手
- **Phase 1**: SKILL.md 全面改訂（§2）
- **Phase 2**: フォールトインジェクション（§5）とその指摘に基づく SKILL.md 修正。**巡ごとにコミットする**（中断前提・#431）
- **Phase 3**: ガバナンス面の同期（`AGENTS.md` / ルート `CLAUDE.md` / `start-issue`）+ `npm run governance:check`
- **Phase 4**: 遡及テスト（§6）と結果の記録

Phase 3 を Phase 2 の後に置くのは、述語の最終文言が Phase 2 の指摘で変わりうるため（先に写すと二度手間になる）。

## 4. 不変条件（この変更が守るもの）

1. **スキルは事実の正本にならない**——wake 義務・hidden 中 `update()` 不走行・ロック最小化は `src-tauri/CLAUDE.md` / 各 `//!` が正本。スキルは**節名で参照**し内容を写さない。破れたときの症状: 正本を更新してもスキル内の写しが古いまま残り、読者が誤った不変条件で検査する
2. **全称表現は前提条件とセットで書く**（AGENTS.md「検証の作法」）。「worker はすべて repaint する」のような主張は、実装より強くなった瞬間に嘘になる。改訂稿の全称文は既存 4 経路すべてに当てて検算してから残す
3. **述語は発火する**——決定 1 の文言は、直近サイクルで実際に起きた変更（#646 PR2 の results 窓追加・#671 PR D の `WindowWaker` 導入・#673 の `VisualSnapshot`）に当てて「発火したはずか」を確認してから確定する
4. **スキル名・ファイルパスは変えない**（`/race-check`・`.claude/skills/race-check/SKILL.md`）。改名すると `AGENTS.md`・`CLAUDE.md`・`start-issue`・`retrospective`・過去 spec の参照が一斉に腐る。**中身の置換であって移設ではない**

## 5. 検証方針（1）— 規範のフォールトインジェクション

`.claude/rules/safety-nets.md` により、規範は実行して測れないため「**回避しようとする読者**」で検証する。**停止条件を先に決める**（#488）:

### 合格条件（通してはならないシナリオ）

改訂稿を読んだ読者が、次のいずれかを `[安全]` と判定できてしまってはならない:

| # | シナリオ | 対応する観点 |
|---|---|---|
| S1 | worker が channel へ send するが `request_repaint()` を呼ばない（次の無関係な入力まで UI が stale） | 4a |
| S2 | view 寿命の**共有** channel を新設し、token も所有権 drop も持たない（旧 nav の結果が新 nav へ適用される） | 4b |
| S3 | in-flight 状態（`Option<...>`）を新設するが reset-on-show でクリアしない（hide を跨いだ遅着が再 show 窓を撃つ） | 4c |
| S4 | 新しい drain を `reset_pending` 消費の**前**に置く | 4d |
| S5 | 同一フレーム内で config を 2 度読む（`show_icons` を冒頭と後段で別々に読む型） | 4e |
| S6 | 毎フレーム同一 path 集合へ spawn する（重複ガードなし・thread pileup） | 4b |
| S7 | 外部スレッドから起こすために managed state へ `egui::Context` の clone を置く | 4a |
| S8 | `tauri::async_runtime::spawn` 内の `.await` をまたいで状態を復元する（**降格した await 軸が生きているかの検査**） | 4b |
| S9 | **Tauri `listen` を新設し、コールバック内で UI 状態を書く**（emit 元が Win32 メッセージループスレッドや config_watcher スレッドである自覚なしに）。channel が無いため「worker を足していない」と自己申告されうる | Step 1⑤ |
| S10 | **窓をまたぐ共有スロットが世代を運ばない**（例: クリック index を裸の `usize` で渡し、受け側フレームで行が総入れ替えされていても `.get(i)` の境界チェックだけで適用する）。**改訂稿の Step がこの問いを機械的に浮上させないなら構成が緩すぎる**——plan-review が実在コードで指摘した構造的窓であり、スキルの「歯」の試金石 | 4b + 4c |

### 読者クラス（2 種を必ず両方走らせる・#488）

- **手を抜く読者**: Step を飛ばす・「たぶん安全」で `[OK]` を書く・grep せず記憶で答える。→ 逃げ道を探させる
- **規則を全部守る読者**: 文面どおり忠実に実行し、それでも S1〜S8 を取りこぼす経路を探す。**忠実な読者が誤る経路は手を抜く読者からは見えない**

### 上限巡数と受容する残余

- **上限 2 巡**（各巡で 2 クラス）。逃げ道は塞ぐたびに隣へ移るため 1 巡では終わらない（#489）が、規範文書に無限反復はできない
- **2 巡終了時点で残る指摘は「受容する未対応リスク」として SKILL.md 末尾ではなく PR 本文に列挙する**（スキル本文に免責を書くと読者が免責を根拠に検査を省く）
- サブエージェントへ委譲する場合、**メインの system prompt にしか無い事実は明示的に渡す**（委譲はコンテキストを継承しない）。ただし本 Phase では検査対象（SKILL.md）を**変更しながら検査を走らせない**（#489）——巡ごとに「凍結 → 委譲 → 回収 → 修正」の順を守る

## 6. 検証方針（2）— 遡及テスト（受け入れ基準・省略不可）

「よく書けている」と「実際に発火する」を分ける唯一の検査。`safety-nets.md`「検査の入力集合を、具体対象で検算する」をスキルへ適用する。

**過去に実在した 3 件の並行性欠陥に改訂稿を当て、Step が当該欠陥を surface させるかを確認する**（できなければ Step を修正する）:

| 事例 | 実在の欠陥 | 期待される surface 先 |
|---|---|---|
| PR #647 e746826 | toast dismiss で状態を変えたのに `request_repaint()` が無く、次の無関係な入力まで stale 表示 | 4a |
| #671 PR A′ | `main_visible` を `results.hide()` の後に落とすと、隙間フレームが results を再表示（main が隠れたまま results だけ最前面に残る） | 4d |
| #673 | 同一フレーム内で config の新旧が混ざる（新 `font_size` を旧行高で描く） | 4e |
| #632 Important 3 | `selected` の比較だけでは「結果が丸ごと変わったが selected は偶然 0 のまま」を検出できない（`snapshot_generation` の導入で解決） | 4b（世代の書き側全サイト） |
| #671 PR D | managed state に置いた `egui::Context` の clone が `RepaintScheduler` の Arc を窓の `Destroyed` 越しに握り、worker の停止・join を止める | 4a |
| #636 Finding A | folder ロード滞留中に stale 行を activate（`folder_load_pending` ガードで解決） | 4c |

さらに**逆方向の検算**（両方向を示す・safety-nets.md）:

- **入るべきものが入る**: 上記 3 件が Step 1 の「境界の列挙」に現れること
- **入るべきでないものが入らない**: 純粋核（`search_state.rs::interpret`・`layout.rs::Metrics`・`lifecycle.rs::plan_hotkey`）は並行境界を持たないため、Step 1 が空を返して「非該当」で終わること（全ファイルで発火する述語は誤爆で信用を失う）

## 7. 検証コマンド

| カテゴリ | コマンド | 理由 |
|---|---|---|
| ガバナンス | `npm run governance:check` | スキル表・参照実在・rules glob。**`.md` 編集には PostToolUse hook が割り当てられていない**ため沈黙は合格を意味しない（#497）——明示実行が必須 |

**governance:check の限界（plan-review Step 2 の実測）**: スキル表の検査（G8）は `CLAUDE.md` の表と `.claude/skills/*/SKILL.md` の**存在の双方向照合だけ**であり、説明文・frontmatter の `description` の**内容**は見ない。`AGENTS.md`「条件別チェック」表も検査対象外。参照実在検査（G3）は拡張子付きパスのみが対象で、`` `/race-check` `` のようなスキル名参照は母集団に入らない。**ゆえに「4 箇所の述語が互いに一致しているか」は機構では検知されない**——Phase 3 の完了判定を governance:check の green に依拠させず、4 箇所を人手で並べて照合する（この照合手順を Phase 3 の作業項目として書く）。
| コード | **なし** | `*.rs` を変更しないため clippy/test は非該当 |
| smoke | **なし** | 挙動変更なし |

## 8. SPEC.md 更新要否

**不要。** エージェント運用の変更であり、プロダクトの意図（`SPEC.md`）に影響しない。

---

## セルフレビュー

### Step 5a — check スキルの適用可否

| スキル | 判定 |
|---|---|
| `/plan-review` | **実施済み**（Explore 2 体 + Step 2b 独立再導出 1 体）。結果は下記「plan-review 結果の反映」 |
| `/symmetric-check` | 非該当（対称ペアを持つコードパスに触れない・`*.md` のみ） |
| `/cache-check` | 非該当 |
| `/persistence-check` | 非該当（on-disk 形式に触れない） |
| `/state-check` | 非該当（UI モード・ガード条件に触れない） |
| `/race-check` | **非該当かつ非実行**——本計画の対象が当のスキル自身であり、現行版は #663 のとおり発火しない（同 spec 2026-07-25 の判断と同じ） |

### Step 5b — plan-review が扱わない 3 観点

1. **境界条件**: §6 の逆方向検算（純粋核で発火しないこと）が「述語が広すぎる」側の境界を、S8（await 軸の残存）が「狭すぎる」側の境界を担う。加えて **worker を持たない `*.md` / 設定ファイル変更で発火しない**ことを確認する
2. **シンプル化の挑戦**: 「Step 5 段 + 観点 5 つ」は旧版（Step 5 段 + 観点 4 つ）とほぼ同規模で、増分は観点 1 つ。新たな状態・機構は導入しない（スキルは読むだけ・`allowed-tools` 据え置き）。**削る候補**として決定 2 の範囲拡大を検討したが、機構カテゴリが crate 非依存ゆえ本文が増えないため維持する
3. **破壊不変条件 + 検知手段**: 本変更で「壊れたら即アウト」は **(i) スキルが発火しなくなる**（検知: §6 の遡及テスト + 決定 1 の述語を直近 3 サイクルの変更に当てる検算）、**(ii) スキル表・参照の不整合**（検知: `npm run governance:check`・PR CI の governance-check job）、**(iii) 事実の写しが正本と乖離**（検知: 不変条件 1 の「節名で参照し内容を写さない」規律。機構的検知は無く、**受容する残余**として明記する）

### plan-review 結果の反映

**要対処（すべて本計画へ反映済み）**

1. **「worker → UI は channel 経由の 4 経路だけ」は不成立**——`app.listen` のコールバックは emit 元スレッドで**同期実行**される（一次資料で確認）。Win32 メッセージループスレッドが `show_egui_main` / `hide_egui_main` を直接呼び、config_watcher スレッドが `Mutex` へ直接書く。channel を持たない worker（設定サイドカー監視・背景再スキャン）も同様。→ **Step 1 に境界 ⑤⑥ を追加、述語に listener を追加、S9 を合格条件に追加**
2. **「worker は送信のたびに repaint」は全称主張として不成立**——アイコン経路は複数 send をループで撃ち repaint はループ外 1 回。→ **4a を「到達性（次フレームを起こす者がいるか）」で書く**
3. **「staleness は 4 型」は網羅でない**——5 型目（アプリ全体スコープの単調カウンタのフレーム毎 diff）が実在。→ **「代表 4 型・網羅ではない」と明記**
4. **「in-flight は reset-on-show でクリアされる」は例外あり**——アイコン系は `ResultsView` 側で `reset_pending` を消費せず自然収束に依存、`pending_hotkey_failure` は意図的に非クリア。→ **4c を「クリアされるか、されない理由が書かれているか」へ**
5. **自動配送の起動経路が無い**——`.claude/rules/src-tauri.md` に race-check の行が無い（`snotra-core.md` には `/cache-check` の行がある）。→ **決定 4 として合意対象に追加**

**軽微な懸念（反映済み）**

- governance:check は存在照合のみで述語の一致を検知しない → §7 に限界と人手照合を明記
- `CLAUDE.md` スキル表の**右端の呼び出し例**（`executeInstantCommandSelected: await ...`）は視線から外れやすく、かつエージェントが複製する雛形ゆえ文言より影響が大きい → §1 の該当行に「説明文 **+ 呼び出し例**」と明記済み
- 他スキルに「判定マトリクス」という独立節は実在せず、実態は最終 Step 内の判定表 → §2 の Step 5 はその形に合わせる

**独立導出との差分（Step 2b）**

- **漏れ（導出 ∖ plan）**: (a) `.claude/rules/src-tauri.md` のトリガー行不在（→ 決定 4）、(b) `.claude/agents/code-reviewer.md` の SolidJS 残骸（→ スコープ外を明示宣言 + follow-up issue）、(c) `docs/superpowers/specs/` の陳腐化（→ 受容を明記）、(d) 遡及テスト事例 3 件の追加（#632 / #671 PR D / #636）、(e) 旧 4c の消滅を「削除」ではなく「4a への置換」と扱う論点
- **スコープ過剰（plan ∖ 導出）**: 無し。独立導出も 4 箇所の述語同期・スキル本体全面改訂を必須と結論した
- **一致（完全性の能動的証拠）**: 参照箇所の列挙（`AGENTS.md` / ルート `CLAUDE.md` / `start-issue` / SKILL.md 本体の 4 箇所）、`retrospective` と `implement` が変更不要であること、`#532` 注記の削除、シンボル名ハードコードを避ける方針、規範のフォールトインジェクション（2 読者クラス・停止条件先決め）、遡及テストの必要性——いずれも独立に再一致した
- **追加で得た自由度**: race-check の Step 番号を外部から参照している箇所は **0 件**（半角 `Step [0-9]` と全角「ステップ」の両方で確認）。**Step 構成は自由に組み替えてよい**

### 残る留保

- 決定 1〜4 は**合意待ち**。合意内容によって §1 の変更ファイル数（1〜5）が変わる
- **改訂稿を書いた後、その本文自体に対して S1〜S10 の検算をもう一度行う**——本計画の合格条件は計画時点のコード理解に基づくため、稿が具体化した時点で再照合が要る
</content>
