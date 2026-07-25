use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tauri_runtime::{UserEvent, window::WindowId};
use tauri_runtime_wry::{Message, WindowMessage, tao::event_loop::EventLoopProxy};

enum SchedulerMessage {
    Request { deadline: Instant },
    Stop,
}

/// 窓を外部（別スレッド・別窓・Tauri イベントリスナー）から起こすハンドル。
/// `EguiRuntime::attach` が窓ごとに 1 つ返す（#671 PR D）。
///
/// **窓ごとの `egui::Context` を clone して外へ配る代わりに、repaint worker への送信側
/// だけを渡す。** 前者は Context に括り付いた repaint callback ごと clone するため、
/// callback が握る `RepaintScheduler` の Arc が窓の `Destroyed` を越えて生き残り、
/// `SchedulerInner::drop`（stop + join）を止めていた。
///
/// このハンドルは `RepaintScheduler` の Arc を持たないため、**永久保持しても停止を
/// 妨げない**——`SchedulerInner::drop` は `Stop` を明示送信してから join するので、
/// チャネルの切断（全 Sender の drop）を待たない。
///
/// **活性化前の `wake()` は queue される**（イベントループが窓を活性化した直後に 1 回
/// 描画要求として現れる）。活性化自身も `request(ZERO)` を撃つため実効差は無い。
#[derive(Clone)]
pub struct WindowWaker {
    sender: Sender<SchedulerMessage>,
}

impl WindowWaker {
    /// 次フレームを要求する。窓が既に破棄されていれば無害な no-op
    /// （失敗を呼び出し側へ伝えないのが契約——呼び出し点は wake の成否を分岐しない）。
    pub fn wake(&self) {
        self.request(Duration::ZERO);
    }

    fn request(&self, delay: Duration) {
        let _ = self.sender.send(SchedulerMessage::Request {
            deadline: Instant::now() + delay,
        });
    }

    fn stop(&self) {
        let _ = self.sender.send(SchedulerMessage::Stop);
    }
}

/// wake 経路の受信側。`attach`（任意スレッド）が作り、活性化時にイベントループで
/// `RepaintScheduler::new` が消費する。worker 用の送信側を同梱するのは、活性化側が
/// 送信側を別経路で受け取らずに済むため（`attach` は送信側を呼び出し元へ返す）。
pub(crate) struct WakeReceiver {
    sender: Sender<SchedulerMessage>,
    receiver: Receiver<SchedulerMessage>,
}

/// wake 経路の 1 対を作る（`EguiRuntime::attach` が窓ごとに呼ぶ）。
pub(crate) fn wake_channel() -> (WindowWaker, WakeReceiver) {
    let (sender, receiver) = mpsc::channel();
    (
        WindowWaker {
            sender: sender.clone(),
        },
        WakeReceiver { sender, receiver },
    )
}

#[derive(Clone)]
pub(crate) struct RepaintScheduler {
    inner: Arc<SchedulerInner>,
}

struct SchedulerInner {
    waker: WindowWaker,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl RepaintScheduler {
    /// worker を起動する。`wake` は `attach` が作った受信側（`wake_channel`）。
    pub(crate) fn new<T: UserEvent>(
        proxy: EventLoopProxy<Message<T>>,
        window_id: WindowId,
        wake: WakeReceiver,
    ) -> Self {
        let WakeReceiver { sender, receiver } = wake;

        let worker = thread::Builder::new()
            .name("snotra-egui-repaint".to_owned())
            .spawn(move || {
                let mut pending: Option<Instant> = None;

                loop {
                    let message = match pending {
                        Some(deadline) => {
                            let timeout = deadline.saturating_duration_since(Instant::now());
                            match receiver.recv_timeout(timeout) {
                                Ok(message) => Some(message),
                                Err(RecvTimeoutError::Timeout) => None,
                                Err(RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        None => match receiver.recv() {
                            Ok(message) => Some(message),
                            Err(_) => break,
                        },
                    };

                    match message {
                        Some(SchedulerMessage::Stop) => break,
                        Some(SchedulerMessage::Request { deadline }) => {
                            pending = match pending {
                                Some(current_deadline) if current_deadline <= deadline => {
                                    Some(current_deadline)
                                }
                                _ => Some(deadline),
                            };
                        }
                        None => {
                            let Some(_) = pending.take() else {
                                continue;
                            };
                            if proxy
                                .send_event(Message::Window(
                                    window_id,
                                    WindowMessage::RequestRedraw,
                                ))
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
            })
            .expect("repaint worker thread creation should succeed");

        Self {
            inner: Arc::new(SchedulerInner {
                waker: WindowWaker { sender },
                worker: Mutex::new(Some(worker)),
            }),
        }
    }

    pub(crate) fn request(&self, delay: Duration) {
        self.inner.waker.request(delay);
    }
}

impl Drop for SchedulerInner {
    fn drop(&mut self) {
        // Stop を**明示送信**してから join する。外部が持つ `WindowWaker`（= 同じチャネルの
        // 送信側）が生きていてもチャネルは切断されないため、この明示送信が停止の根拠である。
        self.waker.stop();
        if let Some(worker) = self.worker.lock().expect("repaint worker lock").take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SchedulerMessage, wake_channel};

    #[test]
    fn wake_before_activation_is_queued() {
        // 活性化前（worker 未起動）の wake は落ちずに queue される。活性化後の最初の
        // 1 回として現れるため、setup〜初フレームの窓で要求が消えない。
        let (waker, wake_rx) = wake_channel();
        waker.wake();
        waker.wake();
        assert!(matches!(
            wake_rx.receiver.try_recv(),
            Ok(SchedulerMessage::Request { .. })
        ));
        assert!(matches!(
            wake_rx.receiver.try_recv(),
            Ok(SchedulerMessage::Request { .. })
        ));
        assert!(wake_rx.receiver.try_recv().is_err(), "3 通目は無い");
    }

    #[test]
    fn wake_after_receiver_drop_is_silent() {
        // 窓が Destroyed になり worker（受信側）が落ちた後の wake は無害な no-op。
        // panic せず、呼び出し側に失敗を伝えないこと自体が契約である。
        let (waker, wake_rx) = wake_channel();
        drop(wake_rx);
        waker.wake();
    }
}
