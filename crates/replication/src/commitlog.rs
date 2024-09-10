use tokio::sync::mpsc;

use crate::{node::DBMessage, CommitLogInstruction};

// Actual commitlog, use Mmap
pub struct CommitLog {
    inner: Vec<u32>,
}

impl CommitLog {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }
}

pub struct ReplicableCommitLog {
    pub leader_pid: u32,
    commitlog: CommitLog,
    tx: mpsc::Sender<DBMessage>,
}

impl ReplicableCommitLog {
    pub fn new(leader_pid: u32, db_tx: mpsc::Sender<DBMessage>) -> mpsc::Sender<CommitLogInstruction> {
        let (log_tx, log_rx) = mpsc::channel::<CommitLogInstruction>(1);
        let mut me = ReplicableCommitLog {
            leader_pid,
            commitlog: CommitLog::new(),
            tx: db_tx,
        };

        tokio::spawn(async move {
            me.spawn(log_rx).await;
        });

        log_tx
    }

    async fn spawn(&mut self, mut rx: mpsc::Receiver<CommitLogInstruction>) {
        loop {
            tokio::select! {
                // Fetch logs asynchronously and append them to commitlog
                mut logs = self.fetch_logs() => {
                    self.commitlog.inner.append(&mut logs);
                    self.tx.send(DBMessage::Add(logs)).await.unwrap();
                },

                // Receive messages from the channel asynchronously
                Some(msg) = rx.recv() => {
                    match msg {
                        CommitLogInstruction::FromReplication(metadata) => {
                            // Check for divergence in log entries
                            let pos = find_divergence(&self.commitlog.inner, metadata.idx, &metadata.entries);

                            if let Some(idx) = pos {
                                // Truncate logs at the point of divergence
                                self.commitlog.inner.truncate(idx);

                                // Fetch new logs after truncation
                                let mut logs = self.fetch_logs().await;
                                self.commitlog.inner.append(&mut logs);

                                // Check if there's still divergence after appending
                                let pos_after_append = find_divergence(&self.commitlog.inner, metadata.idx, &metadata.entries);

                                if pos_after_append.is_some() {
                                    // If still diverged, signal failure and send abort message
                                    let _ = metadata.callback.send(false);
                                    self.tx.send(DBMessage::Abort).await.unwrap();
                                } else {
                                    // Signal success if logs are now consistent
                                    let _ = metadata.callback.send(true);
                                }
                            } else {
                                // If no divergence, signal success
                                let _ = metadata.callback.send(true);
                            }
                        },

                        CommitLogInstruction::FromDB(entry) => {
                            // Append the new entry from the DB directly
                            self.commitlog.inner.push(entry);
                        }
                    }
                },

                // Exit the loop if the receiver is closed
                else => break,
            }
        }
    }

    // Helper function to find divergence in log entries
    async fn fetch_logs(&self) -> Vec<u32> {
        [1, 2, 3].to_vec()
    }
}

fn find_divergence(log: &[u32], start_idx: usize, entries: &[u32]) -> Option<usize> {
    log[start_idx..].iter().zip(entries.iter()).position(|(a, b)| a != b)
}
