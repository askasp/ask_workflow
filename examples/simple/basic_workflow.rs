use std::sync::Arc;

use ask_workflow::{
    db_trait::WorkflowDbTrait, run_activity_m, run_sync_activity_m, worker::Worker, workflow::{Workflow, WorkflowErrorType}, workflow_state::WorkflowState
};
use axum::async_trait;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct HttpBinResponse {
    pub origin: String,
    pub url: String,
}

// // Example of a function that performs a GET request and can be retried
// pub struct FetchHttpActivity {
//     pub url: String,
//     pub http_client: reqwest::Client,
// }

// #[async_trait]
// impl Activity<u16> for FetchHttpActivity {
//     async fn run(&self) -> Result<u16, WorkflowErrorType> {
//         let res = self
//             .http_client
//             .get(&self.url)
//             .send()
//             .await
//             .map_err(|err| WorkflowErrorType::TransientError {
//                 message: format!("HTTP request failed: {}", err),
//                 content: None,
//             })?;

//         let status = res.status().as_u16();
//         Ok(status)
//     }

//     fn name(&self) -> &str {
//         "FetchHttpActivity"
//     }
// // }
// pub struct SimpleActivity;
// #[async_trait]
// impl Activity<String> for SimpleActivity {
//     async fn run(&self) -> Result<String, WorkflowErrorType> {
//         println!("Running a simple activity!");

//         Ok("Activity completed".to_string())
//     }

//     fn name(&self) -> &str {
//         "SimpleActivity"
//     }
// }
// pub struct FailingActivity;
// #[async_trait]
// impl Activity<String> for FailingActivity {
//     async fn run(&self) -> Result<String, WorkflowErrorType> {
//         println!("Running a failining activity!");
//         Err(WorkflowErrorType::TransientError {
//             message: "Failed".to_string(),
//             content: None,
//         })
//     }
//     fn name(&self) -> &str {
//         "FailingActivity"
//     }
// }

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

    async fn run(
        &mut self,
        worker: Arc<Worker>,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        let state = self.state_mut();
        println!("about to run simple acitvities");

        worker
            .run_sync_activity("Simple_1", state, || {
                println!("Running simple");
                Ok(json!({"res":"hei"}))
            })
            .await?;

        worker
            .run_sync_activity("Simple_2", state, || {
                println!("Running simple");
                Ok(json!({"res":"hei"}))
            })
            .await?;

        run_sync_activity_m!(worker, "Simple_Macro", state,  [], { generate_number() })?;
        let a = run_activity_m!(worker, "Simple_Async_Macro", state,  [], { generate_number_async().await });
        println!("a: {:?}", a);

        Ok(None)
    }
}

pub fn generate_number() -> Result<String, WorkflowErrorType> {
    Ok("hei".to_string())
}
pub async fn generate_number_async() -> Result<String, WorkflowErrorType> {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    Ok("async a".to_string())
}

pub struct BasicWorkflowContext {
    pub http_client: reqwest::Client,
}
