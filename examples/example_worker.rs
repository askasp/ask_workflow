use ask_workflow::activity::Activity;
use ask_workflow::db_trait::InMemoryDB;
use ask_workflow::start_axum_server;
use ask_workflow::worker::Worker;
use ask_workflow::workflow::{run_activity, run_activity_async, Workflow, WorkflowErrorType};
use ask_workflow::workflow_state::WorkflowState;
use async_trait::async_trait;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::signal;
use tokio::time::Duration;

// Define your workflow
//
//

#[derive(Deserialize)]
pub struct HttpBinResponse {
    pub origin: String,
    pub url: String,
}

// Example of a function that performs a GET request and can be retried
pub async fn get_http_data() -> Result<HttpBinResponse, &'static str> {
    let response = reqwest::get("https://httpbin.org/get")
        .await
        .map_err(|_| "Request failed")?
        .json::<HttpBinResponse>()
        .await
        .map_err(|_| "Failed to parse response")?;

    Ok(response)
}
pub struct FetchHttpActivity {
    pub url: String,
    pub http_client: reqwest::Client,
}

#[async_trait]
impl Activity<u16> for FetchHttpActivity {
    async fn run(&self) -> Result<u16, WorkflowErrorType> {
        let res = self
            .http_client
            .get(&self.url)
            .send()
            .await
            .map_err(|err| WorkflowErrorType::TransientError {
                message: format!("HTTP request failed: {}", err),
                content: None,
            })?;

        let status = res.status().as_u16();
        Ok(status)
    }

    fn name(&self) -> &str {
        "FetchHttpActivity"
    }
}
pub struct SimpleActivity;
#[async_trait]
impl Activity<String> for SimpleActivity {
    async fn run(&self) -> Result<String, WorkflowErrorType> {
        println!("Running a simple activity!");

        Ok("Activity completed".to_string())
    }

    fn name(&self) -> &str {
        "SimpleActivity"
    }
}
pub struct FailingActivity;
#[async_trait]
impl Activity<String> for FailingActivity {
    async fn run(&self) -> Result<String, WorkflowErrorType> {
        println!("Running a failining activity!");
        Err(WorkflowErrorType::TransientError {
            message: "Failed".to_string(),
            content: None,
        })
    }
    fn name(&self) -> &str {
        "FailingActivity"
    }
}

#[derive(Clone)]
struct MyWorkflow {
    state: WorkflowState,
    context: Arc<MyContext>,
}

// #[typetag::serde]
#[async_trait::async_trait]
impl Workflow for MyWorkflow {
    fn name(&self) -> &str {
        "myWorkflow"
    }
    fn static_name() -> &'static str {
        "myWorkflow"
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
    fn state(&self) -> &WorkflowState {
        &self.state
    }

    async fn run(&mut self) -> Result<Option<Value>, WorkflowErrorType> {
        // Activity 1: print a string, retry up to 3 times
        let state = self.state_mut();

        let res = SimpleActivity {}.execute(state).await?;

        if state.instance_id == "failing_id".to_string() {
            FailingActivity {}.execute(state).await?;
        }

        let ctx = self.context.clone();
        let res2 = FetchHttpActivity {
            url: "https://httpbin.org/get".to_string(),
            http_client: ctx.http_client.clone(),
        }
        .run()
        .await?;

        println!("Activity 1 completed: {}", res);
        println!("Activity 2 completed: {}", res2);

        Ok(None)
    }
}

pub struct MyContext {
    pub http_client: reqwest::Client,
}

#[tokio::main]
async fn main() {
    let db: Arc<dyn ask_workflow::db_trait::DB> = Arc::new(InMemoryDB::new());
    let mut worker = Worker::new(db.clone());
    worker.add_workflow("myWorkflow", |state| {
        let context = Arc::new(MyContext {
            http_client: reqwest::Client::new(),
        });

        return Box::new(MyWorkflow {
            state,
            context: context.clone(),
        });
    });

    let _ = worker
        .schedule(
            MyWorkflow::static_name(),
            "workflow-instance_1",
            SystemTime::now() + Duration::from_secs(20), // Schedule for 20 seconds in the future
        )
        .await;

    let _ = worker
        .schedule_now(MyWorkflow::static_name(), "workflow_instance_2", None)
        .await;

    let _ = worker
        .schedule(
            MyWorkflow::static_name(),
            "failing_id",
            SystemTime::now() + Duration::from_secs(3),
        )
        .await;
    // Start the worker in its own background thread
    let worker_handle = tokio::spawn(async move {
        worker.run(1).await;
    });

    // Keep the main thread alive (or start other tasks like a web server here)
    println!("Worker running in the background. Press Ctrl+C to exit.");

    start_axum_server(db, 3008).await;
    

    // signal::ctrl_c().await.expect("failed to listen for event");

    // Optionally, join the worker thread when exiting
    worker_handle.await.unwrap();
}
