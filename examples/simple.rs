use ask_workflow::db_trait::InMemoryDB;
use ask_workflow::start_axum_server;
use ask_workflow::worker::Worker;
use ask_workflow::workflow::{Workflow, WorkflowErrorType};
use ask_workflow::workflow_mongodb::MongoDB;
use mongodb::Client;
use reqwest;
use simple::create_user_workflow::{CreateUserWorkflow, CreateUserWorkflowContext};
use simple::mock_db::MockDatabase;
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
    let uri = std::env::var("asdadMONGODB_URI").unwrap_or_else(|_| "mongodb://localhost:27017/".into());
    let client = Client::with_uri_str(&uri).await.unwrap();
    let database = client.database("ask_workflow");

    let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> =
        Arc::new(MongoDB::new(database.clone()));

    let mut worker = Worker::new(db.clone());
    worker.add_workflow::<BasicWorkflow, _>(|state| {
        let context = Arc::new(BasicWorkflowContext {
            http_client: reqwest::Client::new(),
        });

        return Box::new(BasicWorkflow {
            state,
            context: context.clone(),
        });
    });
    let mock_db = Arc::new(MockDatabase::new());
    let mock_db_clone = mock_db.clone();
    let create_user_context = Arc::new(CreateUserWorkflowContext {
        http_client: Arc::new(reqwest::Client::new()),
        db: mock_db_clone.clone(),
    });

    worker.add_workflow::<CreateUserWorkflow, _>(move |state| {
        return Box::new(CreateUserWorkflow {
            state,
            context: create_user_context.clone(),
        });
    });

    println!("adding workflow");

    let _ = worker
        .schedule_now::<BasicWorkflow, ()>(
            &cuid::cuid1().unwrap(),
            None,
        )
        .await.unwrap();

    let worker = Arc::new(worker);
    let worker_clone = worker.clone();

    // Start the worker in its own background thread
    let worker_handle = tokio::spawn(async move {
        worker_clone.run(100).await;
    });

    // Keep the main thread alive (or start other tasks like a web server here)
    println!("Worker running in the background. Press Ctrl+C to exit.");

    start_axum_server(worker.clone(), 3008).await;

    // Optionally, join the worker thread when exiting
    worker_handle.await.unwrap();
}
