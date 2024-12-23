use std::{sync::Arc, time::Duration};

use ask_workflow::{
    run_activity_m, run_activity_with_timeout_m,
    worker::Worker,
    workflow::{DuplicateStrategy, Workflow, WorkflowErrorType},
    workflow_state::WorkflowState,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct BasicWorkflow {}

#[async_trait::async_trait]
impl Workflow for BasicWorkflow {
    fn name(&self) -> &str {
        "BasicWorkflow"
    }
    fn static_name() -> &'static str {
        "BasicWorkflow"
    }

    fn duplicate_strategy(&self) -> DuplicateStrategy {
        DuplicateStrategy::Reject
    }
    fn static_duplicate_strategy() -> DuplicateStrategy
    where
        Self: Sized,
    {
        DuplicateStrategy::Reject
    }

    async fn run(
        &self,
        worker: Arc<Worker>,
        mut state: &mut WorkflowState,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        println!("about to run simple acitvities");
        run_activity_m!(state, "generate_number", "sync", [], { generate_number() })?;

        run_activity_m!(state, "generate_number_async", [], {
            generate_number_async().await
        })?;

        // Timeout-aware activity
        println!("Starting activity with timeout");
        run_activity_with_timeout_m!(state, "timeout_activity", Duration::from_secs(5), [], {
            Ok("Timeout Activity Completed".to_string())
        })?;

        Ok(None)
    }
}

pub fn generate_number() -> Result<String, WorkflowErrorType> {
    Ok("hei".to_string())
}
pub async fn generate_number_async() -> Result<String, WorkflowErrorType> {
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    Ok("async a".to_string())
}

pub struct BasicWorkflowContext {
    pub http_client: reqwest::Client,
}

#[cfg(test)]
mod tests {
    use crate::simple::mock_db::MockDatabase;

    use super::*;
    use ask_workflow::db_trait::InMemoryDB;
    use ask_workflow::worker::Worker;
    // Adjust as necessary
    use ask_workflow::workflow_state::{Closed, WorkflowState, WorkflowStatus};
    use chrono::Duration;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_run_basic_workflow() {
        let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        let mock_db = Arc::new(MockDatabase::new());
        let mock_db_clone = mock_db.clone();
        println!("adding workflow");

        worker.add_workflow::<BasicWorkflow>(BasicWorkflow {});

        let worker = Arc::new(worker);
        let worker_clone = worker.clone();
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await;
        });

        let run_id = worker
            .schedule_now::<BasicWorkflow, ()>("Aksel", ())
            .await
            .unwrap();

        let state = worker
            .await_workflow::<BasicWorkflow>(&run_id, tokio::time::Duration::from_secs(10), 500)
            .await
            .unwrap();
        assert_eq!(state.status, WorkflowStatus::Closed(Closed::Completed));
    }

    #[tokio::test]
    async fn test_run_create_duplicate_workflow() {
        let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        let mock_db = Arc::new(MockDatabase::new());
        let mock_db_clone = mock_db.clone();
        println!("adding workflow");

        worker.add_workflow::<BasicWorkflow>(BasicWorkflow {});

        let worker = Arc::new(worker);
        let worker_clone = worker.clone();
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await;
        });

        let run_id = worker
            .schedule_now::<BasicWorkflow, ()>("Aksel", ())
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let run_id_duplicate = worker
            .schedule_now::<BasicWorkflow, ()>("Aksel", ())
            .await
            .unwrap();

        let state_duplicate = worker
            .await_workflow::<BasicWorkflow>(
                &run_id_duplicate,
                tokio::time::Duration::from_secs(10),
                500,
            )
            .await;

        assert_eq!(
            state_duplicate,
            Err(WorkflowErrorType::PermanentError {
                message: "Duplicate workflow detected".to_string(),
                content: None
            })
        );

        let state = worker
            .await_workflow::<BasicWorkflow>(&run_id, tokio::time::Duration::from_secs(10), 500)
            .await
            .unwrap();
        assert_eq!(state.status, WorkflowStatus::Closed(Closed::Completed));
    }
}
