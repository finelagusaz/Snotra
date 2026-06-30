# research.md — issue #404 SPEC.md の最新化

## issue の要約

`SPEC.md` のセクションによっては内容が古く、**実装済みの機能が「将来やること」と書かれている**（例: インスタントコマンド）。あるべき姿は「実装済みのものは実装済みになっていること」。
備考: これから実装予定のものの管理方法（GitHub Issue 集約 / 作業ブランチの SPEC など）は **別途検討** ＝本 issue のスコープ外。

→ 本 issue のスコープ: **SPEC.md の記述を as-built（現行実装）に揃える**。具体的には「実装済みなのに未来/未実装と書かれている」「記述が現行コードと factual に矛盾する」箇所を是正する。

## 調査方法

- SPEC.md（1005 行）を §0–§8 / §9–§17 / §18–§20 の 3 範囲に分割し、独立した Explore エージェント 3 体で並列監査。各エージェントには「実装済みなのに将来/未実装 framing」「現行コードと明確に矛盾する factual な記述」のみを厳格に報告させ、表現の好み・completeness 提案は除外。
- 高リスク候補（§20 自動更新の実在性、§17.1 V1 削除の意図性、§7.4 の Win+* 拒否の意図性）は **メイン側で独立に裏取り**（grep + git log + コード精読）。

## 検出結果（陳腐化 6 件・すべて「実装が真実、SPEC が追従漏れ」）

> F1–F3 は 3 分割監査 + メイン裏取りで検出。**F4–F6 は plan-review Step 2b の独立再導出（plan/research を見せない Plan エージェント）が追加検出**し、メインで再裏取り済み。3 分割監査（section 範囲で分担・「将来 framing と明白な矛盾」に焦点）は F4/F5/F6 のような"地味な factual ドリフト"を境界で取りこぼしており、枠組みの異なる独立再導出だけが拾った（plan-review skill の「枠組みの独立が盲点に効く」の実証）。

### F1. §1 スコープ境界 / line 28 — インスタントコマンドが「将来の拡張ポイント」（issue の明示例）

- SPEC.md:28 `- **将来の拡張ポイント**: インスタントコマンド（@プレフィックス）によるユーザー定義コマンド`
- 実装の事実: **完全実装済み**。
  - `snotra-core/src/instant.rs`（`expand_instant_command` / `expand_exec_args` / 修飾子パイプ / `filter_instant_commands` + 多数のユニットテスト）
  - IPC `get_instant_commands` / `execute_instant_command`（`src-tauri/src/commands/instant.rs`、`main.rs:458-459` 登録済み）
  - 設定型 `InstantCommand` / `InstantAction`（`config.rs:48-67`）、既定 prefix `@`、既定コマンド 2 件（`g` / `gh`）
  - 状態遷移図にも `InstantCommandMode` として組込済み（SPEC.md:433,438-439）。SPEC §19 全体が as-built。
- 付随: line 26 `やること` の列挙にもインスタントコマンド（およびカスタムオープナー §18）が含まれず、現在のコア機能を反映していない。
- 分類: **将来framing-but-implemented**。バグではなく doc 追従漏れ。

### F2. §7.4 ホットキーバリデーション / line 319-334 — ブロック対象表が実装より少ない

- SPEC の記述: ブロック対象を `Alt+F4` / `Ctrl+Shift+Escape` / `Alt+Tab` / `Ctrl+Alt+Delete` の **4 件のみ** 列挙。line 333「modifier セットが異なる場合はブロックしない（完全一致）」と明記。
- 実装の事実（`snotra-core/src/config.rs`）:
  - `SYSTEM_SHORTCUTS`（config.rs:1211-1217）は 4 件に加え **`("alt","space")`** を持つ。コメント `// Alt+Space: Windows system menu (SC_KEYMENU)` ＝意図的。
  - `is_system_shortcut`（config.rs:1256-1259）に `if parts.contains(&"win") { return true; }`。コメント `// Win 8+ reserves all Win+* combinations at the shell level.` ＝**全 `Win+*` 組み合わせを無条件拒否**（意図的）。これは line 333「完全一致のみブロック」と矛盾する（Win はワイルドカード例外）。
  - フロント `ui/src/lib/hotkeyValidation.ts:24`（`"alt|space"`）・`:81-87`（Win 修飾子チェック）も同一挙動 → Rust/TS 実装は一致、**SPEC のみ追従漏れ**。
- 分類: **factual-contradiction**。実装が正（コメントで意図明示）、SPEC が記載漏れ → SPEC を as-built に揃える。

### F3. §17.1 履歴フォーマット互換 / line 608-609 — V1 フォールバックが実在しない

- SPEC.md:608 `読み込みは V3 -> V2 -> V1 の順でフォールバックする`、SPEC.md:609 `V1/V2（秒単位）を読み込んだ場合は…ms へ変換する`
- 実装の事実:
  - `snotra-core/src/history.rs:10-14`: `HISTORY_VERSION = 3`（ms）、`HISTORY_VERSION_POSTCARD_V2 = 2`（sec）、`HISTORY_FALLBACKS = &[HISTORY_VERSION_POSTCARD_V2]` ＝**フォールバックは V2 のみ**。
  - V1（bincode）を表す定数も旧 deserialize 経路も存在しない（`V1` / `bincode` grep ヒット 0）。実際のフォールバックは **V3 → V2** のみ。
  - 秒→ms 変換 `migrate_time_unit_if_legacy`（history.rs:225-232）は `version < 3` で発火するが、V1 はロード経路に無いため実際に走るのは V2 のみ。
- 意図性の裏取り（git log -S "V1" -- history.rs）:
  - `aa08b72` bincode→postcard 移行 + 旧形式フォールバック追加
  - **`fc4dd4e`（#198）「chore: bincode 依存を削除し旧フォーマットフォールバックを廃止」** ＝ V1（bincode）フォールバックは**意図的に廃止済み**。
- 分類: **factual-contradiction**。#198 の意図的削除に SPEC が追従していない → SPEC を as-built（V3→V2、V2 のみ秒→ms）に揃える。**事故的削除（データ損失バグ）ではない**ことを git で確認済み。

### F4. §4.7 結果表示制御 / line 181（SPEC 内では §4.7、独立導出は :182 と参照）— `shouldShowResults` の式が陳腐化

- SPEC の記述: `結果の表示/非表示は shouldShowResults メモシグナル（results().length > 0 && !indexing()）で制御する`
- 実装の事実（`ui/src/stores/search.ts:82-95`）: `shouldShowResults` は `viewKind()` の switch。`results().length === 0` で false／**tool・folder ビューは indexing 中でも常に true**／results ビューは `interpKind() === "instant" || !indexing()`。単純式 `results().length > 0 && !indexing()` は folder/tool/instant モードを反映していない。
- 補強: `ui/CLAUDE.md`「単一ウィンドウの高さ管理」が既に正しい式を記載済み（SPEC のみ追従漏れ）。
- 分類: **factual-contradiction**。SPEC が実装詳細（メモ式）を過剰記述したまま実装が 2 軸モデル（`viewKind`/`interpKind`）へ進化した。

### F5. §19.2 共通フィールド `display` / line 739（独立導出は :738 と参照）— config フィールド扱い + 優先順が逆

- SPEC の記述: `display`: 結果リスト副テキスト…**省略時は説明を使い、説明も無い場合はコマンドテンプレートを自動生成`（name/description と並ぶ config 共通フィールドとして提示）
- 実装の事実:
  - `config.rs:49-55` の `InstantCommand` は `name` / `description` / `#[serde(flatten)] action` のみ。**`display` という config フィールドは存在しない**（ユーザーが省略する/しないの対象ではない）。
  - `display` は派生 DTO 値で**常にコマンドテンプレート**（`src-tauri/src/commands/launch.rs`: Url→url / Exec→`exe args` / Legacy→command）。
  - UI 副テキスト優先は `cmd.description || cmd.display`（`ui/src/stores/search.ts:301`）＝**description 優先・display フォールバック**。SPEC line 739 の「display 省略時に説明（description）」とは優先順が逆。さらに §19.5 line 866-867（description 優先＝正しい）と**内部矛盾**。
- 分類: **factual-contradiction**。display を「設定可能フィールド」かつ「説明より優先」と誤記。

### F6. §18.6 / §19.8 設定タブ列挙のドリフト — line 663（§18.6）・line 921（§19.8）

- SPEC の記述: §18.6:663「（全般/検索/インデックス/ビジュアル/オープナー）」＝**5 タブ**（インスタント・バックアップ欠落）。§19.8:921「全般/検索/インデックス/ビジュアル/オープナー/インスタントコマンド」＝**6 タブ**（バックアップ欠落）。
- 実装の事実: タブは**7 つ**。`snotra-settings/src/tabs/` に backup/general/index/instant/opener/search/visual。表示順は `app.rs:53-70`（`TabId`）で **General / Search / Index / Visual / Opener / InstantCommand / Backup**。バックアップタブは §13.3 で別途実装記載あり。
- 関連（弱い・completeness 寄り）: §7.2「タブ構成と設定項目」（line 264-305）も `[全般][検索][インデックス][ビジュアル][オープナー]` の 5 タブのみを詳述し、`[インスタントコマンド]`（→§19.8 に詳細）・`[バックアップ]`（→§13.3 に詳細）のサブセクションが無い。タブ自体は実在＝列挙の取りこぼし。
- 分類: **factual-contradiction**（§18.6:663 / §19.8:921）。§7.2 は completeness ギャップ（後述スコープ判断で扱い決定）。

## 裏取り済み・矛盾なし（報告対象外 / SPEC は正しい）

- §3.4 アイコンキャッシュ: 上限 ×5（200×5=1000）・FIFO 退避・`get` 非変更 — 実装一致
- §4 スコア式（`5*global + 20*query`、kana `max(4500-byte_pos,1)`、path 基準 3000）、各種既定値（`visible_rows`=8 / `migemo_min_chars`=2 / `fuzzy_history_cap_ratio`=0.30）— 一致
- §10 トレイ（`Shell_NotifyIconW` / 右クリック設定・終了 / 左クリック履歴）、§13.2 バイナリ保存（magic+version, tmp→rename）、§14.2 `launch_item` / `LaunchResult`（4000ms, timeout code=-1）、§15.2 スラッシュコマンド `/o /s /q /r` — いずれも一致
- §18 カスタムオープナー（`[[openers]]` / 最具体ルール先勝ち / プリセット）、§19 インスタントコマンド（修飾子パイプ / date・uuid / `{{…}}` エスケープ）の**機能本体**、**§20 自動更新（`tauri-plugin-updater`・`auto_update`・`UpdateToast.tsx`・endpoint）はすべて実装済みで SPEC と整合** — §20 が「未実装なのに実装記述」という疑いは否定された
  - ただし §18/§19 内の周辺記述に F5（§19.2 display）・F6（§18.6/§19.8 タブ列挙）のドリフトがあった（3 分割監査は機能本体に焦点を当て見落とし、独立再導出が検出）。機能の実装有無は正しいが、フィールド定義・タブ列挙という付随事実が古い、というクラスの陳腐化

## 技術的制約

- **doc-only 変更**: 修正対象は `SPEC.md` のみ。コード・挙動・IPC 契約は一切変えない。コードは既に正しく、SPEC を実態に寄せるだけ（逆方向の SPEC→コード同期は不要）。
- **セクション番号への影響なし**: いずれの修正も §1 / §7.4 / §17.1 の**既存セクション内の本文修正**で、セクションの追加・削除がない → 子セクション番号・後続 `## N.` のずれは発生しない（AGENTS.md「SPEC.md のセクション番号整合」観点はクリア）。
- **後方互換性への影響なし**: SPEC は意図ドキュメントであり、settings.json / config.toml のフォーマットには触れない。

## 未解決の疑問 / スコープ判断

- **スコープ判断（決定済み）**: issue タイトルは「SPEC.md の最新化」、本文は「**セクションによって**内容が古い」と複数セクションを明示。よって明示例の F1 だけでなく、同型の陳腐化 F2–F6 も含めて是正するのが忠実な解釈。6 件いずれも小さく外科的な doc 修正で、すべて exact なコード根拠で裏取り済み。→ **F1–F6 を対象とする**。3 分割監査 + 独立再導出の 2 枠組みが収束した（F4–F6 は後者が追加検出）ため、completeness の確度は高い。
- **§7.2 のタブ列挙ギャップ（F6 関連・completeness 寄り）**: §7.2 は虚偽を述べてはおらず（factual contradiction ではない）、単に `[インスタントコマンド]`/`[バックアップ]` のサブセクションを欠く。詳細は §19.8/§13.3 にあるため、§7.2 にはフル複製ではなく**ポインタ的な小エントリ**（→§19.8 / →§13.3）を 2 つ足してタブ構成を 7 タブ整合させる。最小・最優先度低・ユーザー判断で省略可として plan に記載。
- **FORBIDDEN_MAIN_KEYS の Rust/TS 乖離（スコープ外・follow-up）**: `hotkeyValidation.ts` は `Config::validate()` に無い禁止メインキー群（capslock・IME キー等）を持つ。SPEC line 334「同じリストでガード」は厳密には不正確だが、これは**実装の不整合（TS が Rust より厳格）**であり「実装済み→将来 framing」の doc 陳腐化ではない。Rust 側も同じガードを持つべきか否かは設計判断 → **本 issue のスコープ外・follow-up issue 候補**。F2 では Win/Alt+Space のみ是正し、line 334 は触らない。
- **docs/ は本 issue のスコープ外**: 監査は SPEC.md のみ。`docs/architecture.md` 等に並行する陳腐化があるかは未監査（issue は SPEC.md に限定）。発見があれば follow-up issue とする。
- **F1 の "やること" にカスタムオープナー（§18）も加えるか**: §18 はどこでも「将来」とは書かれていない（単に §1 サマリに未記載なだけ）。インスタントコマンドと同じく「コマンド実行のカスタマイズ」系の実装済みコア機能のため、サマリの正確性のため一緒に加える。検証エージェントが文言の妥当性を確認済み。
