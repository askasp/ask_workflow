use std::{sync::Arc, time::Duration};

use ask_workflow::{
    run_activity_m, run_activity_with_timeout_m,
    worker::Worker,
    workflow::{DuplicateStrategy, Workflow, WorkflowErrorType},
    workflow_state::WorkflowState,
};
use serde::{Deserialize, Serialize};

use super::mock_db::MockDatabase;

#[derive(Clone)]
pub struct EnqueuedWorkflow {
    pub db: Arc<MockDatabase>,
}

#[async_trait::async_trait]
impl Workflow for EnqueuedWorkflow {
    fn name(&self) -> &str {
        "EnqueuedWorkflow"
    }
    fn static_name() -> &'static str {
        "EnqueuedWorkflow"
    }

    fn duplicate_strategy(&self) -> DuplicateStrategy {
        DuplicateStrategy::Enqueue
    }
    fn claim_duration(&self) -> Duration {
        Duration::from_secs(4)
    }

    fn static_claim_duration() -> Duration {
        Duration::from_secs(4)
    }

    fn static_duplicate_strategy() -> DuplicateStrategy
    where
        Self: Sized,
    {
        DuplicateStrategy::Enqueue
    }

    async fn run(
        &self,
        _worker: Arc<Worker>,
        mut state: &mut WorkflowState,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        let db = self.db.clone();

        tokio::time::sleep(Duration::from_secs(1)).await;

        run_activity_m!(state, "AddToCounter", "sync", [db], {
            db.increment_counter();
            Ok("Incremented".to_string())
        })?;

        Ok(None)
    }
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
    async fn test_run_enqueue_workflow() {
        let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        let mock_db = Arc::new(MockDatabase::new());
        let mock_db_clone = mock_db.clone();
        println!("adding workflow");

        worker.add_workflow::<EnqueuedWorkflow>(EnqueuedWorkflow { db: mock_db_clone });

        let worker = Arc::new(worker);
        let worker_clone = worker.clone();
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await;
        });

        let run_id = worker
            .schedule_now::<EnqueuedWorkflow, ()>("Aksel", ())
            .await
            .unwrap();

        let run_id_duplicate = worker
            .schedule_now::<EnqueuedWorkflow, ()>("Aksel", ())
            .await
            .unwrap();

        let state = worker
            .await_workflow::<EnqueuedWorkflow>(&run_id, tokio::time::Duration::from_secs(10), 500)
            .await
            .unwrap();
        assert_eq!(state.status, WorkflowStatus::Closed(Closed::Completed));

        assert_eq!(mock_db.get_counter(), 1);

        let state_duplicate = worker
            .await_workflow::<EnqueuedWorkflow>(
                &run_id_duplicate,
                tokio::time::Duration::from_secs(10),
                500,
            )
            .await
            .unwrap();

        assert_eq!(
            state_duplicate.status,
            WorkflowStatus::Closed(Closed::Completed)
        );

        assert_eq!(mock_db.get_counter(), 2);
    }
}
