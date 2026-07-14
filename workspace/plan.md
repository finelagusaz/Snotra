# plan: issue #539 — preGen+1 baseline 判定を withLaunchLifecycle 側の述語へ引き上げる

## 種別判定（AGENTS.md Step 0）

**refactor（挙動保存）**。SPEC.md 記載のフロー・IPC 契約・状態遷移を変えない
（`disturbed()` は既存 `current() === preGen+1` と厳密に等価 ── research.md 参照）。
→ SPEC.md 更新は**不要**。ただし内部実装の正規手段を記す 3 docs は同時更新（issue 明記）。

## 設計判断（確定）

`withLaunchLifecycle` が `searchLane.invalidate()` を所有する ＝「自分の launch を超えて world が
動いたか」を答える述語 `disturbed()` も所有すべき。invalidate 直後に `launchGen` を捕捉し、
`onSuccess`/`onFailure` の**両方**へ `disturbed: () => boolean` を渡す。

- **両方に渡す根拠**: issue proposal が `onFailure`/`onSuccess` を名指し（CLAUDE.md「計画書の要素を
  省略・統合・削除するのは明示指示のみ」）。かつ onSuccess/onFailure は対称ペアゆえ署名も対称に保つ。
- **消費者**: 現状は `executeInstantCommandSelected` の onFailure のみ。onSuccess の `disturbed` は
  未消費（署名対称性 + issue 忠実性のため配る）。
- **plan-review 結論**: onSuccess 伝播は YAGNI 寄りだが両監査とも「issue 忠実性・対称署名として許容・実害なし」。
  ただし ui/CLAUDE.md「実装しない機能のコメントは書かない」との整合のため、**onSuccess の `disturbed` 引数には
  「現状 consumer ゼロ・対称署名のための供給」を 1 行コメントで明記する**（未使用の理由を証跡化）。
- **codex 敵対レビュー（実装後・commit 664bb62）**: claim 4 として同じ過剰抽象を再指摘（onFailure だけに絞るべき）。
  correctness は 6 主張すべて反証不成立（等価性・復元順序・導入バグ・docs 整合すべて堅牢）。
  **ユーザー判断で「忠実のまま両方に残す」を再確認**（issue proposal の明示 + 対称署名を優先）。設計は据え置き。

## 変更ファイル一覧

### 1. `ui/src/stores/search.ts`（実装本体）

#### (a) `withLaunchLifecycle`（496-517）── 署名変更 + `disturbed` 合成

```ts
async function withLaunchLifecycle(
  launch: () => Promise<api.LaunchResult>,
  onSuccess: (result: api.LaunchResult, disturbed: () => boolean) => void,
  onFailure: (result: api.LaunchResult, disturbed: () => boolean) => void,
): Promise<boolean> {
  clearLaunchNotice();
  setLaunching(true);
  try {
    searchLane.invalidate();
    // この launch が確立した world 世代。await 中に他の invalidate/run が走れば current() が
    // これを超える＝disturbed。呼び出し側は「1 bump」という内部実装を知らずに staleness を問える。
    const launchGen = searchLane.current();
    const disturbed = () => searchLane.current() !== launchGen;
    clearResults();
    const launchResult = await launch();
    if (launchResult.status !== "ok") {
      notifyLaunchFailure(launchResult);
      onFailure(launchResult, disturbed);
      return false;
    }
    onSuccess(launchResult, disturbed);
    return true;
  } finally {
    setLaunching(false);
  }
}
```

- 関数ドキュメント（490-495）に「world 世代の comparison choke point でもある」旨を 1 行追記。

#### (b) `executeInstantCommandSelected`（632-681）── preGen 撤去 + `!disturbed()` 判定

- **削除**: line 650-651 のコメント + `const preGen = searchLane.current();`。
- onFailure を `(launchResult, disturbed) => { ...; if (!disturbed()) { restore } }` へ。
- 復元本体（`searchLane.invalidate()` + `setInstantCommandItems` + `updateResults` + `setSelected`）は**不変**。
  `disturbed()` は `if` 条件で復元前に評価されるため、復元内の `invalidate()` は影響しない（従来同様）。
- コメント「onFailure 判定時点で world 世代が preGen+1（withLaunchLifecycle の 1 bump のみ）」→
  「await 中に world が動いていなければ（＝この launch のみ）候補を復元」へ更新（magic number 記述を除去）。

#### (c) `launchAndReset`（607-630）・`launchWithSelectedTool`（519-554）── 新署名へ追随

- onSuccess/onFailure は現状 `() => {...}` / `(launchResult) => {...}`。TS はより少ない引数の callback を
  新署名へ代入可能（関数引数の代入可能性）ゆえ**本文変更不要**。`disturbed` を消費しない。
- **訂正（plan-review 指摘）**: TS は「引数が少ない callback を多い型へ代入」を**常に許可する**ため、
  署名変更は呼出し元で**型エラーを出さない＝コンパイラは全呼出し元を炙り出さない**。3 呼出し元の網羅は
  **grep が根拠**（`withLaunchLifecycle` は未 export の module-private・`grep -rn "withLaunchLifecycle" ui/src`
  で定義 1 + 呼出し 3＝search.ts:533/611/653 のみ・外部呼出し元なし）。コンパイラを検証根拠に据えない。
- typecheck は「新署名でも既存 callback が代入可能（本文変更不要）」を確認するために走らせる（署名整合の
  network は grep、typecheck は「壊していないこと」の確認）。

### 2. `ui/src/stores/search.test.ts`（テストコメント追随・assertion 不変）

- `1030-1062`「world 世代が進んだら復元しない」テスト: assertion は挙動同一ゆえ**不変**。
  コメント（1030 見出し `preGen+1 判定`、1033 `preGen+1 不成立`、1046 `preGen 捕捉 → +1`、
  1049 `preGen+1 を超える`、1054 `current() !== preGen+1`）を `disturbed()` 述語ベースの表現へ更新。
- `614-629`「失敗: 候補が復元される」= 非 disturbed → 復元の正パス: **変更不要**（挙動同一）。

### 3. docs 同時更新（3 ファイル・4 箇所。issue 明記・正規手段の記述）

- `ui/CLAUDE.md:109`: 「`current() === preGen + 1`」の例示を
  「`withLaunchLifecycle` が invalidate 直後に捕捉した `disturbed()` 述語（例: `executeInstantCommandSelected`
  の失敗ロールバックが `if (!disturbed())`）」へ。lane 外復元の正規手段が「呼出し側の生算術」から
  「lifecycle 所有の述語」へ移った旨を反映。
- `.claude/rules/ui.md:10`: 「`await` 前に `searchLane.current()` をキャプチャし `current() === captured + 1`
  の基準値比較」→「起動フローでは `withLaunchLifecycle` が invalidate 直後に捕捉した世代との差分を
  `disturbed()` 述語で配り、呼出し側は `if (!disturbed())` で復元判定する」へ。
  （lane 外一般の captured+1 パターンが起動フローに限っては lifecycle へ集約された、という記述に整える）
- `.claude/skills/race-check/SKILL.md`: **2 箇所**（plan-review で漏れ検出・両監査が独立収束）。
  - `:85`（チェックリスト 4b）: 保存状態復元の staleness 検証手段として `withLaunchLifecycle` の
    `disturbed()` を正規手段に加える（生 `captured+1` 比較の例を述語へ差し替え）。
  - `:108`（Step 5 出力テンプレート `4b staleness: [OK] ... / current() === preGen + 1 で検証済み`）:
    `current() === preGen + 1` の見本を `!disturbed()` へ差し替え。**`:85` だけ直すと同一スキルが自己矛盾**
    （85 行「述語で検証せよ」 vs 108 行「preGen+1 で検証済み＝OK」）＝governance-docs.md の「序数参照が静かに腐る」典型。

### 4. `preGen` シンボルへの間接参照の是正（plan-review で漏れ検出・別カテゴリ）

`executeInstantCommandSelected` から `const preGen` が消えるため、それを**同期プレフィックスの例**として
名指しする 2 コメントが dangling 参照になる（「captured+1 を正規手段と記す doc」とは別カテゴリ・機能無関係）:

- `ui/src/lib/exclusive.ts:13`: JSDoc「同期プレフィックス——例: `executeInstantCommandSelected` の
  `preGen` 捕捉・`selected()` 読み——が」。refactor 後も同期プレフィックス（`savedResults`/`savedSelected`/
  `savedItems` 捕捉・`selected()` 読み・`interpret()`）は残るので、`preGen` 例を保存状態捕捉の例へ差し替える。
- `ui/src/lib/exclusive.test.ts:70`: テストコメント「（`preGen` 捕捉・`selected()` 読みが現行と同 tick で
  走る不変条件の担保）」。同様に `preGen` を保存状態捕捉の表現へ差し替え（テストロジック・assertion は不変）。

## 実装順序（フェーズ）

署名変更(a)と `executeInstantCommandSelected` の判定(b)は相互依存（`disturbed` を渡す署名が無いと
`if (!disturbed())` が書けない）ため **1 コミット単位**で編集する。(c) の 2 呼出し元は本文変更なし
（fewer-params 代入ゆえ型エラーも出ない）が、同一コンパイル単位で typecheck green を確認する。

1. **Phase 1（実装・1 コミット単位）**: search.ts の (a)(b)(c) を同時編集 → `npm run typecheck` green +
   `search.test.ts` green（挙動保存の実証）。PostToolUse hook が typecheck + vitest を自動発火。
2. **Phase 2**: search.test.ts のコメント更新（1030/1033/1046/1049/1054 の `preGen+1` 語彙 →
   `disturbed()` ベース。assertion 不変）+ `exclusive.test.ts:70` の `preGen` 参照差し替え → 再度 green 確認。
   - `exclusive.ts:13`（JSDoc）と `exclusive.test.ts:70`（コメント）は `.ts` 編集ゆえ typecheck が発火するが、
     どちらもコメントのみの変更で型に影響しない（沈黙 = 合格）。
3. **Phase 3**: docs 更新（`ui/CLAUDE.md:109` / `.claude/rules/ui.md:10` / race-check `SKILL.md` の :85 と :108）。
   - `.claude/rules/ui.md` / `race-check/SKILL.md` の編集は `selectChecks` 対象外（沈黙 = 未実行・合格ではない）
     ── 目視で「旧例示が新述語へ差し替わり実装と整合」を確認。`ui/CLAUDE.md` も同様。

## 不変条件

1. **挙動保存**: `disturbed()` 判定は `current() === preGen+1` と全入力で一致（research.md の `k===0` 証明）。
   検知手段 = `search.test.ts:614-629`（復元する）と `1030-1062`（復元しない）が実 `withLaunchLifecycle`
   経由で両パスを固定。両テストが green のままなら挙動保存が実証される。
2. **1 bump 不変条件**（codex claim 2 で文言を正確化）: `withLaunchLifecycle` が **`await launch()` 前の
   本体**で world 世代を進めるのは本体先頭の invalidate の **1 回のみ**（onSuccess/onFailure の callback 側は
   別途 `invalidate()`/`run()` を呼びうるが、それらは `disturbed` 捕捉後・await 後に走るため等価性に影響しない
   ── 「withLaunchLifecycle は 1 回だけ bump」という無条件の言い方は避ける）。`disturbed` の `launchGen` は
   この本体先頭 invalidate の直後に捕捉する。この不変条件が崩れる（例: clearResults が invalidate を呼ぶよう
   変わる）と `disturbed` の意味も変わるが、**その変更は同一関数内で launchGen 捕捉位置と同居する**ため、
   旧設計（別関数 executeInstantCommandSelected に散った `+1`）より破綻を局所化・可視化する＝これが issue の
   狙い（coupling をコンパイラの見える場所へ引き寄せる）。
3. **復元順序**: `disturbed()` は `if` 条件で復元本文の前に評価。復元内 `searchLane.invalidate()` は
   評価後に走るため自己汚染しない（従来と同一）。
4. **署名波及の完全性**: onSuccess/onFailure 署名変更の呼出し元網羅は **grep が根拠**（TS は
   fewer-params callback を代入許可＝型エラーで炙り出さない・plan-review 訂正）。`withLaunchLifecycle` は
   未 export の module-private。`grep -rn "withLaunchLifecycle" ui/src` = 定義 1（496）+ 呼出し 3
   （533/611/653）、外部呼出し元なし。test 側ヒット（1046/1074）はコメント文字列で実呼出しでない。

### 異常系（失敗・異常終了・予期しない順序）

- `disturbed` は plain closure で `searchLane.current()`（= `latestRun` 内 `let generation` の読取）を読む。
  例外を投げる経路は無い。SolidJS 購読も発生しない（`current()` は追跡なしの plain 関数）。
- onFailure が `disturbed()` を呼ばない呼出し元（launchAndReset / launchWithSelectedTool）では、
  `disturbed` closure は生成されるが未評価のまま GC される ── リソースリークなし（タイマー/リスナー等の
  ライフサイクル資源を握らない純粋 closure）。
- launch が reject した場合（`await launch()` が throw）: onSuccess/onFailure いずれも呼ばれず
  `finally` で `setLaunching(false)`。この経路は今回変更しない（従来同様）。`disturbed` 未評価。

## テスト方針

- **新規テストは追加しない**（挙動保存 refactor ゆえ）。既存の 2 テストが安全網:
  - `search.test.ts:614-629`（非 disturbed → 復元）
  - `search.test.ts:1030-1062`（disturbed → 非復元）
  - 両者は実 `withLaunchLifecycle` を経由し、`+1` の内部実装ではなく**観測可能な挙動**を固定するため、
    述語への差し替え後も無改変で green を維持する（＝ refactor の正しさをコンパイラ + 既存テストで実証）。
- **検証コマンド**（`docs/build-commands.md` カテゴリ準拠。PostToolUse hook が自動発火）:
  - `ui/src/**` の `.ts` 編集 → typecheck（署名整合）+ vitest（`search.test.ts`）。
  - hook 沈黙 = 合格（`ui/src` は `selectChecks` 対象）。失敗時のみ会話に届く。
- **手動確認**: docs 3 箇所は目視で「旧例示が新述語へ差し替わり、記述が実装と整合」を確認
  （`.claude/rules/ui.md` / `race-check/SKILL.md` は hook 対象外）。

## SPEC.md 更新要否

**不要**。挙動保存 refactor であり、SPEC.md 記載のフロー・状態遷移・IPC 契約に変更なし。

## セルフレビュー（Step 5）

### 5a. check スキル結果

- **`/plan-review`**（Explore×2 + Plan×1 独立導出）: 要対処の欠陥なし。核心不変条件（1 bump・捕捉点移動の
  等価性・復元順序・テスト無改変 green・SPEC 不要・latestRun.ts 不変）を独立に再一致＝**完全性の証拠**。
  漏れ 3 件を検出し計画へ反映済み（race-check SKILL.md:108 / exclusive.ts:13 / exclusive.test.ts:70）。
  訂正 1 件（「TS が波及強制」→ grep が根拠）を反映済み。
- **`/symmetric-check`**: onSuccess/onFailure は署名対称・消費非対称（復元は失敗時のみ＝正しい非対称）。
  自 grep で production の baseline-delta サイトは search.ts:651/672 のみ＝他コードパスへの適用漏れなし。
  disturbed は資源を握らない closure＝生成/破棄ペア不要。
- **`/race-check`**: await 地点は withLaunchLifecycle:506 の 1 つ。4a〜4d 全て [安全]。staleness は等価移設、
  入力ガード（launching()）・再入ガード（activationLane）は不変。捕捉点移設で新 race 窓は生じない。

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: ✓（5a symmetric-check で検証。onSuccess/onFailure 署名対称・消費非対称は正当）
2. **影響範囲の網羅性**: ✓（`grep withLaunchLifecycle` = 3 呼出し元、`.current()` grep = baseline は 1 サイト、
   `preGen` grep = exclusive.ts/test + search.test.ts。独立導出が同一集合へ収束）
3. **境界条件**: ✓（disturbed の分岐＝擾乱あり/なしの 2 ケースを既存テスト 614-629/1030-1062 が固定）
4. **リソース管理**: ✓（disturbed は純粋 closure・資源なし。launchGen は数値。異常系は plan の該当節に記述）
5. **既存パターンとの整合**: ✓（lane 向け `isStale()` の launch-lane 版を合成＝既存 primitive パターンの延長。
   新規状態フラグ・Mutex・子プロセスの導入なし）
6. **YAGNI 違反**: △→許容（onSuccess の disturbed は未消費。issue 忠実性・対称署名として両監査が許容。
   未使用理由をコメント明記で証跡化する ── 上「設計判断」参照）
7. **シンプル化の挑戦**: 新状態の導入ゼロ。むしろ magic number（+1）と別関数への coupling を除去する減算的変更。
   「この操作が失敗したら」= disturbed は例外を投げず SolidJS 購読も起こさない（plan「異常系」に記述）
8. **破壊不変条件の明示**: 「壊れたら即アウト」の不変条件 = ①挙動保存（等価性）②1 bump。
   検知手段 = 既存テスト 2 本（614-629 復元する / 1030-1062 復元しない）が実 withLaunchLifecycle 経由で
   両パスを固定。typecheck が署名整合を確認。docs 3 種は hook 対象外ゆえ**目視確認**を検知手段とする。

### 総評

計画 completeness: **高**（独立導出が核心判断を再一致 + 漏れ 3 件を検出し反映済み）。
実装着手可否: **可**（要対処なし。`/implement` へ進める）。
