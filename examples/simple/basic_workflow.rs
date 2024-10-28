use std::sync::Arc;

use ask_workflow::{activity::Activity, workflow::{Workflow, WorkflowErrorType}, workflow_state::WorkflowState};
use axum::async_trait;
use serde::Deserialize;


#[derive(Deserialize)]
pub struct HttpBinResponse {
    pub origin: String,
    pub url: String,
}

// Example of a function that performs a GET request and can be retried
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
pub struct BasicWorkflow {
    pub state: WorkflowState,
    pub context: Arc<BasicWorkflowContext>,
}

// #[typetag::serde]
#[async_trait::async_trait]
impl Workflow for BasicWorkflow {
    fn name(&self) -> &str {
        "BasicWorkflow"
    }
    fn static_name() -> &'static str {
        "BasicWorkflow"
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
    fn state(&self) -> &WorkflowState {
        &self.state
    }

    async fn run(&mut self) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
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

pub struct BasicWorkflowContext {
    pub http_client: reqwest::Client,
}

