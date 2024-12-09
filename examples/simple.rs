use ask_workflow::start_axum_server;
use ask_workflow::worker::Worker;
use ask_workflow::workflow_mongodb::MongoDB;
use mongodb::Client;
use reqwest;
use simple::create_user_workflow::{
    CreateUserInput, CreateUserWorkflow, CreateUserWorkflowContext, NonVerifiedUserOut,
    VerificationCodeSignal,
};
use simple::mock_db::MockDatabase;
use std::sync::Arc;
use std::time::Duration;

mod simple {
    pub mod basic_workflow;
    pub mod create_user_workflow;
    pub mod mock_db;
    pub mod poll_fail_workflow;
    pub mod enqueued_workflow;
}

use simple::basic_workflow::{BasicWorkflow, BasicWorkflowContext};

#[tokio::main]
async fn main() {
    let uri =
        std::env::var("asdadMONGODB_URI").unwrap_or_else(|_| "mongodb://localhost:27017/".into());
    let client = Client::with_uri_str(&uri).await.unwrap();
    let database = client.database("ask_workflow");

    let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> =
        Arc::new(MongoDB::new(database.clone()));

    let mut worker = Worker::new(db.clone());


    worker.add_workflow::<BasicWorkflow>(BasicWorkflow {});

    let mock_db = Arc::new(MockDatabase::new());
    let mock_db_clone = mock_db.clone();
    let create_user_context = Arc::new(CreateUserWorkflowContext {
        http_client: Arc::new(reqwest::Client::new()),
        db: mock_db_clone.clone(),
    });

    worker.add_workflow::<CreateUserWorkflow>(CreateUserWorkflow {
        context: create_user_context.clone(),
    });

    println!("adding workflow");

    let _ = worker
        .schedule_now::<BasicWorkflow, ()>(&cuid::cuid1().unwrap(), ())
        .await
        .unwrap();

    let worker = Arc::new(worker);
    let worker_clone = worker.clone();
    let worker_clone_2 = worker.clone();

    // Start the worker in its own background thread
    let worker_handle = tokio::spawn(async move {
        worker_clone.clone().run(100).await;
    });

    let axum_handle = tokio::spawn(async move {
        start_axum_server(worker_clone_2.clone(), 3008).await;
    });

    println!("Worker running in the background. Press Ctrl+C to exit.");

    let user_input = CreateUserInput {
        name: "Aksel".to_string(),
    };

    println!("scheduling craete user");
    let run_id = worker
        .schedule_now::<CreateUserWorkflow, CreateUserInput>("Aksel", user_input)
        .await
        .unwrap();

    let await_signal =
        worker.await_signal::<NonVerifiedUserOut>("Aksel", None, Duration::from_secs(10));
    let unverified_user: NonVerifiedUserOut = await_signal.await.unwrap();
    println!("Unverified user: {:?}", unverified_user);

    worker
        .send_signal(
            "Aksel",
            VerificationCodeSignal {
                code: "123".to_string(),
            },
            None,
        )
        .await
        .unwrap();

    worker_handle.await.unwrap();
}
