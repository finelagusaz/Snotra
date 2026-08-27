//! 検索の発行（打鍵 → debounce → worker への要求）と、その結果の採り込み（#1004 PR 2）。
//!
//! **行を差し替える点の主たる面はここである**——`run_search_with` の view_kind 分岐・
//! `drain_search`・`clear_search` が `SearchState` の `set_results` / `accept_worker_rows` を呼ぶ。
//! **ここだけではない**: `activation.rs` も撃つ（`start_launch` の in-flight 失効と `on_enter` の
//! flush。どちらも理由は当該 doc が持ち、前者は `layout.rs` の表示ゲートの導出根拠でもある）。
//! **差し替え点を数え上げないこと**——足すたびにこの行だけが腐る。母集団は
//! `git grep 'state\.set_results('` が持つ。
//!
//! ここに**無いもの**:
//!
//! - **世代の加算と in-flight の失効**は [`crate::egui_shell::search_state`] の内側にある
//!   （#699 / #1039）。行が差し替わったことと世代が進むことを一致させるため、両者は同じ場所に置く
//! - **folder の列挙とその drain** は `folder_nav.rs`。ここの `run_search_with` は届いた
//!   cache / error を同期でフィルタするだけである

use std::time::Instant;

use snotra_core::config::SearchConfig;
use tauri::Manager;

use super::LauncherController;
use crate::egui_shell::{QueryIntent, SlashCmd, ViewKind, find_slash_command};

impl LauncherController {
    /// クエリ/結果/armed trailing/instant 来歴をまとめてクリアする単一チョークポイント
    /// （execute_slash・instant 成功経路・`execute_tool_selected` が共有）。追加のクリアサイトが
    /// `search_debounce.cancel()` を書き忘れて「クリア後に stale trailing 検索が発火」を
    /// 再発させないための集約（/code-review #637 finding 6）。
    pub(super) fn clear_search(&mut self) {
        self.state.set_query(String::new());
        self.state.set_results(Vec::new());
        self.search_debounce.cancel();
        self.instant_rows_query = None;
    }

    /// instant prefix を実行中 config から都度読む（キャッシュしない・#576 と同設計）。
    /// フィールドは `SearchConfig::instant_command_prefix`（既定は同 struct の `Default` 実装）。
    ///
    /// **この読みは #1076 で `engine.lock()` から [`crate::egui_shell::read_config`] へ移した。**
    /// 呼び出し点（`run_search` / 打鍵の changed エッジ / `on_enter`）はどれも毎フレームではないが、
    /// **どれも egui フレームの中にあり、ユーザーが待っている**——#1032 の規範が挙げる害は
    /// 「フレームが走査の完了まで返らない」ことであって頻度ではない（射程の定義は
    /// `src-tauri/CLAUDE.md`「モジュール構成」の #1032 条項が正本）。
    pub(super) fn instant_prefix(&self) -> String {
        crate::egui_shell::read_config(
            &self.app_handle,
            |c| c.search.instant_command_prefix.clone(),
            || SearchConfig::default().instant_command_prefix,
        )
    }

    /// view_kind 先の同期 dispatch（#532 SU3 M2）。folder は cache/error を同期フィルタ、
    /// results は M1 の interp 分岐（plain 検索）。folder 打鍵が engine.search へ漏れない。
    /// prefix を内部で取得する薄いラッパー（trailing poll・folder drain 用）。changed エッジは
    /// 取得済み prefix を `run_search_with` へ渡し、毎打鍵の config の読みの回数を減らす
    /// （/code-review #637 finding 9。当時は engine lock の回数だった・#1076 で読み口が移った）。
    pub(super) fn run_search(&mut self) {
        let prefix = self.instant_prefix();
        self.run_search_with(&prefix);
    }

    fn run_search_with(&mut self, prefix: &str) {
        // 世代の加算も in-flight の失効もここには無い（#699 / #1039）——`SearchState` の
        // 行差し替え点が両方を持つ。この関数は folder cache 未着などで `set_results` を
        // 呼ばずに返る経路があり、ここで無条件に進めると「行は変わっていないのに世代だけ
        // 進む」空撃ちになる。世代の意味を「行が差し替わった」と一致させるため、加算は
        // 差し替えと同じ場所に置く。
        // 来歴は行と一体で更新する（Instant 分岐だけが Some を立て直す・finding 0）。
        self.instant_rows_query = None;
        match self.state.view_kind() {
            ViewKind::Folder => {
                if let Some(err) = &self.folder_error {
                    self.state.set_results(err.clone()); // 列挙失敗行（filter 非適用）
                } else if let Some((ctx, sorted)) = &self.folder_cache {
                    let filtered = ctx.filter_sorted(sorted, self.state.folder_filter());
                    self.state.set_results(filtered);
                }
                // cache 未着（ロード中）は前フレーム結果を保持（フリット無し・set しない）
            }
            ViewKind::Tool => {
                // §18.5: ツール選択中は検索結果を上書きしない（trailing/changed とも no-op）
            }
            ViewKind::Results => {
                match self.state.interp(prefix) {
                    QueryIntent::Plain => {
                        if self.state.query().trim().is_empty() || self.indexing().get() {
                            // 空クエリと構築中は**同期でクリアする**（worker を経由させると消した文字が 1 フレーム残る）。同期で差し替える以上 in-flight は失効するが、それは `SearchState` の内側で起きる（#1039）。
                            self.state.set_results(Vec::new());
                            return;
                        }
                        let query = self.state.query().to_string();
                        let seq = self.state.issue_search(self.last_input_at, Instant::now());
                        // 送信できたなら**結果が届くまで前の行を保つ**（folder cache 未着枝と同じ扱い）。
                        if self
                            .search_tx
                            .send(crate::egui_shell::SearchRequest { seq, query })
                            .is_err()
                        {
                            // **worker が死んでいる。** 無界チャネルゆえ `Err` はこれ以外を意味せず
                            // （混雑・一時的失敗が存在しない）、死因（早期 return・panic・将来足す
                            // 経路）を問わず必ずここを通る——死因ごとの塞ぎ方より射程が広い。
                            // **再送では回復しない**（受け手はもう居ない）ため、前の行を保つと
                            // 保持が恒久化し、debounce が armed でない Enter が旧クエリの項目を
                            // 起動する（`on_enter` の flush が「どちらの枝でもクリアする」理由と
                            // 同じ危険である）。上の空クエリ枝と同じ処置へ合流させる。
                            self.state.set_results(Vec::new());
                        }
                    }
                    QueryIntent::Instant {
                        filter_name,
                        instant_query,
                    } => {
                        // §19.5: 前方一致フィルタ。毎打鍵同期（30ms debounce 撤廃・spec M3 実装確定）。
                        // indexing を見ない（§19.7: instant はインデックス非依存ゆえ構築中でも使用可）。
                        // 絞り込みと表示規則は `snotra_core::instant::matching_results` が持つ純関数で、
                        // ここは config の読みだけを担う（#1124 で `commands/instant.rs` から移した——
                        // かつて IPC コマンドと共有していた相手は #532 SU7 で消滅していた）。
                        //
                        // **fallback が空なのは、AppState 不在＝config 未ロードだからである**（#1124）。
                        // 既定の instant コマンドを返すと「たまたま既定と一致する利用者にだけ正しい」
                        // 行が出て、**しかもその行は起動できない**——`execute_instant_selected` の
                        // 読みは同じ不在で `None` へ落ち、`egui_instant_error` を残して何もしない。
                        // 空へ倒せば、起動できない行がそもそも出ない。
                        // なお不在そのものが実運用で起きない（`.manage` は `.setup` より前・根拠は
                        // `crate::egui_shell::read_config` のコメントと
                        // ADR-config-default-fallback-references）。
                        let rows = crate::egui_shell::read_config(
                            &self.app_handle,
                            |c| {
                                // **アイコンキーの env 展開は起動側と同じ関数を通す**（#1133）
                                // ——`launch_exec_core` が `expand_env(exe)` を実行するので、
                                // ここで別の展開（あるいは無展開）にすると、`%VAR%` を含む exe が
                                // **起動できるのにアイコンだけ出ない**。`ExpandEnvironmentStringsW`
                                // 1 発ぶんが read guard の中に入るが、アプリの錠も I/O も取らず不定時間
                                // ブロックしないので `AppState::read_config` の契約に反しない。
                                snotra_core::instant::matching_results(
                                    &c.instant_commands,
                                    &filter_name,
                                    crate::commands::launch::expand_env,
                                )
                            },
                            Vec::new,
                        );
                        // 来歴 snapshot: この行集合が instant 候補であることと、その時点の
                        // instant_query を一体で記録する（activate_or_execute が参照・finding 0）。
                        self.instant_rows_query = Some(instant_query);
                        self.state.set_results(rows);
                    }
                    QueryIntent::Command => {
                        // §15.2 /r: 履歴を注入して留まる（冪等ゆえ trailing 再発火も無害）。
                        // 他（部分入力・実行済み直後）は候補なしクリア（§15.3: command 中は検索しない）。
                        if matches!(
                            find_slash_command(self.state.query()),
                            Some(SlashCmd::History)
                        ) {
                            let rows = {
                                let state = match self.app_handle.try_state::<crate::AppState>() {
                                    Some(s) => s,
                                    None => return,
                                };
                                let engine = state.engine.lock().unwrap();
                                engine.recent_history()
                            };
                            self.state.set_results(rows);
                        } else {
                            self.state.set_results(Vec::new());
                        }
                    }
                }
            }
        }
    }

    /// worker の結果を採り込む（#1004）。**判定は `SearchState` の内側にある**（#1039）——seq が現
    /// pending と一致し、かつ Results ビューにいるときだけ行が差し替わり、世代と in-flight の失効も
    /// そこで同時に起きる（#699 は無傷）。
    ///
    /// **捨てた理由（`egui_search:dropped` の `"reason"`）は、採り込みを呼ぶ前の `pending_seq` から
    /// 導く**（`SearchState::accept_worker_rows` は 2 種の `None` を区別しない）。`pending_seq() == seq`
    /// のまま `None` が返るのは view ガードの発火だけであり、**ガードの効きを実機で観測しているのは
    /// この区別である**——区別しないと、新設したガードが一度も効いていなくても件数からは分からない。
    ///
    /// **この 2 行の順序を固定する検知器は無い。** 入れ替えると view のケースでも `accept` が既に
    /// pending を take しているため `was_current` が常に偽になり、`reason` が恒久的に `"seq"` へ化ける
    /// ——**この reason を足した理由そのものが静かに失われる**。`SearchState` 側の
    /// `pending_seq_separates_the_two_drop_reasons` は純粋核の内側を測るので、ここの入れ替えでは落ちない。
    pub(in crate::egui_shell) fn drain_search(&mut self) {
        while let Ok(crate::egui_shell::SearchMsg::Done {
            seq,
            results,
            index_entries,
        }) = self.search_rx.try_recv()
        {
            let now = Instant::now();
            // **採り込みより前に読む**——`accept` が pending を take するため、後では区別できない。
            let was_current = self.state.pending_seq() == seq;
            let Some(settled) = self.state.accept_worker_rows(seq, results, now) else {
                crate::trace::trace(
                    "egui_search:dropped",
                    serde_json::json!({
                        "dispatch_seq": seq,
                        "pending_seq": self.state.pending_seq(),
                        // "view" = Folder ビューへ遷移していた（#1039 のガード）。ガードは Tool も
                        //          弾くが、そちらは `enter_tool` が in-flight を失効させるため
                        //          production では到達しない（`accept_worker_rows` の doc）。
                        // "seq"  = 追い越された、または同期で差し替えて失効していた。
                        "reason": if was_current { "view" } else { "seq" },
                    }),
                );
                continue;
            };
            crate::trace::trace(
                "egui_search:settled",
                serde_json::json!({
                    "dispatch_seq": settled.seq,
                    "pending_seq": self.state.pending_seq(),
                    "index_entries": index_entries,
                    "since_key_us": settled.since_key.as_micros() as u64,
                    "since_dispatch_us": settled.since_dispatch.as_micros() as u64,
                }),
            );
        }
    }

    /// 段 22: TextEdit が `changed()` を返したフレームの処置。`buf` は編集後のバッファ、
    /// `in_folder` は**その TextEdit を組み立てたときの** view_kind（段 21 で読んだ値をそのまま
    /// 渡す——ここで読み直すと同一フレーム内で 2 つの真実ができる）。
    pub(in crate::egui_shell) fn on_input_changed(
        &mut self,
        buf: String,
        in_folder: bool,
        ctx: &egui::Context,
    ) {
        if crate::trace::trace_enabled() {
            // #840 の実機回帰検査用。入力文字列そのものは診断ログへ残さず、変更前後の
            // 文字数と「旧文字列を prefix に持つ増加か」だけで末尾追記を観測する。
            // state 更新より前でなければ比較元を失うため、この位置を保つ。
            let previous = if in_folder {
                self.state.folder_filter()
            } else {
                self.state.query()
            };
            let before_chars = previous.chars().count();
            let after_chars = buf.chars().count();
            crate::trace_main(
                "egui_input:changed",
                serde_json::json!({
                    "scope": if in_folder { "folder" } else { "search" },
                    "before_chars": before_chars,
                    "after_chars": after_chars,
                    "appended_at_end": after_chars > before_chars && buf.starts_with(previous),
                }),
            );
        }
        if in_folder {
            self.state.set_folder_filter(buf);
            self.run_search(); // folder は同期フィルタ（debounce 不要・I/O 無し）
        } else {
            self.state.set_query(buf);
            // `SPEC.md`「4.9 入力と選択」の実体（folder 側の対は `set_folder_filter` 内の
            // `selected = 0`）。
            self.state.reset_selection(); // SolidJS parity: 毎打鍵 selected=0（M1 gap 是正）
            // prefix はこの changed エッジで 1 回だけ取得し run_search_with へ渡す
            //（interp と合わせ config の毎打鍵多重読みを避ける・finding 9）。
            let prefix = self.instant_prefix();
            match self.state.interp(&prefix) {
                QueryIntent::Plain => {
                    self.last_input_at = Instant::now();
                    if self.search_debounce.on_input() {
                        self.run_search_with(&prefix); // leading
                    }
                    ctx.request_repaint_after(self.search_debounce.interval());
                }
                QueryIntent::Instant { .. } => {
                    // 同期直フィルタ（30ms debounce 撤廃・spec M3 実装確定）。
                    // plain 由来の armed trailing は掃除（cancelDebounce parity）。
                    self.search_debounce.cancel();
                    self.run_search_with(&prefix);
                }
                QueryIntent::Command => {
                    // §15.3: debounce をキャンセルして即実行（changed エッジ＝query 変化時
                    // のみゆえ immediate-mode でも fire-once）。/r と部分入力は run_search の
                    // Command 分岐（冪等: /r=履歴注入・他=結果クリア）。
                    self.search_debounce.cancel();
                    match find_slash_command(self.state.query()) {
                        Some(SlashCmd::History) | None => self.run_search_with(&prefix),
                        Some(cmd) => self.execute_slash(cmd),
                    }
                }
            }
        }
    }

    /// 段 27: trailing debounce の poll と再 arm。
    pub(in crate::egui_shell) fn poll_search_debounce(&mut self, ctx: &egui::Context) {
        // trailing debounce: 連打が収まって interval 経過したら最終クエリで検索し直す。
        if self.search_debounce.poll(self.last_input_at.elapsed()) {
            self.run_search();
        }
        // armed のまま = trailing 未発火。scheduler の coalescing で +interval の wake が
        // 消されても deadline で確実に起きるよう毎フレーム残り時間を再要求する。
        if self.search_debounce.is_armed() {
            let remaining = self
                .search_debounce
                .interval()
                .saturating_sub(self.last_input_at.elapsed());
            ctx.request_repaint_after(remaining);
        }
    }
}
