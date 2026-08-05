//! 即時／遅延 repaint を Tauri イベントループへ配送する worker（窓ごと 1 スレッド）。
//! 配送には下限間隔がある（フレーム上限＝モニターのリフレッシュレート・取得失敗時 60Hz・
//! contract-design spec 契約②・#737）——gate は要求 deadline を**早めも取りこぼしもしない**。
//! 外部から窓を起こす公開ハンドル `WindowWaker`（`EguiRuntime::attach` の戻り値）もここが所有する。

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
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

/// repaint 原因列を 1 行の trace 文字列へ（`file:line reason` を `; ` 区切り）。
///
/// 空列は `-` を返す——「原因が無い（入力イベント起因のフレーム）」と「トレースが
/// 壊れた」を出力で区別するため。`RepaintCause` の `Display` は `{file}:{line} {reason}`。
pub(crate) fn format_repaint_causes(causes: &[egui::RepaintCause]) -> String {
    if causes.is_empty() {
        return "-".to_owned();
    }
    causes
        .iter()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// 60Hz の配送間隔（ナノ秒）。取得失敗時のフォールバック（#737・ユーザー合意 2026-07-26）。
const FALLBACK_INTERVAL_NANOS: u64 = 1_000_000_000 / 60;

/// リフレッシュレート(Hz)から配送の下限間隔へ（契約②・#737）。
/// 0/1 は DEVMODE の慣習で「取得できていない」を意味する番兵値なので None
/// （呼び出し側 `set_min_interval` が 60Hz フォールバックへ倒す）。
fn interval_from_hz(hz: u32) -> Option<Duration> {
    // 上限 1000Hz: 異常値（間隔 0ns で gate が無効化する類）を防ぐ現実的な天井（レビュー L4）。
    (2..=1_000)
        .contains(&hz)
        .then(|| Duration::from_nanos(1_000_000_000u64 / u64::from(hz)))
}

/// 次の dispatch 予定時刻（契約②・#737）: 要求 deadline と gate（前回 dispatch +
/// min_interval）の遅い方。gate は「要求より早く配送しすぎない」制御であって、
/// **要求 deadline を早めも取りこぼしもしない**——契約②が契約③の「armed の間は毎フレーム
/// 再要求する」規範と両立する根拠がこれである（gate より遅い予約を早める機構でもない）。
///
/// **契約③を「予約はフレーム 1 枚以上を約束する」と要約してはならない**——その表現は
/// #711 で偽と判明して撤回された（`pending` は単一スロットで、より早い要求が割り込むと
/// 後の deadline は `take()` ごと消える）。gate の性質と予約の消失は別の話である。
fn pace(deadline: Instant, gate: Instant) -> Instant {
    deadline.max(gate)
}

#[derive(Clone)]
pub(crate) struct RepaintScheduler {
    inner: Arc<SchedulerInner>,
}

struct SchedulerInner {
    waker: WindowWaker,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// 配送の下限間隔（ナノ秒・契約②・#737）。worker が dispatch のたび読む level-triggered 値。
    /// 書き手は plugin（活性化時 + モニター変化時 + `Focused(true)`・`runtime.rs`）。
    /// clone は worker スレッド生成**前**に作って closure へ渡してある（`wake_channel` と同じ順序制約
    /// ——`SchedulerInner` は spawn 後に構築されるため、ここ経由では worker から読めない）。
    min_interval_nanos: Arc<AtomicU64>,
}

impl RepaintScheduler {
    /// worker を起動する。`wake` は `attach` が作った受信側（`wake_channel`）。
    pub(crate) fn new<T: UserEvent>(
        proxy: EventLoopProxy<Message<T>>,
        window_id: WindowId,
        wake: WakeReceiver,
    ) -> Self {
        let WakeReceiver { sender, receiver } = wake;
        let min_interval_nanos = Arc::new(AtomicU64::new(FALLBACK_INTERVAL_NANOS));
        let min_interval_for_worker = Arc::clone(&min_interval_nanos);

        let worker = thread::Builder::new()
            .name("snotra-egui-repaint".to_owned())
            .spawn(move || {
                let mut pending: Option<Instant> = None;
                // 前回 dispatch + min_interval（契約②の gate）。初期値は起動時刻——活性化直後の
                // 初回フレーム（ウォームアップ）は遅らせない。
                let mut next_allowed: Instant = Instant::now();

                loop {
                    let message = match pending {
                        Some(deadline) => {
                            // 待ち先は pace(deadline, gate)。pending の意味論（最も早い要求
                            // deadline）は変えない——gate へ書き戻すと、後着の早い Request が
                            // pending を引き戻すたび早発 wake が生じる。Timeout 到達 = gate 通過。
                            //
                            // Request 洪水（マウス移動 ≤~1kHz）でも満期 arm は飢餓しない:
                            // メッセージ処理は ns 級で queue は生産より速く枯れ、空 queue の
                            // recv_timeout が Timeout を返してここへ戻る（/race-check #737）。
                            let target = pace(deadline, next_allowed);
                            let timeout = target.saturating_duration_since(Instant::now());
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
                            // hidden 中の抑止点の切り分け計器（#697）。受信側は runtime.rs の
                            // RedrawRequested arm。ここが出て受信側が出なければ、落としたのは
                            // proxy 以降である（wry の user-message 処理は窓の引き当てに成功
                            // すれば request_redraw() へ渡すだけで、hidden でも非破棄なら
                            // 引き当ては成功する——ゆえに実体は tao/OS 層）。
                            if crate::env::trace_hatch_enabled("SNOTRA_EGUI_WAKE_TRACE") {
                                eprintln!("SNOTRA_EGUI_WAKE_SEND window_id={window_id:?}");
                            }
                            if proxy
                                .send_event(Message::Window(
                                    window_id,
                                    WindowMessage::RequestRedraw,
                                ))
                                .is_err()
                            {
                                break;
                            }
                            // 契約②: 次の dispatch は min_interval 以上あける（gate 更新）。
                            next_allowed = Instant::now()
                                + Duration::from_nanos(
                                    min_interval_for_worker.load(Ordering::Relaxed),
                                );
                        }
                    }
                }
            })
            .expect("repaint worker thread creation should succeed");

        Self {
            inner: Arc::new(SchedulerInner {
                waker: WindowWaker { sender },
                worker: Mutex::new(Some(worker)),
                min_interval_nanos,
            }),
        }
    }

    pub(crate) fn request(&self, delay: Duration) {
        self.inner.waker.request(delay);
    }

    /// 配送の下限間隔をリフレッシュレートで設定する（契約②・#737）。
    /// `None` と番兵値（0/1）は 60Hz フォールバックへ倒す。反映は次の dispatch から
    /// （遅れて効いても最大 1 dispatch の誤差で自己回復する level-triggered 値）。
    pub(crate) fn set_min_interval(&self, hz: Option<u32>) {
        let interval = hz
            .and_then(interval_from_hz)
            .map_or(FALLBACK_INTERVAL_NANOS, |d| d.as_nanos() as u64);
        self.inner
            .min_interval_nanos
            .store(interval, Ordering::Relaxed);
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
    use super::{SchedulerMessage, format_repaint_causes, wake_channel};
    use super::{interval_from_hz, pace};
    use std::time::{Duration, Instant};

    #[test]
    fn interval_from_hz_rejects_sentinel_and_converts() {
        // 0/1 は DEVMODE の「取得できていない」番兵値——間隔にせず None（60Hz へ倒すのは呼び出し側）。
        assert_eq!(interval_from_hz(0), None);
        assert_eq!(interval_from_hz(1), None);
        // 60Hz ≈ 16.67ms / 144Hz ≈ 6.94ms（µs 精度で確認）。
        assert_eq!(interval_from_hz(60).unwrap().as_micros(), 16_666);
        assert_eq!(interval_from_hz(144).unwrap().as_micros(), 6_944);
        // 異常な巨大値も弾く（間隔 0ns で gate が無効化する類・現実的な天井 1000Hz）。
        assert_eq!(interval_from_hz(1_001), None);
        assert_eq!(interval_from_hz(u32::MAX), None);
    }

    #[test]
    fn pace_takes_the_later_of_deadline_and_gate() {
        let base = Instant::now();
        let later = base + Duration::from_millis(10);
        // gate が未来なら gate まで遅らせる（上限が効く）。
        assert_eq!(pace(base, later), later);
        // deadline が gate より遅ければ deadline のまま（遅い予約を早めない）。
        assert_eq!(pace(later, base), later);
        assert_eq!(pace(base, base), base);
    }

    #[test]
    fn empty_repaint_causes_render_as_dash() {
        // 「原因が無い（入力イベント起因のフレーム）」を空文字にしない——ログの読み手が
        // 「トレースが壊れた」と区別できなくなるため。
        assert_eq!(format_repaint_causes(&[]), "-");
    }

    #[test]
    fn repaint_causes_join_with_file_and_line() {
        // 1 パスで複数の原因が積まれることがある（点滅 + 状態変化等）。先頭だけ出すと
        // 源を取り違えるため全件を `; ` で連結する。各要素は `file:line reason`。
        let causes = [
            egui::RepaintCause {
                file: "text_selection/visuals.rs",
                line: 313,
                reason: "".into(),
            },
            egui::RepaintCause {
                file: "view.rs",
                line: 439,
                reason: "state changed".into(),
            },
        ];
        let rendered = format_repaint_causes(&causes);
        assert_eq!(
            rendered,
            "text_selection/visuals.rs:313 ; view.rs:439 state changed"
        );
        assert_eq!(rendered.split("; ").count(), 2, "件数ぶん連結される");
    }

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
