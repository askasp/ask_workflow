use std::sync::Arc;

use ask_workflow::{
    run_activity_m,
    worker::Worker,
    workflow::{Workflow, WorkflowErrorType},
    workflow_state::WorkflowState,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct HttpBinResponse {
    pub origin: String,
    pub url: String,
}

#[derive(Clone)]
pub struct BasicWorkflow {
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


    async fn run(
        &self,
        _worker: Arc<Worker>,
        mut state: &mut WorkflowState,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        println!("about to run simple acitvities");
        run_activity_m!(state, "generate_number", [], { generate_number() })?;

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
