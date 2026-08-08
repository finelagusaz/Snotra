//! イベントループスレッド上にいることの証人（`EventLoopProof`）と、そこへ入る唯一の口。
//!
//! **この型は「どのスレッドで走っているか」を型に持ち上げるためだけに在る。** 窓の可視性を
//! 変える操作（main の show/hide・results の raw show/hide）は、書き手が複数スレッドに散ると
//! 「判定してから撃つまで」に他スレッドの逆操作が割り込みうる。証人を引数に要求すれば、
//! 別スレッドからの呼び出しは**コンパイルが通らなくなる**。
//!
//! **相互排他は lock ではなく tao の runner が与える。** `call_event_handler` は
//! `event_handler.take()` してから呼び（非再入）、`send_event` はハンドラ実行中のイベントを
//! `event_buffer` へ回す（tao 0.35.3 `event_loop/runner.rs`）。ゆえにイベントループ上の
//! 2 つの処理は互いに割り込めない。**lock を足してはならない**——窓を所有しないスレッドからの
//! `ShowWindow` は所有スレッドのポンプ待ちでブロックしうるため、イベントループ側も取る lock は
//! race をデッドロックへ化けさせる。

use std::marker::PhantomData;

/// イベントループスレッド上にいることの証人。
///
/// **フィールドは private かつ `PhantomData<*const ()>` である。** 前者はこの crate の外での
/// 構築を防ぎ、後者は `!Send + !Sync` にして**参照ごと別スレッドへ持ち出すこと**を防ぐ
/// （`on_event_loop` が要求する `F: Send` のクロージャにも `std::thread::spawn` にも入らない）。
///
/// 構築点はここに挙げるものに限る: [`crate::RuntimeFrame`]（フレームの中）と [`on_event_loop`]（marshalling したタスクの中）。**足すときは、その経路が本当にイベントループ上かを一次証拠で示すこと。**
pub struct EventLoopProof {
    _not_send: PhantomData<*const ()>,
}

impl EventLoopProof {
    /// **crate 内部専用。** イベントループスレッド上であることが呼び出し側で保証されている
    /// 箇所からのみ呼ぶ。
    pub(crate) fn new() -> Self {
        Self {
            _not_send: PhantomData,
        }
    }
}

/// フレームの外からイベントループスレッドへ入る唯一の口。
///
/// **遅延 primitive ではない。** `AppHandle::run_on_main_thread` は `tauri-runtime-wry` の `send_user_message` へ落ち、**イベントループスレッドから呼ぶとその場で同期・再入的に実行される**（`src/lib.rs:235-255` の `current_thread().id() == context.main_thread_id` 分岐）。別スレッドからは `PostMessageW` で post して即座に戻る。ゆえにフレーム内から出た要求は今日と同じフレーム内順序を保つ。
///
/// **hidden な窓でも走る。** Task の受け口は tao が別に建てる `thread_msg_target`
/// （0×0・`WS_EX_LAYERED` ゆえ不可視・イベントループの寿命と同じ）であり、アプリ窓の可視性とは
/// 無関係である。止まるのはフレーム（`RedrawRequested` の配送）であってタスクではない。
///
/// **送信失敗は握りつぶす。** 失敗するのはイベントループが既に閉じたときで、そのとき窓は
/// もう無い。
pub fn on_event_loop<F>(app: &tauri::AppHandle, f: F)
where
    F: FnOnce(&tauri::AppHandle, &EventLoopProof) + Send + 'static,
{
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        f(&handle, &EventLoopProof::new());
    });
}
