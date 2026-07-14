use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, RecvTimeoutError, Sender},
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

#[derive(Clone)]
pub(crate) struct RepaintScheduler {
    inner: Arc<SchedulerInner>,
}

struct SchedulerInner {
    sender: Sender<SchedulerMessage>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl RepaintScheduler {
    pub(crate) fn new<T: UserEvent>(
        proxy: EventLoopProxy<Message<T>>,
        window_id: WindowId,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();

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
                sender,
                worker: Mutex::new(Some(worker)),
            }),
        }
    }

    pub(crate) fn request(&self, delay: Duration) {
        let _ = self.inner.sender.send(SchedulerMessage::Request {
            deadline: Instant::now() + delay,
        });
    }
}

impl Drop for SchedulerInner {
    fn drop(&mut self) {
        let _ = self.sender.send(SchedulerMessage::Stop);
        if let Some(worker) = self.worker.lock().expect("repaint worker lock").take() {
            let _ = worker.join();
        }
    }
}
