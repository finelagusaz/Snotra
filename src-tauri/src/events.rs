//! アプリ内 Tauri イベント名の定数（#673 spec 決定 3）。
//!
//! **実利は「アプリ内イベント名と payload 契約の一覧が 1 箇所で読める」こと**である。定義へ
//! 飛べば、名前と一緒に payload の約束（値を運ぶのか、合図だけなのか）も読める。
//!
//! **ここに emit / listen の位置は無い——受け口の有無はこのファイルからは分からない。**
//! それがまさに #652 の実形（新しい UI 経路を並走させたとき、旧経路にしかない受け口を複製し
//! 忘れた）であり、定数化で防げないものである。**このファイルを読み切っても網羅の確認には
//! ならない**（定数と doc は受け口が消えても残る）。
//!
//! **文字列リテラルで emit / listen しない**——両端が同じ path を参照して初めて意味を持つ。
//! ただし**これは規範であって機構ではない**（新しいリテラルを弾く lint も CI も無い）。

/// ホットキー押下（platform スレッド → main）。値は運ばない。
pub(crate) const HOTKEY_PRESSED: &str = "hotkey-pressed";

/// 設定変更によるホットキー再登録の失敗。payload は**素の `String`**（`"Alt+Q"` 等の表記）。
/// 旧設定は維持される。受け口が `INITIAL_HOTKEY_FAILED` と逆を向く（wake しない）理由の正本は
/// `egui_shell::register_initial_hotkey_failure_listener` の doc。
pub(crate) const HOTKEY_REGISTRATION_FAILED: &str = "hotkey-registration-failed";

/// 起動時の初回ホットキー登録の失敗。payload は**素の `String`**。受け口は窓を能動表示して
/// から通知する（SPEC §10）。`HOTKEY_REGISTRATION_FAILED` と**受け口の挙動が逆を向く**理由は
/// `egui_shell::register_initial_hotkey_failure_listener` の doc。
pub(crate) const INITIAL_HOTKEY_FAILED: &str = "initial-hotkey-failed";

/// config 適用完了の合図。**値は運ばない**——次フレームの live-read が最新値を拾う
/// （#532 SU6 spec 決定 1。この「値を運ばない」は空振りが benign であることの load-bearing 前提）。
pub(crate) const CONFIG_APPLIED: &str = "config-applied";

/// index 構築の開始の合図。値は運ばない。
pub(crate) const INDEXING_STARTED: &str = "indexing-started";

/// index 構築の完了の合図。値は運ばない（世代は `AppState.index_generation` を読む）。
pub(crate) const INDEXING_COMPLETE: &str = "indexing-complete";

/// view からの hide 要求。値は運ばない。hide の副作用を `hide_egui_main` の 1 経路へ
/// 集約するための内部イベントである（view から直接 hide しない）。
pub(crate) const EGUI_HIDE_REQUESTED: &str = "egui-hide-requested";

/// トレイメニューからの設定画面起動要求。値は運ばない。
pub(crate) const OPEN_SETTINGS: &str = "open-settings";

/// 終了要求（トレイメニュー / `/q` スラッシュコマンド）。値は運ばない。
pub(crate) const EXIT_REQUESTED: &str = "exit-requested";

#[cfg(test)]
mod tests {
    use super::*;

    /// **保証は狭い**: ここに並べた 9 種が互いに異なることだけを見る。定数を新設しても
    /// この配列へ足さなければ検査対象にならない——「将来の追加を守る」機構ではなく、
    /// 現時点のコピペ重複（同じ文字列を持つ 2 定数 = listener の相互誤発火）を弾くだけ。
    #[test]
    fn event_names_are_pairwise_distinct() {
        let all = [
            HOTKEY_PRESSED,
            HOTKEY_REGISTRATION_FAILED,
            INITIAL_HOTKEY_FAILED,
            CONFIG_APPLIED,
            INDEXING_STARTED,
            INDEXING_COMPLETE,
            EGUI_HIDE_REQUESTED,
            OPEN_SETTINGS,
            EXIT_REQUESTED,
        ];
        let mut sorted = all.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "イベント名が重複している");
    }
}
