use crate::commitlog::CommitLog;

const MOD: u32 = 1000_000_007;

struct DB {
    pub state: u32,
}

// a Db which does not support undo
impl DB {
    pub fn new() -> Self {
        Self { state: 0 }
    }

    pub fn commit(&mut self, tx: u32) {
        self.state = (self.state + tx) % MOD;
    }
}

pub enum DBMessage {
    Add(Vec<u32>),
    Abort,
}

struct Node {
    db: DB,
    commit_log: CommitLog,
    omnipaxos: OmniPaxos<Entry, ConsistentLogStore>,
    pid: u32,
    leader: Option<u32>,
}

impl Node {
    pub fn commit(&mut self) {}
    pub fn replay_db(&mut self) {
        self.commit_log.inner.iter().for_each(|i| {
            self.db.commit(*i);
        });
    }

    pub fn new(pid: u8) -> Self {
        let omnipaxos_config = OmniPaxosConfig {
            cluster_config: ClusterConfig {
                configuration_id: 1,
                nodes: [1, 2, 3].to_vec(),
                ..Default::default()
            },
            server_config: omnipaxos::ServerConfig {
                pid: pid.into(),
                election_tick_timeout: 10,
                resend_message_tick_timeout: 0,
                ..Default::default()
            },
        };

        let persistent_storage = PersistentStorageConfig::default();
        let mut commit_log = CommitLog::new();
        let db = DB::new();

        let (tx, rx) = oneshot::channel::<LogSyncMessage>();

        tokio::task::spawn_blocking(|| commit_log.fetch_loop());
        let storage = ConsistentLogStore::new(persistent_storage, tx);
        let omnipaxos = omnipaxos_config.build(storage).unwrap();
        Self {
            db,
            commit_log,
            omnipaxos,
            pid: 1,
            leader: None,
        }
    }
}
