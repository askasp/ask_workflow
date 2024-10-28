use ask_workflow::db_trait::InMemoryDB;
use ask_workflow::start_axum_server;
use ask_workflow::worker::Worker;
use ask_workflow::workflow::{run_activity_async, Workflow, WorkflowErrorType};
use reqwest;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::time::Duration;

mod simple {
    pub mod basic_workflow;
    pub mod create_user_workflow;
    pub mod mock_db;
}

use simple::basic_workflow::{BasicWorkflow, BasicWorkflowContext};

#[tokio::main]
async fn main() {
    let db: Arc<dyn ask_workflow::db_trait::DB> = Arc::new(InMemoryDB::new());
    let mut worker = Worker::new(db.clone());
    worker.add_workflow(BasicWorkflow::static_name(), |state| {
        let context = Arc::new(BasicWorkflowContext {
            http_client: reqwest::Client::new(),
        });

        return Box::new(BasicWorkflow {
            state,
            context: context.clone(),
        });
    });

    let _ = worker
        .schedule(
            BasicWorkflow::static_name(),
            "workflow-instance_1",
            SystemTime::now() + Duration::from_secs(20), // Schedule for 20 seconds in the future
        )
        .await;

    let _ = worker
        .schedule_now(BasicWorkflow::static_name(), "workflow_instance_2", None)
        .await;

    let _ = worker
        .schedule(
            BasicWorkflow::static_name(),
            "failing_id",
            SystemTime::now() + Duration::from_secs(3),
        )
        .await;

    let worker = Arc::new(worker);

    let worker_clone = worker.clone();

    // Start the worker in its own background thread
    let worker_handle = tokio::spawn(async move {
        worker_clone.run(1).await;
    });

    // Keep the main thread alive (or start other tasks like a web server here)
    println!("Worker running in the background. Press Ctrl+C to exit.");

    start_axum_server(worker.clone(), 3008).await;

    // Optionally, join the worker thread when exiting
    worker_handle.await.unwrap();
}
