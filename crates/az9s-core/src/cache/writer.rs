//! Background cache writer.
//!
//! Replacing a large scope takes a while: about 0.7 s for 20k resources and
//! 4 s for 100k (measured with `cache_scope_perf`). That must never block the
//! UI thread, so file-backed caches send writes to a dedicated thread that
//! owns its own SQLite connection. The database runs in WAL mode, so the UI
//! connection keeps reading while a write is in progress.
//!
//! In-memory caches (tests) can't share a database between connections, so
//! they write synchronously on the caller's connection instead.

use super::repository::CacheRepository;
use crate::models::{Resource, ResourceGroup, Subscription};
use crate::timing;
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{debug, warn};

/// A cache write: replace one scope with fresh rows.
#[derive(Debug)]
pub enum WriteJob {
    Subscriptions(Vec<Subscription>),
    ResourceGroups {
        subscription_ids: Vec<String>,
        items: Vec<ResourceGroup>,
    },
    Resources {
        subscription_ids: Vec<String>,
        resource_group: Option<String>,
        items: Vec<Resource>,
    },
    /// Cache one resource's full document.
    ResourceDetail {
        resource_id: String,
        document: serde_json::Value,
    },
    /// Store a whole document under a key.
    Document {
        key: String,
        value: String,
    },
}

impl WriteJob {
    /// Apply the job through `repo`.
    pub fn apply(self, repo: &CacheRepository<'_>) -> crate::Result<()> {
        match self {
            WriteJob::Subscriptions(items) => {
                let _t = timing::timer("cache.write_subscriptions");
                repo.replace_subscriptions(&items)
            }
            WriteJob::ResourceGroups {
                subscription_ids,
                items,
            } => {
                let _t = timing::timer("cache.write_resource_groups");
                repo.replace_resource_groups(&subscription_ids, &items)
            }
            WriteJob::Resources {
                subscription_ids,
                resource_group,
                items,
            } => {
                let _t = timing::timer("cache.write_resources");
                repo.replace_resources(&subscription_ids, resource_group.as_deref(), &items)
            }
            WriteJob::ResourceDetail {
                resource_id,
                document,
            } => {
                let _t = timing::timer("cache.write_resource_detail");
                repo.put_resource_detail(&resource_id, &document)
            }
            WriteJob::Document { key, value } => {
                let _t = timing::timer("cache.write_document");
                repo.put_document(&key, &value)
            }
        }
    }
}

enum Message {
    Write(WriteJob),
    Flush(mpsc::Sender<()>),
}

/// Handle to the writer thread. Dropping it finishes queued writes first.
pub struct Writer {
    tx: Option<mpsc::Sender<Message>>,
    handle: Option<JoinHandle<()>>,
}

impl Writer {
    /// Start a writer thread with its own connection to the database at `path`.
    pub fn spawn(path: &Path) -> crate::Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        let (tx, rx) = mpsc::channel::<Message>();
        let handle = std::thread::Builder::new()
            .name("az9s-cache-writer".into())
            .spawn(move || {
                let repo = CacheRepository::new(&conn);
                for message in rx {
                    match message {
                        Message::Write(job) => {
                            if let Err(e) = job.apply(&repo) {
                                warn!("Background cache write failed: {}", e);
                            }
                        }
                        Message::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
                debug!("Cache writer stopped");
            })
            .expect("failed to spawn cache writer thread");
        Ok(Self {
            tx: Some(tx),
            handle: Some(handle),
        })
    }

    /// Queue a write. Returns immediately.
    pub fn send(&self, job: WriteJob) {
        if let Some(tx) = &self.tx {
            if tx.send(Message::Write(job)).is_err() {
                warn!("Cache writer is gone; write dropped");
            }
        }
    }

    /// Block until every write queued so far is on disk.
    pub fn flush(&self) {
        if let Some(tx) = &self.tx {
            let (done_tx, done_rx) = mpsc::channel();
            if tx.send(Message::Flush(done_tx)).is_ok() {
                let _ = done_rx.recv();
            }
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Closing the channel ends the thread after it drains queued writes.
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
