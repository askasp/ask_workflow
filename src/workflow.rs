use crate::db_trait::DB;
use crate::workflow_state::{WorkflowError, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ops::Add;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// The Workflow trait defines the behavior of a workflow
//
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum WorkflowErrorType {
    TransientError {
        message: String,
        content: Option<Value>,
    },
    PermanentError {
        message: String,
        content: Option<Value>,
    },
}
// #[typetag::serde(tag = "workflow_type")]
#[async_trait]
pub trait Workflow: Send + Sync {
    fn name(&self) -> &str;
    // fn static_name() -> &'static str;

    fn static_name() -> &'static str
    where
        Self: Sized;

    async fn run(&mut self) -> Result<Option<Value>, WorkflowErrorType>; // Executes the workflow logic

    fn state_mut(&mut self) -> &mut WorkflowState; // Provides mutable access to the workflow state
    fn state(&self) -> &WorkflowState; // Provides mutable access to the workflow state

    async fn execute(&mut self, db: Arc<dyn DB>) -> Result<(), WorkflowErrorType> {
        {
            let state = self.state_mut();
            state.scheduled_at = SystemTime::add(SystemTime::now(), state.claim_duration);
            if state.start_time.is_none() {
                state.start_time = Some(SystemTime::now());
            }

            db.insert(state.clone()).await;
        }
        let result = self.run().await;
        match result {
            Ok(output) => {
                let state = self.state_mut();
                state.mark_completed();
                state.output = Some(serde_json::to_value(output).unwrap_or_default());
                db.update(state.clone()).await;
                eprintln!(
                    "Workflow successfully completed {}- {}",
                    self.name(),
                    self.state().instance_id
                );
                Ok(())
            }
            Err(e) if matches!(e, WorkflowErrorType::TransientError { .. }) => {
                {
                    let state = self.state_mut();
                    state.retry();
                    db.update(state.clone()).await;
                }
                eprintln!(
                    "Got a transient error on workflow {} with id {}, the rror is {:?}",
                    self.name(),
                    self.state().instance_id,
                    e
                );
                Err(e)
            }
            Err(e2) => {
                let state = self.state_mut();
                state.mark_failed();
                state.output = Some(serde_json::to_value(e2.clone()).unwrap_or_default());
                db.update(state.clone()).await;
                Err(e2)
            }
        }
    }
}
//
//

// Function to handle activity retries and state updates
pub async fn run_activity<F, T>(
    state: &mut WorkflowState,
    activity_name: &str,
    activity_fn: F,
) -> Result<T, WorkflowErrorType>
where
    F: Fn() -> Result<T, WorkflowErrorType> + Send + Sync,
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync,
{
    // Check if the activity has already been completed
    if let Some(result) = state.get_activity_result(activity_name) {
        let result: T = serde_json::from_value(result.clone()).unwrap();
        println!("Using cached result for activity '{}'", activity_name);
        return Ok(result);
    }

    match activity_fn() {
        Ok(result) => {
            state.add_activity_result(activity_name, &result);
            return Ok(result);
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: activity_name.to_string(),
                timestamp: SystemTime::now(),
            });
            Err(e)
        }
    }
}

// Function to handle async activity retries and state updates
pub async fn run_activity_async<F, T>(
    state: &mut WorkflowState,
    activity_name: &str,
    mut activity_fn: F,
) -> Result<T, WorkflowErrorType>
where
    F: FnMut() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<T, WorkflowErrorType>> + Send>,
    >,
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync,
{
    // Check if the activity has already been completed
    if let Some(result) = state.get_activity_result(activity_name) {
        let result: T = serde_json::from_value(result.clone()).unwrap();
        println!("Using cached result for activity '{}'", activity_name);
        return Ok(result);
    }

    // Run the activity asynchronously and handle result
    match activity_fn().await {
        Ok(result) => {
            state.add_activity_result(activity_name, &result);
            Ok(result)
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: activity_name.to_string(),
                timestamp: SystemTime::now(),
            });
            Err(e)
        }
    }
}
