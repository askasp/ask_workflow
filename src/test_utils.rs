// src/test_utils.rs

use crate::db_trait::InMemoryDB; // Assuming you have a MockDatabase for testing
use crate::db_trait::WorkflowDbTrait;
use crate::worker::Worker;
use crate::workflow::Workflow;
use crate::workflow_state::WorkflowState;
use std::sync::Arc;
use tokio::task::{self, JoinHandle};
use tokio::time::Duration;

/// Sets up an in-memory mock database for testing.
pub fn setup_mock_db() -> Arc<InMemoryDB> {
    Arc::new(InMemoryDB::new())
}

/// Initializes a WorkflowState with optional input data.
pub fn setup_workflow_state(
    workflow_type: &str,
    instance_id: &str,
    input: Option<serde_json::Value>,
) -> WorkflowState {
    WorkflowState::new(
        workflow_type,
        instance_id,
        std::time::SystemTime::now(),
        input,
    )
}

pub async fn initialize_and_start_test_worker<W, F>(
    workflow_factory: F,
) -> (Arc<Worker>, JoinHandle<()>)
where
    W: Workflow + 'static,
    F: Fn() -> Box<dyn Workflow + Send + Sync> + Send + Sync + 'static,
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
