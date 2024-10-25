use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::{
    workflow::WorkflowErrorType,
    workflow_state::{WorkflowError, WorkflowState},
};

#[async_trait]
pub trait Activity<T>: Send + Sync
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    // The core logic that the implementer will define
    async fn run(&self) -> Result<T, WorkflowErrorType>;

    // Activity name
    fn name(&self) -> &str;

    // Default execute method that handles state saving and caching
    async fn execute(&self, state: &mut WorkflowState) -> Result<T, WorkflowErrorType> {
        // Check if the activity is already completed
        if let Some(result) = state.get_activity_result(self.name()) {
            println!("Using cached result for activity '{}'", self.name());
            let cached_result: T = serde_json::from_value(result.clone()).unwrap();
            return Ok(cached_result);
        }

        // Run the core activity logic
        match self.run().await {
            Ok(result) => {
                // Save result in the state
                println!("Caching result for activity '{}'", self.name());
                state.add_activity_result(self.name(), &result);
                Ok(result)
            }
            Err(e) => {
                // Log errors in the workflow state
                state.errors.push(WorkflowError {
                    error_type: e.clone(),
                    activity_name: self.name().to_string(),
                    timestamp: SystemTime::now(),
                });
                Err(e)
            }
        }
    }
}
