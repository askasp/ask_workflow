// src/test_utils.rs

use std::sync::Arc;
use crate::db_trait::{InMemoryDB}; // Assuming you have a MockDatabase for testing
use crate::workflow_state::WorkflowState;
use crate::worker::Worker;
use crate::workflow::Workflow;
use tokio::task::{self, JoinHandle};
use crate::db_trait::WorkflowDbTrait;
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

///
/// - `workflow_factory`: The workflow factory function to add to the worker.
pub async fn initialize_and_start_test_worker<W, F>(
    workflow_factory: F,
) -> (Arc<Worker>, JoinHandle<()>)
where
    W: Workflow + 'static,
    F: Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync + 'static,
{
    // Initialize the in-memory mock database
    let db: Arc<dyn WorkflowDbTrait> = Arc::new(InMemoryDB::new());

    // Create the worker
    let mut worker = Worker::new(db.clone());

    // Add the specified workflow to the worker
    worker.add_workflow::<W, _>(workflow_factory);

    // Wrap the worker in an Arc and start it in a background task
    let worker = Arc::new(worker);
    let worker_clone = worker.clone();

    let worker_handle = tokio::spawn(async move {
        worker_clone.run(500).await;
    });

    (worker, worker_handle)
}
