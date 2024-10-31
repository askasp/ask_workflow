// src/test_utils.rs

use std::sync::Arc;
use crate::db_trait::{InMemoryDB}; // Assuming you have a MockDatabase for testing
use crate::workflow_state::WorkflowState;
use crate::worker::Worker;
use crate::workflow::Workflow;
use tokio::task;
use crate::db_trait::DB;
use tokio::time::Duration;

#[cfg(test)]
/// Sets up an in-memory mock database for testing.
pub fn setup_mock_db() -> Arc<InMemoryDB> {
    Arc::new(InMemoryDB::new())
}

#[cfg(test)]
/// Initializes a WorkflowState with optional input data.
pub fn setup_workflow_state(workflow_type: &str, instance_id: &str, input: Option<serde_json::Value>) -> WorkflowState {
    WorkflowState::new(workflow_type, instance_id, std::time::SystemTime::now(), input)
}

/// Starts a worker and runs it for a specified duration (useful for tests).

#[cfg(test)]
/// Helper function to check if a workflow completed successfully.
pub fn assert_workflow_completed(workflow: &dyn Workflow) {
    assert_eq!(workflow.state().status.to_string(), "Completed");
}
