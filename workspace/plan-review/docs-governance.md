# docs-governance レビュー — issue #803

## 問題なし

- **G10（恒久規範の面積 ratchet）は本変更の対象外である**（実測）: `AREA_BUDGET.alwaysLoaded` の母集団は `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]`（ルート直下のみ）+ skill description、`rules` は `.claude/rules/*.md` のみ（`governance-check.mjs:542,619,687`）。plan.md が触る `SPEC.md` / `docs/build-commands.md` / `snotra-core/CLAUDE.md` はいずれもこの母集団に入らない（モジュール CLAUDE.md は明示的に対象外とコメントされている・`governance-check.mjs:536-538`）。ratchet 超過の懸念は無い。
- **G12（config フィールド到達性）は `pub const` / private fn に反応しない**（実測）: `configFields()`（`governance-check.mjs:920-934`）は `pub struct {...}` ブロック内の `pub field:` のみを抽出する正規表現で、`pub const ENV_CONFIG_DIR` も `fn config_dir_from` も対象外。G12 への影響は無い。
- **`npm run governance:check` はベースラインで green**（実行結果）: `governance:check — G1..G12 passed（対象文書 50 件 / rules 7 件 / skills 13 件 / 恒久規範 常時ロード 13250/13338 字・rules 8123/8159 字 / 見出し参照 164 件を 63 文書から照合 / config フィールド 67 件の到達性）`
- **G6（CI/CD メモ対応表）は無関係**: `check:colors` は `docs/build-commands.md` のどの CI 対応表行にも `.github/workflows/*.yml` にも現れない（grep で確認）。`-Restore` 行の削除・bullet 差し替えは G6 の照合対象外。
- **package.json「変更なし」判断は妥当**: `package.json:14` の `check:colors` スクリプトは `pwsh -NoProfile -File scripts/visual-check-colors.ps1`（フラグを一切埋め込まない）。内部の `-Restore` パラメータ削除・env 設定への切替は呼び出し形に影響しない。
- **PERFORMANCE.md への追記は不要**: `PERFORMANCE.md:245` が「このリストが計器の正本である（`docs/build-commands.md` には置かない）」と明示するのは `SNOTRA_EGUI_*_TRACE` 系の**計測**専用ハッチのみ。`SNOTRA_CONFIG_DIR` は計測ではなく config 差し替えなので、この一覧に足す対象ではない。plan.md が触れていないのは正しい。
- **`docs/build-commands.md` の `check:colors` ブロックの記載（`-Restore` 行削除・bullet 差し替え・`SNOTRA_CONFIG_DIR` 追記）は現状（63-76行目）と過不足なく対応している**。

## 要対処

- **plan.md 自身の「SPEC.md 更新要否」節と実装チェックリストが食い違っている**: 「SPEC.md 更新要否」節（plan.md:154-159）は「§13.2 は §13.1 を参照する形で足りる」と書くが、Phase 3 のチェックリスト（plan.md:96-103）には §13.2 へ何かを足す項目が無い。このままでは §13.2（`SPEC.md:611-618`、`%APPDATA%\Snotra\index.bin` 等を無条件列挙）に参照ポインタが実際には追加されず、§13.2 だけを読む読者には全称表現が偽のまま残る。Phase 3 に「§13.2 冒頭（または該当行）に `§13.1 を参照`」のチェック項目を明示的に追加すべき。
- **`SPEC.md` §13.3「設定フォルダを開く」（`SPEC.md:637`）が変更ファイル一覧・Phase 3 のどちらにも挙がっていない**: 「`%APPDATA%\Snotra` をエクスプローラーで開く」も同じ `Config::config_dir()` 経由（research.md が確認済み・第二経路なし）で、env 上書き後は無条件に真ではなくなる。§13.1 は「保存先」の話で §13.3 は「操作」の話であり、§13.1 への 1 行追記だけでは §13.3 の記述は自動的にカバーされない。
- **`SPEC.md` §5.2「データ保存」（`SPEC.md:209-214`、履歴データの保存先）も同じ全称表現を持つが、plan.md はこの節に一切触れていない**: 「バイナリ形式で `%APPDATA%\Snotra\` に保存」は §13.1/§13.2 と全く同型の主張で、`history.rs` も `Config::config_dir()` を経由する（research.md「13 箇所」に含まれる）。§13.1 だけを直す計画では、§5.2 単独で読んだ読者に対して同じ嘘が残る。

## 軽微な懸念

- **`docs/architecture.md`「データ永続化」節（`docs/architecture.md:101-106`）にも同じ全称表現がある**（`%APPDATA%\Snotra\` にファイル分割保存: `index.bin`, `icons.bin`, `history.bin`, `window.bin`, `config.toml`）が、plan.md の変更ファイル一覧に無い。この文書はアーキテクチャ横断パターンの記述であり `SPEC.md` が正本なので、`AGENTS.md`「文書に事実の写しを増やす変更→正本を1か所に定め他は参照へ」に照らせば **参照先（SPEC.md §13.1）への 1 行言及を足すか、あるいは意図的に据え置く判断を明示すべき**。据え置くにしても plan.md にその判断根拠が書かれていない。
- **`snotra-core/tests/memory_footprint.rs:11`** の `%APPDATA%\Snotra\index.bin` 言及はテストの実運用計測点の説明（コメント）であり、env 上書き後も「実運用でのデフォルト経路」としては引き続き真——全称表現の破綻ではないため対応不要と判断できるが、plan.md に判断の記載は無い（重要度は低い）。
- **`docs/build-commands.md` の `SNOTRA_CONFIG_DIR` 追記の置き場所・形式が plan.md では未指定**（「env として記載」としか書かれていない）。既存の updater env ハッチ節（78-91行目）はテーブル形式だが、`SNOTRA_CONFIG_DIR` は「検証スクリプトの内部実装詳細」という性質が強く、ユーザー向け env ハッチ一覧に混ぜるか独立させるかは実装時の判断に委ねられている。計画の欠落というより実装の自由度の範囲内と見るが、レビュー時に確認する価値はある。

## 未検証（理由）

- **G11（見出し参照の着地）**: plan.md は Phase 3 で新設する文言の逐語（例えば `` `SPEC.md`「13.1 設定データ」 `` のような正準参照構文を使うか）を specifies していないため、実装後の文言が G11 の正準形（`` `<file>`「<見出し>」 ``）に該当するかは実装を見るまで判定できない。plan.md の記述からは意図的な見出し参照の追加は読み取れず、通常のプレーンな追記であれば G11 には引っかからない可能性が高いが、確証は無い。
- **`snotra-core/CLAUDE.md`「config.rs」節への 1 行追記の具体文言**: plan.md は「上書きの非対称（`Snotra` を付けない）と空文字の扱いを 1 行足す」とだけ記述し、逐語案が無い。既存節の記述密度・`config.rs` 節の他の bullet との整合（例えば「Config のデシリアライズ経路」節との重複有無）は実装時の文言を見ないと判断できない。
