use std::sync::Arc;
mod macros;

// Re-export the macros to make them available to users of the crate
pub use macros::*;

use axum::{
    Extension, Router,
};
use db_trait::{InMemoryDB, WorkflowDbTrait};
use tokio::task::JoinHandle;
use worker::Worker;

pub mod activity;
pub mod db_trait;
pub mod test_utils;
pub mod ui;
pub mod worker;
pub mod workflow;
pub mod workflow_mongodb;
pub mod workflow_signal;
pub mod workflow_state;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
// Axum server setup
#[derive(Clone)]
struct AppState {
    worker: Arc<worker::Worker>,
}
pub async fn start_axum_server(worker: Arc<worker::Worker>, port: u16) {
    let app_state = AppState { worker };
    let app = Router::new()
        .nest("/workflows", ui::controllers::workflow::routes())
        .layer(Extension(app_state));
    let addr: String = format!("0.0.0.0:{}", port).parse().unwrap();

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
/// Initializes and returns a new worker instance for testing, allowing configuration before starting.
pub fn init_test_worker_with_workflow() -> Worker {
    // Initialize the in-memory mock database
    let db: Arc<dyn WorkflowDbTrait> = Arc::new(InMemoryDB::new());
    // Create the worker
    Worker::new(db.clone())
}
pub fn start_test_worker(worker: Worker) -> (Arc<Worker>, JoinHandle<()>) {
    let worker = Arc::new(worker);
    let worker_clone = worker.clone();
    let worker_handle = tokio::spawn(async move {
        worker_clone.run(500).await;
    });

    (worker, worker_handle)
}
