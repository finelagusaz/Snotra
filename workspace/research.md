# research.md — issue #561: 既存コメントを comment-guidelines へ一括スイープで揃える

2026-07-18 調査。実施時再数え上げ済み（下記各項目）。

## issue の要約

#557 / PR #560 で策定した `docs/comment-guidelines.md` に既存コメントを一括で揃える、意図的な機会的移行の例外。**挙動不変・コメントのみの変更**。4 項目（非定型ラベル / 日英混在 / lib・stores の TSDoc 補完 / 逐語訳・レビュー残骸の削除）は互いに独立。

## 関連コード（再数え上げの結果・全箇所を実読で裏取り済み）

### 項目 1 — 非定型ラベル（grep 実測: issue の事前数え上げと一致、2 箇所のみ）

`NOTE:|HACK:|XXX:|FIXME:` の grep 結果（`*.{rs,ts,tsx,mts,cts}` 全域）:

- `snotra-core/src/binfmt.rs:169` — `#[cfg(test)] mod tests` 内。「header roundtrip 等のカバレッジは下の Result 系テストにある」というテスト設計の所在メモ → **`実装メモ:` へ**（ガイドライン: 実装メモ = 設計判断・テスト設計の補足）
- `snotra-core/src/config.rs:959` — `validate()` 内。「icon_cache_cap は派生値ゆえここでの検証は不要」という将来の変更者向け条件 → **`保守注意:` へ**（実例に挙がっている config.rs の icon_cache_cap 派生値コメントはまさにこの箇所の同族）

受け入れ条件「非テストコードで 0 件」に対し、binfmt.rs はテストコード内だが同時に定型ラベルへ寄せる（同義ラベル分裂の解消が目的のため、テスト内も対象に含めるのが issue の趣旨に整合）。

### 項目 2 — 同一ブロック内の日英混在（全ツリー走査の結果: 1 件のみ）

Explore エージェントによる全対象走査（snotra-core 14 / src-tauri 21 / snotra-settings 15 / ui・e2e）＋メインエージェントによる該当箇所の実読で確認。文単位の日英切り替えは既知の 1 件のみで、他は全ブロックが単一言語:

- `src-tauri/src/main.rs:141-155` — `suspend_webview` の `///` doc。英語本文（TrySuspend の前提・0x8007139F・Best-effort）＋日本語文（149 行の括弧内実測補足、153-155 行の競合ガード説明）が混在

**統一先: 日本語**。理由: (a) 同一ファイルの姉妹コメント（`suspend_and_trim_after_hide` doc・同関数本体のインライン 167-172 行）はすべて日本語、(b) 同内容の正準的説明が `src-tauri/CLAUDE.md`「WebView2 TrySuspend / Resume パターン」節に日本語で存在し語彙を揃えられる。Win32 低レベル部ゆえ英語も許容されるが、ファイル内の支配的言語に寄せる。API 名・HRESULT 値（`TrySuspend` / `IsVisible` / `0x8007139F ERROR_INVALID_STATE`）は原語のまま。内容は削除しない（CLAUDE.md と重複気味だが doc コメント側が精密事実の正準——ガイドライン配置基準）。

### 項目 3 — lib/stores の TSDoc 補完（13 ファイル全読の分類）

模範様式: `ui/src/lib/exclusive.ts` / `ownedTimer.ts`（冒頭 `/** */`、**太字**で契約の要点、崩すと何が壊れるかまで書く）。

分類結果（詳細な分類表は plan.md → PR 本文へ転記）:

| 分類 | ファイル | 根拠 |
|---|---|---|
| **契約あり・TSDoc 追加** | `lib/perf.ts` | 4 関数の呼び出し順序（`perfMarkInput` → `perfStartSearch` → `perfMarkSearchDone` → `perfMarkRenderDone`）、欠けると単一スロットが黙ってドロップ / `perfCancelSearch` による解放義務（怠ると `MAX_PENDING=256` で全 clear・精度劣化）/ requestId は `searchLane.current()` 由来の一意性前提 / `source==="query"` のみ集計 / `ENABLED` ゲート——いずれもコードにも CLAUDE.md にも未記載 |
| **限定的候補（一点のみ）** | `lib/trace.ts` | 「呼び出し側も `import.meta.env.DEV` でガードする義務」は契約だが ui/CLAUDE.md 実装パターンに既載。書くならこの一点限定（重複回避） |
| 契約はあるが既存 TSDoc＋CLAUDE.md で担保済み・追加不要 | `lib/commands.ts`（hideMainWindow 順序契約は関数 TSDoc 済）、`lib/interpretQuery.ts`（SSOT・純関数、ヘッダ＋関数 TSDoc 済）、`lib/truncatePath.ts`（clearTruncateCaches TSDoc 済）、`lib/types.ts`（SavedViewState choke point TSDoc 済）、`stores/search.ts`（ほぼ全関数に詳細 TSDoc。冒頭ブロック新設は CLAUDE.md 責務一覧と正面衝突）、`stores/instantCommand.ts`（契約 TSDoc 充実）、`stores/launchNotice.ts`（タイマー一元管理 TSDoc 済）、`stores/folder.ts` / `stores/tool-selection.ts`（状態の器。規律は search.ts 側の choke point が担う） | ドキュメント間 DRY（コメント=契約 / CLAUDE.md=責務一覧） |
| 契約なし・追加不要 | `lib/invoke.ts`（薄い Promise ラッパー。staleness 調停は呼び出し側の責務）、`lib/theme.ts`（冪等な CSS 変数書き込みのみ） | 機械的付与はしない（ガイドライン） |

### 項目 4 — 逐語訳型・レビュー向け説明の残骸（全ツリー目視パス結果）

**逐語訳型は 0 件**（PR 本文にその旨記載する）。「変更ナラティブ（`以前は〜だった`）とレビュー残骸」が 4 件。いずれも「現在の形の理由（keep）」と「変更履歴の再現（ガイドライン: 書かないもの）」が同居しており、核を残して履歴部分を現在形の理由へ書き換える:

- `src-tauri/src/commands/search.rs:43-48` — keep: IPC 返り値契約の系統・wire 互換性（Ok(v) と v の同一シリアライズ）・#434 参照。削減: 「以前は Result<_, String> だったが…」の履歴文 → 「Result にしない理由」の現在形へ
- `src-tauri/src/commands/system.rs:19-21` — keep: ERR_INDEXING_IN_PROGRESS 共有・#434。削減: 「以前は bool で…不一致だった」
- `src-tauri/src/trace.rs:1-13` — keep: 委譲構造・seq が単一単調列で interleave する現在挙動・#433。削減: `used to each carry their own copy` の履歴文・**`called out in the PR description` は典型的レビュー残骸で削除**
- `ui/src/lib/commands.ts:64-67` — keep: 「Err(ERR_INDEXING_IN_PROGRESS) を握りつぶしユーザー可視挙動を変えない」契約・#434。削減: 「以前は false で表現し…」

**keep と判定（削らない）**: `src-tauri/src/main.rs:167-172`（suspend_webview クロージャ内の「旧実装ではこの安全弁を…」——旧実装差分に基づく現在の設計理由の記録。folder.rs と同種）、`snotra-core/src/folder.rs:57-58`（symlink 挙動不変の不変条件記録）、`snotra-core/src/query.rs:57-58,79`（DRY/SSOT の存在理由・簡潔）、`config_watcher.rs:196-210` / `MainApp.tsx:165,195`（イベント名 ≠ config キーの生きた不変条件）、`icon.rs` #522 記録群・`search.rs` の `# Why parallel Vecs`・`instant.rs` #394 記録（歴史メモの規範例）。

## 既存パターン

- 定型ラベルの実例: `保守注意:`（config.rs）・`実装メモ:`（snotra-settings/app.rs）——寄せ先はガイドライン表の既存慣習
- TSDoc 様式の模範: `exclusive.ts` / `ownedTimer.ts` の冒頭ブロック
- trace.rs の書き換えは `src-tauri/CLAUDE.md` の trace.rs 節（日本語・現在形）と語彙を揃えられる

## 技術的制約

- **挙動・シグネチャに一切触れない**（コメント・doc コメントのみ）。Win32 API の同期性調査は不要（コード変更なし）
- PostToolUse hook: `*.rs` 編集で clippy＋crate テスト、`ui/src/**` の `*.ts` 編集で typecheck が自動発火——**沈黙 = 合格**
- rustdoc は `cargo doc` の対象。doc コメント内のコードフェンスや intra-doc link 形式を壊さないこと（`suspend_webview` doc の書き換えで backtick を維持）

## 追記（plan-review Step 2b 独立再導出による拡張・2026-07-18）

本ファイルの初期走査（項目 2: 混在 1 件・項目 4: 4 件）は plan-review の独立再導出で大幅に拡張された——コメント行ブロック化スクリプト＋語彙「旧/かつて」grep の走査設計が初期走査の網より細かく、混在 13 ブロック・履歴ナラティブ約 15 箇所・TSDoc 候補 2 件（iconBatch.ts / lruIconCache.ts）を追加検出した。全箇所をメインエージェントが実読で裏取り済み。**確定版の変更集合と差分裁定は `workspace/plan.md` を正とする**。

## 未解決の疑問

- なし（要求は issue の受け入れ条件で一意に確定。統一先言語の判断理由は plan.md＋PR 本文に記載する）
