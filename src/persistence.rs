use crate::sim_core::Simulation;
use crate::world_model::{AgentMemory, SimulationSnapshot, WorldEvent};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct Persistence {
    conn: Connection,
}

impl Persistence {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).context("failed to open SQLite database")?;
        let persistence = Self { conn };
        persistence.init_schema()?;
        Ok(persistence)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS checkpoints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                day INTEGER NOT NULL,
                tick_of_day INTEGER NOT NULL,
                total_ticks INTEGER NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                checkpoint_id INTEGER NOT NULL,
                day INTEGER NOT NULL,
                tick INTEGER NOT NULL,
                actor INTEGER NOT NULL,
                target INTEGER,
                kind TEXT NOT NULL,
                summary TEXT NOT NULL,
                tags_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS memories (
                checkpoint_id INTEGER NOT NULL,
                agent_id INTEGER NOT NULL,
                memory_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                day INTEGER NOT NULL,
                tick INTEGER NOT NULL,
                summary TEXT NOT NULL,
                weight INTEGER NOT NULL,
                tags_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS relations (
                checkpoint_id INTEGER NOT NULL,
                agent_id INTEGER NOT NULL,
                other_id INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn save(&mut self, sim: &mut Simulation, kind: &str) -> Result<()> {
        let snapshot = sim.snapshot();
        let payload = serde_json::to_string_pretty(&snapshot)?;
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO checkpoints (kind, day, tick_of_day, total_ticks, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                kind,
                snapshot.day,
                snapshot.tick_of_day,
                snapshot.total_ticks,
                payload,
                now
            ],
        )?;
        let checkpoint_id = tx.last_insert_rowid();
        for event in &snapshot.events {
            insert_event(&tx, checkpoint_id, event)?;
        }
        for agent in &snapshot.agents {
            for memory in &agent.memories {
                insert_memory(&tx, checkpoint_id, agent.id, memory)?;
            }
            for (other_id, relation) in &agent.relations {
                tx.execute(
                    "INSERT INTO relations (checkpoint_id, agent_id, other_id, payload)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        checkpoint_id,
                        agent.id,
                        other_id,
                        serde_json::to_string(relation)?
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_latest(&self) -> Result<Option<SimulationSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM checkpoints ORDER BY total_ticks DESC, id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let payload: String = row.get(0)?;
            let value: serde_json::Value =
                serde_json::from_str(&payload).context("failed to parse persisted payload")?;
            let schema_version = value
                .get("schema_version")
                .and_then(|entry| entry.as_u64())
                .ok_or_else(|| {
                    anyhow::anyhow!("legacy snapshot without spatial grid is not supported")
                })?;
            if schema_version != 22 {
                return Err(anyhow::anyhow!(
                    "unsupported snapshot schema_version={schema_version}; expected 22"
                ));
            }
            let snapshot = serde_json::from_value(value)
                .context("failed to parse persisted spatial snapshot")?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }
}

fn insert_event(
    tx: &rusqlite::Transaction<'_>,
    checkpoint_id: i64,
    event: &WorldEvent,
) -> Result<()> {
    tx.execute(
        "INSERT INTO events (checkpoint_id, day, tick, actor, target, kind, summary, tags_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            checkpoint_id,
            event.day,
            event.tick,
            event.actor,
            event.target,
            format!("{:?}", event.kind),
            event.summary,
            serde_json::to_string(&event.impact_tags)?,
        ],
    )?;
    Ok(())
}

fn insert_memory(
    tx: &rusqlite::Transaction<'_>,
    checkpoint_id: i64,
    agent_id: u64,
    memory: &AgentMemory,
) -> Result<()> {
    tx.execute(
        "INSERT INTO memories (checkpoint_id, agent_id, memory_id, kind, day, tick, summary, weight, tags_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            checkpoint_id,
            agent_id,
            memory.id,
            format!("{:?}", memory.kind),
            memory.day,
            memory.tick,
            memory.summary,
            memory.emotional_weight,
            serde_json::to_string(&memory.tags)?,
        ],
    )?;
    Ok(())
}
