use std::time::Duration;
use std::{sync::Arc, time::SystemTime};

use ask_workflow::{
    run_activity_with_timeout_m,
    worker::Worker,
    workflow::{Workflow, WorkflowErrorType},
    workflow_state::WorkflowState,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PollFailWorkflow {}

#[async_trait::async_trait]
impl Workflow for PollFailWorkflow {
    fn name(&self) -> &str {
        "PollFailWorkflow"
    }
    fn static_name() -> &'static str {
        "PollFailWorkflow"
    }

    async fn run(
        &self,
        _worker: Arc<Worker>,
        mut state: &mut WorkflowState,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        println!("Starting PollFailWorkflow");

        // Simulate a polling activity with a timeout shorter than the sleep duration
        run_activity_with_timeout_m!(
            state,
            "long_running_poll_activity",
            Duration::from_secs(5), // Timeout duration
            [],
            {
                // Simulate a long-running operation (longer than the timeout)
                Err(WorkflowErrorType::Pending {
                    schedule_time: SystemTime::now() + Duration::from_secs(1),
                })
            }
        )?;

        Ok(Some(
            serde_json::json!({ "result": "PollFailWorkflow completed successfully" }),
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ask_workflow::db_trait::InMemoryDB;
    use ask_workflow::worker::Worker;
    use ask_workflow::workflow_state::{Closed, WorkflowState, WorkflowStatus};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_poll_fail_workflow_timeout() {
        // Setup database and worker
        let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        println!("Adding PollFailWorkflow");

        worker.add_workflow::<PollFailWorkflow>(PollFailWorkflow {});

        let worker = Arc::new(worker);
        let worker_clone = worker.clone();
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await; // Run the worker with a short interval
        });

        // Schedule the workflow
        let run_id = worker
            .schedule_now::<PollFailWorkflow, ()>("PollFailInstance", ())
            .await
            .unwrap();

        // Wait for the workflow to complete
        let state = worker
            .await_workflow::<PollFailWorkflow>(&run_id, Duration::from_secs(15), 500)
            .await;

        assert!(state.is_err());
        match state {
            Err(WorkflowErrorType::PermanentError { message, content }) => {
                assert_eq!(message, "Activity timeout")
            }
            _ => panic!("Unexpected workflow state"),
        }

        println!("PollFailWorkflow test completed successfully");
    }
}
