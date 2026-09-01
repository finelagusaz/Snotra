//! 複数のタブが共有する状態と、その最小限の描画。
//!
//! モーダル Create/Edit 状態と非同期ファイルピッカーは index / opener / instant の 3 タブが
//! 共有し（#439）、[`InlineMessage`] は draft/saved を経由しないタブ（general の
//! 「スタートアップ」節・backup）が共有する。
//!
//! UI（egui の描画呼び出し列）は原則タブごとに固有のまま各タブに残し、ここには
//! 状態遷移・境界チェック・ポーリングといった純ロジックを置く。**例外は
//! [`InlineMessage::show`] の 1 行**——色分けは `style` のトークンを通す規約があり、
//! 状態と色の対応をタブごとに書き写すと片方だけ古い見た目になるため。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

/// モーダルの動作モード。
///
/// - `Create`: フィールド初期化 → Save で Vec に push
/// - `Edit`: 既存値をコピー → Save でインデックス指定で上書き。Delete ボタンも表示
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum ModalMode {
    #[default]
    Create,
    Edit,
}

/// モーダル Create/Edit の共通状態。
///
/// - `F`: タブ固有の編集フィールド群（`Default` が Create 時の初期値）
/// - `I`: 編集対象の位置（既定 `usize`。opener はネストした `(rule, tool)` を使う）
///
/// **インデックス陳腐化ガード**: `editing` はモーダルを開いた時点のスナップショット。
/// モーダル表示中に外部で行が削除されるケース（このアプリでは発生しないが）に備え、
/// Save/Delete は必ず境界チェックを通す（[`save_entry`] / [`delete_entry`]、
/// opener は固有ロジック内でチェック）。
#[derive(Default)]
pub struct ModalState<F: Default, I = usize> {
    pub open: bool,
    pub mode: ModalMode,
    pub editing: Option<I>,
    pub fields: F,
}

impl<F: Default, I> ModalState<F, I> {
    /// Create モードで開く（フィールドは `F::default()` に初期化）。
    pub fn open_create(&mut self) {
        self.open_create_with(F::default());
    }

    /// Create モードで開く（複製など、初期値を指定するケース）。
    pub fn open_create_with(&mut self, fields: F) {
        self.open = true;
        self.mode = ModalMode::Create;
        self.editing = None;
        self.fields = fields;
    }

    /// Edit モードで開く（`index` は開いた時点のスナップショット）。
    pub fn open_edit(&mut self, index: I, fields: F) {
        self.open = true;
        self.mode = ModalMode::Edit;
        self.editing = Some(index);
        self.fields = fields;
    }

    /// モーダルを閉じ、編集スナップショットを破棄する。
    pub fn close(&mut self) {
        self.open = false;
        self.editing = None;
    }

    pub fn is_edit(&self) -> bool {
        self.mode == ModalMode::Edit
    }
}

/// モーダル Save の共通処理: Edit（`editing = Some`）なら境界チェック付きで上書き、
/// Create（`None`）なら push。陳腐化したインデックス（範囲外）は黙って無視する
/// （従来の各タブ実装と同じ挙動）。
pub fn save_entry<T>(vec: &mut Vec<T>, editing: Option<usize>, entry: T) {
    match editing {
        Some(idx) if idx < vec.len() => vec[idx] = entry,
        Some(_) => {}
        None => vec.push(entry),
    }
}

/// モーダル Delete の共通処理: 境界チェック付き remove。範囲外・Create モードは無視。
pub fn delete_entry<T>(vec: &mut Vec<T>, editing: Option<usize>) {
    if let Some(idx) = editing
        && idx < vec.len()
    {
        vec.remove(idx);
    }
}

/// draft/saved を経由しないタブの、即時操作の結果メッセージ。
///
/// フッターの `status` / `status_timer` は draft/saved ワークフロー用で、永続性の要件
/// （エラーは消えてほしくない）と衝突する——この使い分けの規範は
/// `snotra-settings/CLAUDE.md`「フッター vs インラインの使い分け」が正本である。
///
/// **持つのは状態と色分けだけで、周囲の余白や区切りは呼び出し側に残す**（backup は結果領域の
/// 境界として `separator` を挟むが、general の「スタートアップ」節は挟まない）。
#[derive(Default)]
pub struct InlineMessage {
    text: String,
    is_error: bool,
}

impl InlineMessage {
    /// 直近の操作結果で置き換える。**次の操作まで表示を維持する**（タイマーで消さない）。
    pub fn set(&mut self, text: String, is_error: bool) {
        self.text = text;
        self.is_error = is_error;
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// 空でなければ、成否に応じた色でラベルを 1 行描く。
    pub fn show(&self, ui: &mut egui::Ui) {
        if self.text.is_empty() {
            return;
        }
        let color = if self.is_error {
            crate::style::STATUS_ERROR
        } else {
            crate::style::STATUS_SUCCESS
        };
        ui.label(egui::RichText::new(&self.text).color(color));
    }

    /// 表示すべきメッセージを持っているか（呼び出し側が区切りを出すかの判定に使う）。
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// 非同期ファイルピッカー状態（rfd + thread spawn パターン）。
///
/// `rfd::FileDialog` はブロッキング API のため UI スレッドで直接呼ばずスレッドに逃がす。
/// `result` の意味: `None` = 実行中, `Some(None)` = キャンセル, `Some(Some(path))` = 選択。
/// `active` が true の間は Browse ボタンを無効化する。
#[derive(Clone, Default)]
pub struct PickerState {
    pub result: Arc<Mutex<Option<Option<PathBuf>>>>,
    pub active: bool,
}

impl PickerState {
    /// 毎フレームの非ブロッキングポーリング。ダイアログが完了していれば結果を取り出し
    /// `active = false` に戻す（キャンセル時も必ず戻す＝ボタン永久無効化バグの防止を
    /// 1箇所に集約）。返り値: `Some(Some(path))` = 選択, `Some(None)` = キャンセル,
    /// `None` = 未完了または非アクティブ。
    pub fn poll(&mut self) -> Option<Option<PathBuf>> {
        if !self.active {
            return None;
        }
        let taken = self
            .result
            .try_lock()
            .ok()
            .and_then(|mut guard| guard.take());
        if taken.is_some() {
            self.active = false;
        }
        taken
    }

    /// ダイアログをバックグラウンドスレッドで起動する。`active = true` にし（対になる
    /// `false` は [`PickerState::poll`] が結果取得時に戻す）、完了時に `request_repaint`
    /// で UI 更新をトリガーする。
    pub fn launch(
        &mut self,
        ctx: &egui::Context,
        dialog: impl FnOnce() -> Option<PathBuf> + Send + 'static,
    ) {
        self.active = true;
        let result = Arc::clone(&self.result);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            *result.lock().unwrap() = Some(dialog());
            repaint_ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, PartialEq, Debug)]
    struct Fields {
        text: String,
    }

    fn fields(s: &str) -> Fields {
        Fields {
            text: s.to_string(),
        }
    }

    // 不変条件: open_create はフィールドを Default に初期化し editing を消す
    #[test]
    fn modal_open_create_resets_fields_and_editing() {
        let mut m: ModalState<Fields> = ModalState::default();
        m.open_edit(3, fields("old"));
        m.open_create();
        assert!(m.open);
        assert_eq!(m.mode, ModalMode::Create);
        assert_eq!(m.editing, None);
        assert_eq!(m.fields, Fields::default());
    }

    // 不変条件: open_create_with（複製）は Create モードのままフィールドだけ引き継ぐ
    #[test]
    fn modal_open_create_with_keeps_create_mode() {
        let mut m: ModalState<Fields> = ModalState::default();
        m.open_create_with(fields("dup"));
        assert_eq!(m.mode, ModalMode::Create);
        assert_eq!(m.editing, None);
        assert_eq!(m.fields, fields("dup"));
    }

    // 不変条件: open_edit はスナップショット index とフィールドを保持する
    #[test]
    fn modal_open_edit_sets_snapshot() {
        let mut m: ModalState<Fields> = ModalState::default();
        m.open_edit(2, fields("row2"));
        assert!(m.open && m.is_edit());
        assert_eq!(m.editing, Some(2));
        assert_eq!(m.fields, fields("row2"));
    }

    // 不変条件: close は editing スナップショットを必ず破棄する
    #[test]
    fn modal_close_clears_editing() {
        let mut m: ModalState<Fields> = ModalState::default();
        m.open_edit(1, fields("x"));
        m.close();
        assert!(!m.open);
        assert_eq!(m.editing, None);
    }

    // 不変条件: ネスト index（opener の (rule, tool)）も同じ状態遷移で扱える
    #[test]
    fn modal_supports_nested_index() {
        let mut m: ModalState<Fields, (usize, usize)> = ModalState::default();
        m.open_edit((1, 2), fields("nested"));
        assert_eq!(m.editing, Some((1, 2)));
        m.close();
        assert_eq!(m.editing, None);
    }

    // 不変条件: save_entry は Create で push、Edit（範囲内）で置換、範囲外で no-op（panic しない）
    #[test]
    fn save_entry_create_pushes() {
        let mut v = vec![1, 2];
        save_entry(&mut v, None, 3);
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn save_entry_edit_replaces_in_bounds() {
        let mut v = vec![1, 2, 3];
        save_entry(&mut v, Some(1), 9);
        assert_eq!(v, vec![1, 9, 3]);
    }

    #[test]
    fn save_entry_stale_index_is_noop() {
        let mut v = vec![1];
        save_entry(&mut v, Some(5), 9);
        assert_eq!(v, vec![1]);
    }

    // 不変条件: delete_entry は範囲内で remove、範囲外/Create で no-op（panic しない）
    #[test]
    fn delete_entry_in_bounds_removes() {
        let mut v = vec![1, 2, 3];
        delete_entry(&mut v, Some(1));
        assert_eq!(v, vec![1, 3]);
    }

    #[test]
    fn delete_entry_stale_or_create_is_noop() {
        let mut v = vec![1];
        delete_entry(&mut v, Some(5));
        delete_entry(&mut v, None);
        assert_eq!(v, vec![1]);
    }

    // 不変条件: poll は非アクティブ時 None、結果到着時に take + active=false（キャンセルでも戻す）
    #[test]
    fn picker_poll_inactive_returns_none() {
        let mut p = PickerState::default();
        assert_eq!(p.poll(), None);
    }

    fn active_picker() -> PickerState {
        PickerState {
            active: true,
            ..Default::default()
        }
    }

    #[test]
    fn picker_poll_pending_keeps_active() {
        let mut p = active_picker();
        assert_eq!(p.poll(), None);
        assert!(p.active, "結果未着では active を維持する");
    }

    #[test]
    fn picker_poll_takes_result_and_deactivates() {
        let mut p = active_picker();
        *p.result.lock().unwrap() = Some(Some(PathBuf::from("C:/x.exe")));
        assert_eq!(p.poll(), Some(Some(PathBuf::from("C:/x.exe"))));
        assert!(!p.active);
        assert_eq!(p.poll(), None, "結果は take 済みで再取得されない");
    }

    #[test]
    fn picker_poll_cancel_also_deactivates() {
        let mut p = active_picker();
        *p.result.lock().unwrap() = Some(None);
        assert_eq!(p.poll(), Some(None));
        assert!(
            !p.active,
            "キャンセルでも active を戻す（ボタン永久無効化の防止）"
        );
    }
}
