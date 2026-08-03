# SPEC.md 層 4 シンボル降格 — 独立導出（issue #884 実装順序 5）

- 対象: `SPEC.md` @ HEAD 8d13646（1155 行）
- 導出者: 独立エージェント（`workspace/plan.md` / `workspace/research.md` は未読）
- 実在照合: snotra-core / src-tauri / snotra-settings の src に対して grep 済み

## 0. 集計

バッククォートスパン総数（コードフェンス外）: **712**。うち層 4（コードのシンボル名）該当は以下のとおり。

| 区分 | 件数 |
|---|---|
| 降格対象（観測文化 / 削除 / パス参照化） | **69 出現** |
| keep — 正本参照アンカー | 3 出現 |
| keep — 外部 API（挙動が契約の主語） | 22 出現 |
| 今回動かさない — 「旧 X は撤去/消滅」の負の契約形 | 4 出現 |
| 境界例として keep（SPEC 語彙へ編入と判定） | 10 出現 |

残る約 600 スパンは層 1（状態・モード・窓名・ガード式）、層 2（config キー・イベント名・環境変数・スラッシュコマンド・テンプレート構文・キー名・ファイル形式・クレート名・UI 文言）、層 3（ファイルパス）、層 5（導出ペア付きの式・値）であり対象外。config キー疑いの snake_case は全数 `snotra-core/src/config.rs` の serde フィールドと照合した（`appearance.window_width` は `AppearanceConfig` に実在し正しい層 2。`visual` の色・padding・`window_gap`、`general` の各 bool、`search` の各キーも全て serde フィールド実在を確認）。

## 1. 降格対象（69 出現）

処置の凡例: **観測** = シンボルを観測可能な挙動の散文へ置換 / **削除** = 括弧書きシンボルを削除（散文が既に契約を述べている） / **パス** = パス参照だけ残しシンボルを落とす。

| 行 | スパン | 処置 | 根拠 |
|---|---|---|---|
| 46 | `Config::default_scan_paths()` | 削除（またはパス: `snotra-core/src/config.rs`） | 既定値は直後の箇条書きが列挙しており、関数名は仕様理解に寄与しない。rename で黙って腐る |
| 92 | `commands::load_icon_pngs` | 観測「worker スレッドの一括アイコン取得」 | 内部コマンド関数名。同行にパスアンカー（`egui_shell/icon_textures.rs`）が既にある |
| 92 | `load_texture` | 観測「テクスチャ登録」 | egui API 呼び出し名。パイプラインの契約は「PNG → decode → テクスチャ」で語れる |
| 97 | `get` | 観測「取得はアクセス順を更新しない」 | メソッド名。契約は「読み出しが LRU 順位に影響しない」という観測 |
| 110 | `target_path` | 観測「エントリのターゲットのフルパス」 | 内部 struct フィールド（engine.rs / indexer.rs 実在）。config キーではない（serde 照合済み） |
| 117 | `trim` | 削除 | 散文「前後空白除去」が契約。std メソッド名は不要。※L893 以降の修飾子 `trim` は層 2（テンプレート構文）で別語義・不動 |
| 123 | `to_hiragana()` | 観測「ひらがな変換」 | 内部関数名。§4.1 に既に「ひらがな変換して」の散文がある |
| 140 | `starts_with` | 観測「prefix 比較」 | std メソッド名。契約は「実際の kana 文字列の prefix 比較が必要」 |
| 151 | `FOLDER_EXPANSION_WEIGHT = 5` | 削除 | 直前の `expansion_count * 5` が層 5 の導出ペアとして完結。定数名は二重化かつ rename で腐る |
| 184 | `bar_height` | 観測「バー高」 | layout.rs の struct フィールド名（実在照合済み）。導出（`font_size + bar_padding`）は §4.7/§11 が既に持つ |
| 184 | `toast_height` | 観測「トースト行高（= バー高）」 | 同上 |
| 184 | `set_size()` | 観測「サイズを設定する」 | tauri API 呼び出し名。契約は「main が算出し results に適用する」 |
| 194 | `selected` | 観測「選択状態」 | 内部状態フィールド名 |
| 197 | `focusable(false)` | 観測「フォーカス不可で生成」 | builder API 呼び出し。契約「フォーカスを取らない」は同文に既にある |
| 282 | `std::process::Command` | 観測「子プロセスとして起動」 | std API 名。契約の主語は「別プロセス」であって API ではない |
| 345 | `Config::default()` | 観測「既定値相当の値」 | 関数名。契約は「既定値をドラフトに適用」 |
| 350 | `Config::validate()` | 観測「保存時のバリデーション」 | 関数名。「保存時に検証しエラー時は保存しない」が契約の全て |
| 375 | `Config::validate()` | 観測「保存時検証がバックストップ」 | 同上 |
| 380 | `PlatformCommand::SetHotkey` | 観測「ホットキーを再登録する」 | enum variant。降格の典型（rename で黙って腐り、読者の理解に寄与しない） |
| 381 | `HotkeyConfig::default()` | 観測「既定ホットキー（`Alt+Q`）」 | 関数名。値 `Alt+Q` は層 2 で残す |
| 382 | `PlatformCommand::SetTrayVisible` | 観測「トレイ表示を切り替える」 | enum variant |
| 384 | `config` | 観測「実行中の設定」 | 裸の変数名。バッククォートの意味が無い |
| 386 | `PlatformCommand::SetLanguage` | 観測「トレイメニューの言語を切り替える」 | enum variant |
| 388 | `AppState` | 観測「実行中の設定から直接読む」 | 内部型名 |
| 394 | `LoadOutcome::ReadFailed` | 削除 | enum variant。分類（権限/ロック/共有違反）は同括弧内の散文が既に述べている |
| 395 | `index_generation` | 観測「構築完了世代の差分」 | 内部フィールド名。「世代番号の差分で再検索」が契約 |
| 395 | `run_ui` | 観測「view 実行 → paint の順」 | 内部関数名 |
| 395 | `update()` | 観測「hidden 中はフレームが走らない」 | egui メソッド名。同じ観測は §8.7 に散文で既出 |
| 399 | `Config::load` | 観測「起動時に読み込んだ設定」 | 関数名 |
| 399 | `Engine` | 観測「本体（コアエンジン）が保持し」 | 内部型名。窓名 `main`/`results` のような公式呼称登録（§8）を持たない |
| 421 | `blur_should_hide` | 削除（または観測「純粋核で判定」） | 内部関数名。100ms 猶予・設定切替が契約で、関数名は寄与しない |
| 434 | `position_on_target_monitor` | パス（`window_coordinator.rs` のみ残す） | 「実装は～」のナビゲーション参照。§15.3 L711 が家風の先例（パス + 散文説明のみ、シンボル無し） |
| 444 | `egui_shell::create` | パス | 内部関数名。契約は「両窓に decorations 無しを指定」 |
| 444 | `Window::builder(...).decorations(false)` | 観測「タイトルバー無しで生成」 | API 呼び出し式。観測可能な契約は同文冒頭「タイトルバーは常に非表示」 |
| 445 | `ui.interact` | 観測「入力欄以外の全域のドラッグ検出」 | egui API 名 |
| 445 | `Window::start_dragging()` | 観測「OS のウィンドウ移動を開始」 | API 名 |
| 445 | `drag_window` | 削除 | tao 内部名の重ね書き。1 つ前の降格で不要になる |
| 449 | `visible: false` | 観測「非表示で作成」 | builder パラメータ表記 |
| 449 | `window.show()` | 観測「表示を呼ぶ」 | API 呼び出し |
| 454 | `visible: false` | 観測「非表示で作成」 | 同上 |
| 454 | `focusable(false)` | 観測「フォーカス不可」 | 同 L197 |
| 454 | `drive_results_window` | 削除 | 括弧書きの内部関数名。「main の毎フレーム更新が駆動する」が契約の全て |
| 455 | `SettingsProcessState` | 削除 | 内部型名。「本体が子プロセスを管理し二重起動を防止」が契約 |
| 517 | `open_settings` | 観測「設定を開く操作は no-op」 | 内部関数名（commands/ 実在）。§8.6 の遷移語彙ではない |
| 518 | `is_first_run` | 削除 | 括弧書きフラグ名。「初回起動では」の散文が契約 |
| 546 | `ShowWindow` | 観測「可視化の適用」 | 閉じた競合の歴史記述内。Win32 名は当時の実装詳細で、段落の主語は「評価と適用の間の隔たり」 |
| 558 | `visible: false` | 観測「非表示で生成」 | 同上 |
| 669 | `#[serde(default)]` | 削除 | 実装機構の括弧書き。「欠損キーはデフォルト補完」が同文に既にある |
| 698 | `target_path` | 観測「ターゲットのフルパス」 | 同 L110 |
| 781 | `reset` | パス（`search_state.rs` のみ残す） | 「実装は～の `reset`」ナビゲーション参照。L711 先例に揃える |
| 841 | `Command::new(exe).args(args_vec)` | 観測「シェルを介さず argv 配列で起動」 | std API 式。契約は「引数がシェル解釈されない」であり §19.4 が性質として詳述済み |
| 841 | `/dev/null` | 観測「stdout/stderr は破棄」 | Windows に `/dev/null` は無い。比喩が事実の顔をしている |
| 854 | `apply_migrations()` | 削除 | 内部関数名。「読み込み時に自動で `url` へ変換」が契約 |
| 861 | `Config::default()` | 観測「既定設定に登録済み」 | 同 L345 |
| 903 | `is_empty()` | 削除 | std メソッド括弧書き。「値が空文字列なら」で足りる |
| 958 | `split_args` | 観測「シェル風トークン分割」 | 内部関数名。説明散文が同行に既にある |
| 1005 | `action` | 観測「コマンドの種別に応じてディスパッチ」 | 内部フィールド名（config.rs の `pub action: InstantAction`）。config キーではない（`#[serde]` の外部表現は url/exe/args） |
| 1007 | `InstantAction::Url` | 削除 | enum variant 括弧書き。見出し「URL 種別」が層 1 語彙として確立済み |
| 1010 | `launch_item_core` | 観測「共有の起動経路を再利用（返り値契約は `src-tauri/CLAUDE.md`）」 | 内部関数名。§14.2 が既に同 CLAUDE.md へ正本委譲している |
| 1012 | `InstantAction::Exec` | 削除 | 同 L1007 |
| 1013 | `expand_exec_args(args, query, clipboard, env_expand)` | 観測「引数ベクタを構築（split → env 展開 → 変数置換）」 | 関数シグネチャ。手順は直後の箇条書きが正本 |
| 1016 | `Command::new(exe).args(args_vec)` | 観測「argv 配列で生成」 | 同 L841 |
| 1029 | `spawn` | 観測「プロセス生成」 | 裸の関数名（`ShellExecuteW` は外部 API として残す・後述） |
| 1048 | `interpret` | パス（`search_state.rs` のみ残す） | 「実装は～の `interpret`（クエリからモードを決める）」— 括弧の散文説明だけで足り、L711 先例に揃える |
| 1050 | `reset` | パス | 同 L781 |
| 1120 | `toast_height` | 観測「トースト行高」 | 同 L184。導出「= バー高・既定 43px」は保持（層 5 ペア） |
| 1120 | `bar_height` | 観測「バー高」 | 同上 |
| 1122 | `bar_height` | 観測「バー高」 | 同上 |
| 1145 | `Err` | 観測「失敗復帰時のみ」 | Rust の Result variant。観測可能な契約は「download 失敗等の復帰時」 |

## 2. keep（層 4 だが残す）— 根拠付き

### 2a. 正本参照アンカー（keep 3 件）

| 行 | スパン | 根拠 |
|---|---|---|
| 97 | `icon_cache_cap` | 「導出の正本は `snotra-core/src/config.rs` の `icon_cache_cap`」— SSOT 委譲の宣言。issue が名指しで挙げた意図的参照の形。パス + シンボルで正本を一意に指す（`Config::icon_cache_cap()` 実在照合済み。config.rs L1059 に保守注意コメントの逆リンクもあり双方向） |
| 1035 | `launch_timeout` | 「文言の正本は `strings.rs` の `launch_timeout`」— 同上、issue 名指しの例。実在照合済み |
| 735 | `normalize_entry_key` | `snotra-core/CLAUDE.md`「`normalize_entry_key` の冪等性契約」— 見出し引用の一部であり、見出し参照は機械照合（G-heading-refs の射程）に載る。シンボル単独参照ではない |

**判定規則（今回適用した線引き）**: 「正本は X の Y」= SSOT 委譲は keep。「実装は X の Y」= ナビゲーションはパスで足りるためシンボルを落とす（§15.3 L711 の既存表記「`launcher_controller.rs` のクエリ変化時のコマンド分岐」がシンボル無しの家風先例）。この規則で L434/781/1048/1050 は降格、L97/1035 は keep に振り分けた。

### 2b. 外部 API（挙動そのものが契約の主語・keep 22 出現）

外部 API 名は rename で腐らない（腐るのは自コードのシンボルだけ）。判定は issue の指針どおり「その API の挙動そのものが契約の主語か」。

| 行 | スパン | 根拠 |
|---|---|---|
| 52 | `REG_EXPAND_SZ` | レジストリ値型の展開意味論が契約（展開してから読む） |
| 87 | `SHGetFileInfoW` | シェルのアイコン解決挙動（.lnk 解決・登録タイプアイコン）が契約の主語 |
| 459 | `RegisterHotKey` | OS 登録の成否・シェル予約の挙動が §7.4/§10 全体の契約を規定 |
| 548 | `!Send` | 型システムによるスレッド拘束そのものが機構の契約 |
| 548 | `tauri::Window` / `Manager` | 「残る生の面」を同定するのが目的であり、外部 API の同一性が主語。射程の正本は `src-tauri/CLAUDE.md` へ委譲済み |
| 623 | `DWMWA_WINDOW_CORNER_PREFERENCE` / `DWMWCP_ROUND` | この API の可用性（build 22000+）が「Windows 10 では角丸なし」という受容残余を導く |
| 678 | `ShellExecute` | 「ショートカット本体を渡す（ターゲット変換しない）」というシェル挙動が契約。※表記ゆれ: 他所は `ShellExecuteW`（L821/1009/1029）。W 付きへの統一を推奨（降格ではなく表記整合） |
| 821, 1029 | `ShellExecuteW` | 既定ブラウザ / 既定プログラムのディスパッチ挙動が契約 |
| 1009 | `ShellExecuteW(..., "open", url, ...)` | verb "open" は API 契約の一部。ただし式形は `ShellExecuteW`（"open"）程度へ簡約可（任意） |
| 841 | `CREATE_NO_WINDOW` | Win32 フラグの挙動（コンソール窓を出さない）が契約 |
| 1036 | `spawn_blocking` | tokio の確立した意味論（abandoned task）を比喩の根拠に使う。外部 API ゆえ腐らない |
| 1106 | `tauri-plugin-updater` | クレート名（層 2） |
| 1137 | `Update` / `UpdaterExt` | §20.4 はプラグインの挙動（非復帰・フック合流点）を契約として書き写す節であり、API 同一性が主語 |
| 1138, 1141, 1144 | `on_before_exit` ×3 | 同上。「保存の正しい合流点」という契約はこのフック名でしか指せない |
| 1139, 1140, 1143 | `download_and_install` ×3 | 「Windows では復帰しない」がこの API の挙動契約 |
| 1141 | `std::process::exit(0)` | プラグイン内部挙動の記述（非復帰の機序） |
| 1142 | `app.restart()` | 「使わない」対象の外部 API を名指す否定の知識。理由（NSIS ロック）併記済み |

### 2c. 今回動かさない（負の契約形・4 出現）

issue の例外規定どおり別装置の射程。

- L184 `compute_window_height`（旧・撤去済み #646）
- L399 `get_bootstrap_payload`（旧 IPC・#532 SU7 で消滅）
- L445 `data-tauri-drag-region`（旧 WebView2・消滅）
- L1030 `spawn_blocking`（「旧 WebView2 経路の～に相当する保護」— 消滅した旧経路の名。歴史参照であり現行シンボルではない）

## 3. 境界例と判定

1. **`kana_query`（L123, 126, 136・3 出現）— keep（SPEC 語彙へ編入済みと判定）**。コード変数として実在する（query.rs / query_plan.rs / scoring.rs）が、§4.2 が生成規則ごと定義し、§4.2.1 の不変条件 4 はこの量に名前が無いと述べられない。SPEC が定義側に回っている（層 1 の要件）。ただし rename 時に腐るリスクは残るため、次善案として「かなクエリ」と層 1 命名し直す選択肢を併記する。
2. **`display`（L860 ×2, 987, 990・4 出現）— keep（SPEC 定義の派生値名）**。SPEC 自身が「config フィールドではなくバックエンドが算出する派生値」と定義し §19.5 が参照する。コード上は DTO フィールド `dto.display` として実在し名前が一致するが、定義の主導権は SPEC 側にある。降格するなら「副テキストの既定表示」だが、`description` / `display` の優先関係の記述が崩れるため keep が優位。
3. **`config_watcher`（L379, 392, 1099・3 出現）— keep（層 3 相当）**。`src-tauri/src/config_watcher.rs` というファイル＝モジュール名であり、コンポーネントのファイル名参照は層 3。SPEC は一貫してこの監視サブシステムの呼称として使っている。
4. **`bar_height` / `toast_height` — 降格と判定**（表 1 に計 5 出現）。layout.rs の struct フィールド名と一致し rename で腐る。一方で SPEC は導出（`font_size + bar_padding`、`toast_height = bar_height`）を層 5 ペアで持つため、「バー高」「トースト行高」の日本語呼称＋導出で情報が落ちない。`kana_query` と違い、不変条件の記述にシンボル名そのものは要らない。
5. **`Shell_NotifyIconW`（L583）— keep 寄り**。「Win32 直接実装（tauri トレイプラグインではない）」という実装方式の pin が §2 と併せて意図的。外部 API ゆえ腐らない。同行にパスアンカー（`platform/tray.rs`）もあり、シンボルを落としても情報は保てるので、削る判断も許容（どちらでも欠陥ではない）。
6. **`indexing` / `launching` / `main_visible` / `!indexing` / `indexing == true`（§8.6 周辺）— 層 1**。mermaid フェンス内で定義される状態語彙とそのガード式。issue の指針どおり不動。`hotkey_toggle` / `auto_hide_on_focus_lost` 等ガード内の config キーは層 2。
7. **`last_launched`（L161, 216, 729, 732, 733）— 層 2**。history.bin（永続形式）のフィールド意味論（ms 単位・V2→V3 変換・max 統合）を定義する側。永続形式の語彙はファイル形式に属する。L732 の変換式は層 5 導出ペア（saturating 意味論が契約）。
8. **`panic = "abort"`（L941）— 層 2**。Cargo プロファイル設定（ビルド設定ファイルのキー）であり、「不正書式で abort させない」契約の前提事実。
9. **`trim` の 2 語義**。L117 は std メソッド（降格）、L893 以降はテンプレート修飾子名（層 2・ユーザーが書く構文）。表層形が同じでも概念が別（重複排除の作法どおり語義ごとに判定）。
10. **`appearance.window_width`（L189）— 層 2 で正しい**。`[visual]` ではなく `AppearanceConfig` の serde フィールドであることを config.rs で確認（誤記疑いだったが実在照合で解消）。

## 4. 走査方法（再現手順)

1. `SPEC.md` 全文を Read（HEAD 8d13646・1155 行）。
2. PowerShell でコードフェンス外の全バッククォートスパンを行番号付き抽出（712 件）:
   ```powershell
   $lines = Get-Content C:/workspace/Snotra/SPEC.md -Encoding UTF8
   $inFence = $false
   for ($i=0; $i -lt $lines.Count; $i++) {
     $l = $lines[$i]
     if ($l -match '^\s*```') { $inFence = -not $inFence; continue }
     if ($inFence) { continue }
     foreach ($m in [regex]::Matches($l, '`([^`]+)`')) { "{0}`t{1}" -f ($i+1), $m.Groups[1].Value }
   }
   ```
   （§8.6 mermaid・§19.2 TOML 例・§19.4 例示ブロックはフェンス内として除外——mermaid の状態語彙は層 1、TOML 例は層 2 のため走査対象外で正しい）
3. 全 712 スパンを目視で 5 層へ分類。snake_case は `snotra-core/src/config.rs` の serde フィールド定義（`GeneralConfig` / `SearchConfig` / `AppearanceConfig` / `VisualConfig` / `PathsConfig`）と突き合わせ、config キー（層 2）と実装フィールド（層 4）を判別。
4. 層 4 候補の実在照合: `grep -rln <symbol> snotra-core/src src-tauri/src snotra-egui-runtime/src snotra-settings/src` を全候補（target_path, kana_query, FOLDER_EXPANSION_WEIGHT, open_settings, is_first_run, drive_results_window, blur_should_hide, index_generation, SettingsProcessState, split_args, expand_exec_args, apply_migrations, launch_item_core, InstantAction, normalize_entry_key, position_on_target_monitor, launch_timeout, default_scan_paths, load_icon_pngs, config_watcher, icon_cache_cap, bar_height/toast_height, PlatformCommand variants, LoadOutcome::ReadFailed, display/action フィールド, interpret/reset）に実行し、全て実在を確認。
5. 例外規定の適用: 「旧 X は撤去/消滅」形を除外（4 件）、正本アンカーを keep/降格判定（「正本は」= keep、「実装は」= パス化）、外部 API を「挙動が契約の主語か」で判定。
