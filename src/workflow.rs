use crate::db_trait::{unique_workflow_id, DB};
use crate::worker::Worker;
use crate::workflow_signal::{Signal, WorkflowSignal};
use crate::workflow_state::{self, Closed, WorkflowError, WorkflowState, WorkflowStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;

use std::ops::Add;
use std::time::{Duration, SystemTime};

#[async_trait]
pub trait Workflow: Send + Sync {
    fn name(&self) -> &str;
    fn static_name() -> &'static str
    where
        Self: Sized;

    fn unique_id(&self) -> String
where {
        format!("{}-{}", self.name(), self.state().instance_id)
    }
    fn create_unique_id(instance_id: &str) -> String
    where
        Self: Sized,
    {
        format!("{}-{}", Self::static_name(), instance_id)
    }

    fn claim_duration(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn run(
        &mut self,
        db: Arc<dyn DB>,
        worker: Arc<Worker>,
    ) -> Result<Option<Value>, WorkflowErrorType>;

    fn state_mut(&mut self) -> &mut WorkflowState; // Provides mutable access to the workflow state
    fn state(&self) -> &WorkflowState; // Provides mutable access to the workflow state

    async fn execute(
        &mut self,
        db: Arc<dyn DB>,
        worker: Arc<Worker>,
    ) -> Result<(), WorkflowErrorType> {
        {
            let duration = self.claim_duration();
            let state = self.state_mut();
            state.scheduled_at = SystemTime::add(SystemTime::now(), duration);
            if state.start_time.is_none() {
                state.start_time = Some(SystemTime::now());
            }

            db.insert(state.clone()).await;
        }
        let result = self.run(db.clone(), worker).await;
        match result {
            Ok(output) => {
                let state = self.state_mut();
                state.mark_completed();
                state.output = Some(serde_json::to_value(output).unwrap_or_default());
                db.update(state.clone()).await;
                eprintln!("Workflow successfully completed {}", self.unique_id());
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

pub async fn run_activity<T, F, Fut>(
    name: &str,
    state: &mut WorkflowState,
    db: Arc<dyn DB>,
    func: F,
) -> Result<T, WorkflowErrorType>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, WorkflowErrorType>>,
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // Check if the activity is already completed
    if let Some(result) = state.get_activity_result(name) {
        println!("Using cached result for activity '{}'", name);
        let cached_result: T = serde_json::from_value(result.clone()).unwrap();
        return Ok(cached_result);
    }

    // Run the function and await its result
    match func().await {
        Ok(result) => {
            println!("Caching result for activity '{}'", name);
            state.add_activity_result(name, &result);
            db.update(state.clone()).await;
            Ok(result)
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: name.to_string(),
                timestamp: SystemTime::now(),
            });
            db.update(state.clone()).await;
            Err(e)
        }
    }
}
pub async fn run_sync_activity<T, F>(
    name: &str,
    state: &mut WorkflowState,
    db: Arc<dyn DB>,
    func: F,
) -> Result<T, WorkflowErrorType>
where
    F: FnOnce() -> Result<T, WorkflowErrorType>,
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // Check if the activity is already completed
    if let Some(result) = state.get_activity_result(name) {
        println!("Using cached result for activity '{}'", name);
        let cached_result: T = serde_json::from_value(result.clone()).unwrap();
        return Ok(cached_result);
    }

    // Run the function and get its result
    match func() {
        Ok(result) => {
            println!("Caching result for activity '{}'", name);
            state.add_activity_result(name, &result);
            db.update(state.clone()).await; // Await the async update call
            Ok(result)
        }
        Err(e) => {
            state.errors.push(WorkflowError {
                error_type: e.clone(),
                activity_name: name.to_string(),
                timestamp: SystemTime::now(),
            });
            db.update(state.clone()).await; // Await the async update call
            Err(e)
        }
    }
}

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
impl From<serde_json::Error> for WorkflowErrorType {
    fn from(err: serde_json::Error) -> Self {
        WorkflowErrorType::PermanentError {
            message: err.to_string(),
            content: None, // Or provide additional context if available
        }
    }
}

pub fn parse_input<S>(state: &WorkflowState) -> Result<S, WorkflowErrorType>
where
    S: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    let input: S = state
        .input
        .as_ref()
        .ok_or_else(|| WorkflowErrorType::PermanentError {
            message: "No input provided".to_string(),
            content: None,
        })
        .and_then(|value| {
            serde_json::from_value(value.clone()).map_err(|e| WorkflowErrorType::PermanentError {
                message: "Failed to deserialize input".to_string(),
                content: None,
            })
        })?;
    Ok(input)
}
