use crate::workflow::WorkflowErrorType;
use crate::workflow_signal::Signal;
use crate::workflow_state::{self, Open, WorkflowError, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

// DB trait that defines the required operations for a workflow database
#[async_trait]
pub trait DB: Send + Sync {
    async fn insert_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType>;
    async fn get_signal(&self, signal_id: &String) -> Result<Option<Signal>, WorkflowErrorType>;
    async fn insert(&self, state: WorkflowState);
    async fn update(&self, state: WorkflowState);
    async fn query_due(&self, now: SystemTime) -> Vec<WorkflowState>;
    async fn get_all(&self) -> Vec<WorkflowState>;
    async fn get_workflow_state(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<Option<WorkflowState>, &'static str>;
}

pub fn unique_workflow_id(workflow_name: &str, instance_id: &str) -> String {
    format!("{}-{}", workflow_name, instance_id)
}

// In-memory database implementation using a HashMap
pub struct InMemoryDB {
    workflows: Mutex<HashMap<String, WorkflowState>>, // Protected by a Mutex for thread safety
    signals: Mutex<HashMap<String, Signal>>,          // Protected by a Mutex for thread safety
}

impl InMemoryDB {
    pub fn new() -> Self {
        Self {
            workflows: Mutex::new(HashMap::new()),
            signals: Mutex::new(HashMap::new()),
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
        db.insert(state.unique_id(), state);
    }

    async fn get_workflow_state(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<Option<WorkflowState>, &'static str> {
        // let mut db = self.workflows.lock().unwrap();

        println!("getting workflow {}_{}", workflow_name, instance_id);
        let workflows = self.workflows.lock().unwrap();
        Ok(workflows
            .get(&unique_workflow_id(workflow_name, instance_id))
            .cloned())
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
    async fn insert_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType> {
        let mut db = self.signals.lock().unwrap();
        db.insert(signal.id.clone(), signal);
        Ok(())
    }

    async fn get_signal(&self, signal_id: &String) -> Result<Option<Signal>, WorkflowErrorType> {
        let db = self.signals.lock().unwrap();
        Ok(db.get(signal_id).cloned())
    }
}
