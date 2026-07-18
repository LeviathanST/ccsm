use std::path::PathBuf;

pub struct SwarmDb {
    conn: rusqlite::Connection,
}

#[derive(Debug, serde::Serialize)]
pub struct WorkerRow {
    pub run_id: String,
    pub orch_name: String,
    pub worker_name: String,
    pub worker_sid: Option<String>,
    pub status: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost: f64,
    pub created: String,
}

impl SwarmDb {
    pub fn open() -> anyhow::Result<Self> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let db_path = PathBuf::from(home).join(".ccsm").join("swarm.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = rusqlite::Connection::open(&db_path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workers (
                run_id       TEXT NOT NULL,
                orch_name    TEXT NOT NULL,
                worker_name  TEXT NOT NULL,
                worker_sid   TEXT,
                status       TEXT NOT NULL DEFAULT 'pending',
                tokens_in    INTEGER DEFAULT 0,
                tokens_out   INTEGER DEFAULT 0,
                cost         REAL DEFAULT 0,
                created      TEXT NOT NULL,
                PRIMARY KEY (run_id, worker_name)
            );
            CREATE INDEX IF NOT EXISTS idx_worker_sid ON workers(worker_sid);
            CREATE INDEX IF NOT EXISTS idx_orch_name ON workers(orch_name);"
        )?;
        Ok(())
    }

    pub fn get_workers(
        &self,
        orch_name: Option<&str>,
        worker_name: Option<&str>,
    ) -> anyhow::Result<Vec<WorkerRow>> {
        let mut sql = String::from(
            "SELECT run_id, orch_name, worker_name, worker_sid, status, tokens_in, tokens_out, cost, created FROM workers WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(name) = orch_name {
            params.push(Box::new(name.to_string()));
            sql.push_str(&format!(" AND orch_name = ?{}", params.len()));
        }
        if let Some(name) = worker_name {
            params.push(Box::new(name.to_string()));
            sql.push_str(&format!(" AND worker_name = ?{}", params.len()));
        }

        sql.push_str(" ORDER BY created ASC");

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(WorkerRow {
                run_id: row.get(0)?,
                orch_name: row.get(1)?,
                worker_name: row.get(2)?,
                worker_sid: row.get(3)?,
                status: row.get(4)?,
                tokens_in: row.get(5)?,
                tokens_out: row.get(6)?,
                cost: row.get(7)?,
                created: row.get(8)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn update_status(
        &self,
        run_id: &str,
        worker_name: &str,
        status: &str,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE workers SET status = ?1 WHERE run_id = ?2 AND worker_name = ?3",
            rusqlite::params![status, run_id, worker_name],
        )?;
        Ok(())
    }

    pub fn insert_worker(
        &self,
        run_id: &str,
        orch_name: &str,
        worker_name: &str,
        worker_sid: Option<&str>,
        created: &str,
    ) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO workers (run_id, orch_name, worker_name, worker_sid, status, created)
             VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
            rusqlite::params![run_id, orch_name, worker_name, worker_sid, created],
        )?;
        Ok(())
    }
}
