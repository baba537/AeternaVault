//! Background work for the GUI.
//!
//! Plain `std::thread` + `mpsc` channels: the engine is synchronous and
//! blocking I/O dominates, so an async runtime would add weight without benefit.
//! Every message wakes the UI via `request_repaint`.

use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;

use crate::engine::backup::BackupReport;
use crate::engine::plan::{BackupPlan, RestorePlan};
use crate::engine::restore::RestoreReport;
use crate::engine::{CancelToken, Progress};
use crate::error::EngineResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// Scan for a backup. `confirm`: continue to the confirmation instead of the preview list.
    PlanBackup {
        confirm: bool,
    },
    PlanRestore {
        confirm: bool,
    },
    Backup,
    Restore,
}

pub enum TaskOutput {
    BackupPlan(EngineResult<BackupPlan>),
    RestorePlan(EngineResult<RestorePlan>),
    Backup(EngineResult<BackupReport>),
    Restore(EngineResult<RestoreReport>),
}

enum Message {
    Progress(Progress),
    Done(Box<TaskOutput>),
    Panicked(String),
}

/// A long-running engine operation with progress and cancellation.
pub struct Task {
    pub kind: TaskKind,
    pub cancel: CancelToken,
    pub progress: Progress,
    rx: Receiver<Message>,
}

impl Task {
    pub fn spawn(
        ctx: &egui::Context,
        kind: TaskKind,
        work: impl FnOnce(&CancelToken, &mut dyn FnMut(&Progress)) -> TaskOutput + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::default();
        let worker_cancel = cancel.clone();
        let ctx = ctx.clone();

        let spawned = std::thread::Builder::new()
            .name("aeterna-worker".into())
            .spawn(move || {
                let progress_tx = tx.clone();
                let progress_ctx = ctx.clone();
                let mut on_progress = move |p: &Progress| {
                    let _ = progress_tx.send(Message::Progress(p.clone()));
                    progress_ctx.request_repaint();
                };
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    work(&worker_cancel, &mut on_progress)
                }));
                let message = match outcome {
                    Ok(output) => Message::Done(Box::new(output)),
                    Err(payload) => Message::Panicked(panic_text(payload.as_ref())),
                };
                let _ = tx.send(message);
                ctx.request_repaint();
            });
        if let Err(err) = spawned {
            tracing::error!("could not start a worker thread: {err}");
        }

        Self {
            kind,
            cancel,
            progress: Progress::default(),
            rx,
        }
    }

    /// Applies progress updates; returns the result once the task has finished.
    pub fn poll(&mut self) -> Option<Result<TaskOutput, String>> {
        loop {
            match self.rx.try_recv() {
                Ok(Message::Progress(p)) => self.progress = p,
                Ok(Message::Done(output)) => return Some(Ok(*output)),
                Ok(Message::Panicked(text)) => return Some(Err(text)),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    return Some(Err("worker stopped unexpectedly".into()));
                }
            }
        }
    }
}

/// A small background loader (folder sizes, backup list, installed apps).
pub struct Job<T> {
    pub cancel: CancelToken,
    rx: Receiver<T>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(
        ctx: &egui::Context,
        work: impl FnOnce(&CancelToken, &dyn Fn(T)) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::default();
        let worker_cancel = cancel.clone();
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new()
            .name("aeterna-loader".into())
            .spawn(move || {
                let send = |value: T| {
                    let _ = tx.send(value);
                    ctx.request_repaint();
                };
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| work(&worker_cancel, &send)));
                ctx.request_repaint();
            });
        Self { cancel, rx }
    }

    /// Returns all values received so far and whether the job has finished.
    pub fn drain(&self) -> (Vec<T>, bool) {
        let mut values = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(v) => values.push(v),
                Err(TryRecvError::Empty) => return (values, false),
                Err(TryRecvError::Disconnected) => return (values, true),
            }
        }
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}
