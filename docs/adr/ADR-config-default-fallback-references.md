# ADR-config-default-fallback-references: config 既定の写しを消すとき、何を参照へ寄せ何を寄せないか

## 文脈

#795 で、`config.rs` の `default_*()` が返す値と同じリテラルが手書きでコピーされている箇所を潰した。一致を保つ機構は 1 つも無く（コメントの規範だけ）、`docs/development-principles.md`「config の値は到達性の検出器を持たない」が言う「既定値の偶然の一致」の実例だった。

## 決定

1. **参照の形は `Default` 実装である**（不足していた `AppearanceConfig` / `HotkeyConfig` に新設）
2. **`*Config::default()` 自身の手書きも先に消す**（消費者を向ける先が写しでは意味が無い）
3. **既定を返す式は例外なく `unwrap_or_else` で受ける**（当初は「確保・I/O を伴うものだけ」という使い分けにしていたが、`/simplify` の指摘を受けて全件を遅延へ揃えた。`unwrap_or_else` は決して高くつかない一方、使い分けは**式のコストを読者が毎回再導出する**ことを要求し、しかも将来 `AppearanceConfig` に `String` フィールドが増えれば `unwrap_or` の側が黙って毎フレーム確保を始める。規則は機械的な方がよい）
4. **「同じ数」でも「同じ概念」でなければ参照へ寄せない**

## 検討した代替案と却下理由

- **`default_*()` を `pub` にして直接呼ぶ**: 却下。`docs/development-principles.md`「config の値は到達性の検出器を持たない」が「**lib crate の `pub` 項目に `dead_code` は出ない**」と明記している。公開面を増やすことは**到達性の検出器を失うこと**であり、この issue が告発している欠陥と同じクラスの穴を新設する。`Default` 実装なら trait 実装であって公開関数を増やさない。
- **`LazyLock<Config>` の静的を置き、全 fallback をそこから読む**（`visual.rs` の `DEFAULT_VISUAL` を一般化する）: 却下。`Config::default()` は `default_scan_paths()` で**ファイルシステムを叩き**（`.exists()`）、`default_language()` で **OS ロケールを読む**。窓幅 1 つの fallback のためにその I/O を静的初期化で走らせるのは過剰である。`DEFAULT_VISUAL` が静的なのは「色 5 本 + font_family の `String` を**構造体まるごと**毎フレーム確保する」という別の理由による。
- **`window_width` に `#[serde(default = "…")]` を足す**: 却下。今日は `[appearance]` に `window_width` が無い TOML は **parse 失敗**し、`.bak` 退避 + 既定起動（`RecoveredFromCorrupt`）へ落ちる。足すと「欠落は正常」へ意味が変わり、**受理する config 形式の変更**（後方互換の判断・`/persistence-check` の領分）になる。既定値をリファクタする PR が持ち込んでよい変更ではない。
- **`window_coordinator.rs` の `inner_size()` 失敗時の `600.0` も参照へ寄せる**: 却下（**一度置換してから差し戻した**）。この `600.0` は `appearance.window_width` の写しではない——読み元は **OS の現在サイズ**であり、これは「問い合わせが失敗したときの便宜値」がたまたま既定幅と同じ数なだけである。参照へ寄せると**存在しなかった結合をコード上の主張として新設**することになり、(a) 本当の欠陥（`window_width = 900` のユーザーで `inner_size()` が失敗すると窓が 600 へ縮む）が「対応済み」に見えて隠れ、(b) それを再検討するために起票した issue の該当項が、コードを読む人に届かなくなる。**重複排除の対象は文字列ではなく概念である**（`AGENTS.md`「検証の作法（全タスク共通）」・#500）。
- **`launcher_controller.rs` の `Language::Ja` を `default_language()` へ寄せる**: 却下。`default_language()` は OS ロケール依存で、非 ja 環境では `En` を返す。**定数と一致しえない**ため、寄せると挙動変更になる。
- **`snotra-settings` の `PRESETS` を既定値から導出する**: 却下。**「導けない」からではなく、フィールドの型を変えない判断による**——`PresetDef` の 5 色を `&'static str` から `String` へ変え、`const PRESETS` を `static PRESETS: LazyLock<[PresetDef; N]>` にすれば、Obsidian エントリは `VisualConfig::default()` から**構造的に導出できる**（`preset_matches` の比較 5 箇所も `&p.bg` へ直す）。それを採らないのは、(a) 既定値のリファクタが UI の型定義と全 4 プリセットの表現を変えることになり、(b) 利用者の裁定が「置換＋不変条件のテスト」であり、(c) 導出した先で「Obsidian だけが既定に追従し他 3 つは追従しない」という非対称が型からは読めなくなるためである。**置換で消せない写しはテストで固定する**——UI が使う述語（`preset_matches`）をそのまま呼ぶ形にした（自前で 5 色を比較すると、UI が守る不変条件より厳しい主張になる）。型を変える案は follow-up の検討対象として残す。
- **`governance:check` に「既定値リテラルの重複」検査を新設する**: 却下。正当なリテラル fallback（`AutoUpdateMode::Disabled` の意図的 fail-safe・`Language::Ja`・レイアウト定数・DragValue の値域など）が 20 箇所以上あり誤爆が必至で、置換後は母集団がほぼ空になる。セーフティネットの変更として合意も要る。

## 後日の決定（#824 の 1 と 2）

上の却下 2 件は**「#795 の射程で触るか」への回答であって、読み元そのものへの回答ではなかった**——どちらも「読み元自体は #824 で決める」とコードのコメントに書いて残していた。その #824 で読み元が決まったので、ここに追記する。**上の却下理由は #795 の判断としては今も正しい**ため書き換えない。

- **`window_coordinator.rs` の show 経路**: **読み元ごと config へ寄せる**（`inner_size()` の読みを撤去する）。当初は「落とし先だけを config の実値へ」で足りると考えたが、それでは上の却下理由が名指ししていた欠陥（`window_width = 900` のユーザーで問い合わせが失敗すると窓が 600 へ縮む）しか閉じず、**hide を跨いだ設定変更が残る**——hidden 中は `update()` が走らないので `inner_size()` は旧幅を返し、show 直後の最初のフレームが新幅へ書き直して幅がスナップする。これは同じブロックが高さについて断っている視覚スナップと同型である。読み口は `read_window_width` に**一本化**し、`view.rs` の `window_width` もそこへ委譲する（読みと落とし先を独立実装に分けない——同型の乖離が `read_metrics` の doc に 52.0/43.0 として記録されている）。**却下理由の (b)「issue の該当項がコードを読む人に届かなくなる」は、届いた結果ここに決定が書かれたことで役目を終えた**
  - 幅について config が正本であることは既に確立していた（`view.rs` の `window_width` の doc が記録する、config_watcher との 2 次元 read-modify-write race の除去）。show 経路だけが OS 往復を残していた形で、main 窓は `resizable(false)` ゆえ OS 側が config と食い違う経路は上の 2 つしか無い。**挙動の変更を含むため `SPEC.md` を同期した**（`SPEC.md`「7.5 設定反映タイミング」の幅の書き手と、`SPEC.md`「4.7 結果表示制御（2 窓構成）」の show 時の記述）
- **`launcher_controller.rs` の `Language::Ja` 固定**: **OS ロケールへ倒す**。`ja` で始まらないロケールと取得失敗はいずれも英語で、これは `SPEC.md`「7.6 起動時の設定初期化」が既に宣言している挙動である——つまり固定 `Ja` は仕様と食い違っており、この変更は仕様変更ではなく修正である。**`default_language()` を `pub` にはしない**（上の却下 1 を維持）——`GeneralConfig::default().language` を経由する
- **両者の fallback はいずれも到達しない防御である。** `.manage` は `.setup` より前に走るので `try_state::<AppState>()` は実運用で `None` を返さない（`read_metrics` の doc が言う「setup 完了前の理論経路のみ」と同じ）。**#824 の本文が項目 2 の現象として書いていた「英語ロケール環境の極初期フレームだけ日本語文言になる」は、この配線の下では起こらない**——直したのは症状ではなく、到達したときに誤る分岐である
- **`window_width` の `#[serde(default = "…")]`（#824 の 3）は未決のまま**である。受理する config 形式の変更であり、`/persistence-check` の領分だという上の却下理由がそのまま生きている

## 後日の決定（#824 の 3）

**足した。** 上の却下 3 は「受理する config 形式の変更になる」を理由にしていたが、**`SPEC.md`「13.1 設定データ」が既に「欠損キーはデフォルト補完」と宣言していた**——却下時にこの宣言を参照していなかった。ゆえにこれは形式の変更ではなく、**宣言に従っていない実装の修正**である（項目 2 を `SPEC.md`「7.6 起動時の設定初期化」への追随として裁いたのと同じ形）。**却下理由は #795 の判断としては今も正しい**ため書き換えない。

- **後方互換の判断への回答は「片方向の拡大」である**: `#[serde(default)]` は既存キーの解釈に触れないので、**今 parse できる TOML はすべて同じ値へ parse され続ける**。`config.toml` はバージョンヘッダを持たず、旧形式の凍結バイト列テストもバンプも要らない。変わるのは *失敗していた* TOML の扱いだけである（`full_config_parse_is_unchanged` がこの向きを固定する）
- **射程は宣言に従っていない箇所の全数**とした: セクション 3（`Config` の `hotkey` / `appearance` / `paths`）とスカラーキー 8（`window_width`・`hotkey` の 2 本・`CustomTheme` の 5 色）。**配列要素の必須フィールド**（`[[paths.scan]]` の `path` / `extensions`、`[[openers]]` の `target` / `tools` 等）は射程外——要素そのものが利用者の記述であり、要素単位で不完全なら値ではなく書き損じだからである。`path` は補完先の値が存在せず `""` を発明することになり（しかも `Config::validate()` が既に `ScanPathEmpty` として拾うので、既定化はエラーの出どころを parse から validate へ動かすだけである）、`extensions` / `tools` は空 `Vec` を撒けるが、**空が既に意味を持っている**ので弾く方を採る——`extensions = []` は `include_folders = true` と組めば「フォルダだけを索引する」構成として成立し（`indexer.rs` はフォルダを索引するかどうかを `extensions` とは独立に決める）、設定 GUI からも保存できる。キー欠落を空へ補完すると、**書き忘れと意図的な空が同じ意味になる**。この線引きは SPEC の字面より狭いので `SPEC.md` 側にも書いた

### `Config.paths` で却下した 2 案（否定の知識）

`PathsConfig` には「何も指定していない」の綴りが 2 つある——**`[paths]` ごと欠落**と、**`[paths]` はあるが `scan` 無し**。採ったのは `PathsConfig` を `#[derive(Default)]`（`scan` は空）にして両方を同じ値へ落とす案で、`Config::default_scan_paths()` は **`Config::default()` 専用のシード**（first-run と `RecoveredFromCorrupt`）へ役割を純化した。

- **却下: `Config.paths` に `#[serde(default = "…既定探索パスを撒く PathsConfig…")]` を付ける。** `[paths]` 欠落だけをシードするので、上の 2 つの綴りが違う値になる。これは**同じ既定が TOML の書き方によって変わる**という #795 が塞いだ乖離クラスそのものである（検査は `empty_section_deserializes_to_default_*` 群）。「`[paths]` と書くこと自体が積極的な意思表示だ」という擁護は可能だが、型にもコードにも現れず読者が再導出できない
- **却下: `scan` に `#[serde(default = "Config::default_scan_paths")]` を足して両経路を「探索パスあり」で揃える。** 2 経路は一致するが、**今日 `[paths]` を書いて `scan` を書いていない config の値が変わる**（空 → スタートメニュー + デスクトップ）。片方向の拡大という上の性質が偽になり、後方互換の判断が「既存挙動の変更」へ格上げされる
- **受容した残余**: `[paths]` を手で消した利用者は、今日の「`.bak` 退避 + バルーン通知 + シード済みで索引は動く」から「正常 parse + `scan` 空 = 索引が空のまま無言で起動」へ変わる。母集団は手編集だけで（アプリが書き出す config.toml は `scan` を必ず含む）、`[paths]` はあるが `scan` を書いていない利用者は**今日すでに同じ状態**である。`config_parses_with_all_sections_omitted` がこの挙動を固定するので、次に触る人が「バグだ」と誤認して却下案へ倒すことは防げる

### 値レベルの個別フォールバックを射程外とした決定

「A と B が不正なら A と B だけ既定へ、C はそのまま」という**値レベル**の個別フォールバックは**足さない**（利用者裁定・2026-08-06）。本 ADR が扱うのは**キー・セクションの欠落**だけで、**型/variant の不一致**（`window_width = "600"` / `preset = "Solarised"`）と**型は合うが意味が不正な値**（`window_width = 50`——`Config::validate()` は本体のロード経路から呼ばれていない）は、全体 parse 失敗 →`.bak` か素通りのまま残る。

**足さない理由: 手で書き損じた設定は設定アプリから設定し直せる。** 足すなら各フィールドを `toml::Value` 経由で個別に変換する `deserialize_with` ヘルパー・`.bak` の rename → copy 化・修復したキー名の通知が同時に要る。**その落とし先はここで足した `#[serde(default = "…")]` そのものなので、将来足す場合も本決定のやり直しは生じない。**
