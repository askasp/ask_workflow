use crate::workflow_state::{Open, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

// DB trait that defines the required operations for a workflow database
#[async_trait]
pub trait DB: Send + Sync {
    async fn insert(&self, state: WorkflowState);
    async fn update(&self, state: WorkflowState);
    async fn query_due(&self, now: SystemTime) -> Vec<WorkflowState>;
    async fn get_all(&self) -> Vec<WorkflowState>;
}
// In-memory database implementation using a HashMap
pub struct InMemoryDB {
    workflows: Mutex<HashMap<String, WorkflowState>>, // Protected by a Mutex for thread safety
}

impl InMemoryDB {
    pub fn new() -> Self {
        Self {
            workflows: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl DB for InMemoryDB {
    async fn get_all(&self) -> Vec<WorkflowState> {
        let db = self.workflows.lock().unwrap();
        db.values().cloned().collect()
    }

    async fn insert(&self, state: WorkflowState) {
        let mut db = self.workflows.lock().unwrap();
        db.insert(state.instance_id.clone(), state);
    }

    async fn update(&self, state: WorkflowState) {
        let mut db = self.workflows.lock().unwrap();
        db.insert(state.instance_id.clone(), state);
    }

    async fn query_due(&self, now: SystemTime) -> Vec<WorkflowState> {
        let db = self.workflows.lock().unwrap();
        db.values()
            .filter(|workflow| {
                workflow.scheduled_at <= now
                    && match workflow.status {
                        WorkflowStatus::Open(Open::Running) => true,
                        _ => false,
                    }
            })
            .cloned()
            .collect()
    }
}
