use crate::workflow::WorkflowErrorType;
use crate::workflow_signal::{Signal, SignalDirection};
use crate::workflow_state::{Open, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

// DB trait that defines the required operations for a workflow database
#[async_trait]
pub trait WorkflowDbTrait: Send + Sync {
    async fn insert_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType>;
    async fn update_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType>;
    async fn get_all_signals(&self) -> Result<Vec<Signal>, WorkflowErrorType>;

    async fn get_signals_for_workflows(
        &self,
        workflow_ids: Vec<String>,
    ) -> Result<Vec<Signal>, WorkflowErrorType>;

    async fn cancel_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<(), WorkflowErrorType>;
    async fn get_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
        signal_name: &str,
        direction: SignalDirection,
        accept_processed: bool,
    ) -> Result<Option<Vec<Signal>>, WorkflowErrorType>;
    async fn insert(&self, state: WorkflowState) -> String;
    async fn update(&self, state: WorkflowState);
    async fn query_due(&self, now: SystemTime) -> Vec<WorkflowState>;
    async fn get_all(&self) -> Vec<WorkflowState>;
    async fn get_by_cursor(
        &self,
        cursor: Option<String>,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType>;
    async fn get_workflow_state(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowState>, WorkflowErrorType>;

    async fn get_running_workflows(
        &self,
        workflow_type: &str,
        instance_id: &str,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType>;
}

// pub fn unique_workflow_id(workflow_name: &str, instance_id: &str) -> String {
//     format!("{}-{}", workflow_name, instance_id)
// }

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
impl WorkflowDbTrait for InMemoryDB {
    async fn get_all(&self) -> Vec<WorkflowState> {
        let db = self.workflows.lock().unwrap();
        db.values().cloned().collect()
    }
    async fn get_signals_for_workflows(
        &self,
        workflow_ids: Vec<String>,
    ) -> Result<Vec<Signal>, WorkflowErrorType> {
        let db = self.signals.lock().unwrap();

        // Filter signals based on workflow IDs, signal name, and direction
        let signals = db
            .values()
            .filter(|signal| workflow_ids.contains(&signal.instance_id))
            .cloned()
            .collect::<Vec<_>>();

        Ok(signals)
    }

    async fn insert(&self, state: WorkflowState) -> String {
        let mut db = self.workflows.lock().unwrap();
        db.insert(state.run_id.clone(), state.clone());
        state.run_id.clone()
    }
    async fn get_by_cursor(
        &self,
        cursor: Option<String>,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType> {
        let db = self.workflows.lock().unwrap();

        // Collect workflows and sort them by `updated_at`
        let mut workflows: Vec<WorkflowState> = db.values().cloned().collect();

        Ok(workflows)
    }

    async fn get_workflow_state(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowState>, WorkflowErrorType> {
        let workflows = self.workflows.lock().unwrap();
        Ok(workflows.get(run_id).cloned())
    }
    async fn get_running_workflows(
        &self,
        workflow_type: &str,
        instance_id: &str,
    ) -> Result<Vec<WorkflowState>, WorkflowErrorType> {
        let workflows = self.workflows.lock().unwrap();
        let workflows_vec = workflows
            .values()
            .filter(|w| {
                w.instance_id == instance_id
                    && w.workflow_type == workflow_type
                    && matches!(w.status, WorkflowStatus::Open(_)) // Match any Closed variant
            })
            .cloned() // Clone to avoid borrow issues
            .collect::<Vec<WorkflowState>>();

        Ok(workflows_vec)
    }

    async fn update(&self, state: WorkflowState) {
        let mut db = self.workflows.lock().unwrap();
        db.insert(state.run_id.clone(), state.clone());
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
    async fn update_signal(&self, signal: Signal) -> Result<(), WorkflowErrorType> {
        let mut db = self.signals.lock().unwrap();
        db.insert(signal.id.clone(), signal.clone());
        Ok(())
    }

    async fn get_all_signals(&self) -> Result<Vec<Signal>, WorkflowErrorType> {
        let db = self.signals.lock().unwrap();
        let res = db.values().cloned().collect();
        Ok(res)
    }

    async fn get_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
        signal_name: &str,
        direction: SignalDirection,
        accept_processed: bool,
    ) -> Result<Option<Vec<Signal>>, WorkflowErrorType> {
        let db = self.signals.lock().unwrap();
        let res = db
            .values()
            .filter(|s| {
                s.workflow_name == workflow_name
                    && s.instance_id == instance_id
                    && s.signal_name == signal_name
                    && s.direction == direction
                    && (accept_processed || !s.processed)
            })
            .cloned()
            .collect();

        Ok(Some(res))
    }
    async fn cancel_signals(
        &self,
        workflow_name: &str,
        instance_id: &str,
    ) -> Result<(), WorkflowErrorType> {
        let mut db = self.signals.lock().unwrap();
        // Filter and update signals in place
        for signal in db.values_mut() {
            if signal.workflow_name == workflow_name && signal.instance_id == instance_id {
                signal.processed = true;
                signal.processed_at = Some(std::time::SystemTime::now());
            }
        }

        Ok(())
    }
}
