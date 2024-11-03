use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::db_trait::DB;
use crate::workflow::{Workflow, WorkflowErrorType};
use crate::workflow_state::WorkflowState;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct Worker {
    pub db: Arc<dyn DB>,
    pub workflows: HashMap<
        String,
        Box<dyn Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync>,
    >,
}

impl Worker {
    // Create a new Worker with the provided database
    pub fn new(db: Arc<dyn DB>) -> Self {
        Self {
            db,
            workflows: HashMap::new(),
        }
    }
    pub fn workflow_keys(&self) -> Vec<String> {
        self.workflows.keys().cloned().collect()
    }


    pub fn add_workflow<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(WorkflowState) -> Box<dyn Workflow + Send + Sync> + Send + Sync + 'static,
    {
        self.workflows.insert(name.to_string(), Box::new(factory));
    }
    // Schedule a workflow to run immediately

    pub fn workflow_names(&self) -> Vec<String> {
        self.workflows.keys().cloned().collect()
    }

    pub async fn schedule_now(
        &self,
        workflow_name: &str,
        instance_id: &str,
        input: Option<Value>,
    ) -> Result<(), &'static str> {
        self.schedule(workflow_name, instance_id, SystemTime::now(), input)
            .await
    }

    // Schedule a workflow to run at a specific time
    pub async fn schedule(
        &self,
        workflow_name: &str,
        instance_id: &str,
        scheduled_at: SystemTime,
        input: Option<Value>,
    ) -> Result<(), &'static str> {
        if let Some(_workflow_factory) = self.workflows.get(workflow_name) {
            let workflow_state =
                WorkflowState::new(workflow_name, instance_id, scheduled_at, input);
            self.db.insert(workflow_state.clone()).await;

            Ok(())
        } else {
            Err("Workflow not found")
        }
    }
    pub async fn execute(
        &self,
        workflow_name: &str,
        instance_id: &str,
        activity_name: &str,
        input: Option<Value>,
    ) -> Result<oneshot::Receiver<Value>, &'static str> {
        // Schedule the workflow immediately
        self.schedule_now(workflow_name, instance_id, input).await?;

        // Create a oneshot channel to return the result
        let (sender, receiver) = oneshot::channel();

        // Polling task to check activity completion
        let db = Arc::clone(&self.db);
        let instance_id = instance_id.to_string();
        let workflow_name = workflow_name.to_string();
        let activity_name = activity_name.to_string();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;

                // Fetch the workflow state to check activity status
                if let Ok(Some(workflow_state)) = db.get_workflow_state(&workflow_name,&instance_id).await {
                    println!("workflow state is {:?}", workflow_state);
                    if let Some(result) = workflow_state.results.get(&activity_name) {
                        // Activity is complete; send the result through the oneshot channel
                        let _ = sender.send(result.clone());
                        break;
                    }
                } else {
                    eprintln!("Failed to fetch workflow state or activity result");
                }
            }
        });

        // Return the receiver to the caller so they can await the result
        Ok(receiver)
    }
    pub async fn execute_and_await<T, I>(
        &self,
        workflow_name: &str,
        instance_id: &str,
        activity_name: &str,
        input: Option<I>,
    ) -> Result<T, &'static str>
    where
        T: DeserializeOwned,
        I: Serialize,
    {
        // Call `execute` to get the receiver for the result
        let receiver = self
            .execute(
                workflow_name,
                instance_id,
                activity_name,
                input.map(|i| serde_json::to_value(i).unwrap()),
            )
            .await?;

        // Await the result from the oneshot receiver
        let result_value = receiver.await.map_err(|_| "Failed to receive result")?;

        // Deserialize the result into the expected type
        let result: T =
            serde_json::from_value(result_value).map_err(|_| "Failed to deserialize result")?;

        Ok(result)
    }

    pub async fn run(&self, interval_millis: u64) {
        let mut interval = tokio::time::interval(Duration::from_millis(interval_millis));
        loop {
            interval.tick().await;
            let now = SystemTime::now();
            let due_workflows = self.db.query_due(now).await;
            for workflow_state in due_workflows {
                if let Some(workflow_factory) = self.workflows.get(&workflow_state.workflow_type) {
                    let db = Arc::clone(&self.db);

                    let mut workflow = workflow_factory(workflow_state.clone());

                    tokio::spawn(async move {
                        let res = workflow.execute(db).await;
                        if !res.is_ok() {
                            eprintln!("Error executing workflow: {:?}", res);
                        }
                    });
                }
            }
        }
    }
    pub async fn poll_result<T>(
        &self,
        workflow_name: &str,
        instance_id: &str,
        activity_name: Option<&str>,
        timeout: Duration,
    ) -> Result<T, &'static str>
    where
        T: DeserializeOwned,
    {
        // Create a oneshot channel to send the result when found
        let (sender, receiver) = oneshot::channel();

        // Clone db and instance_id for use within the spawned task
        let db = Arc::clone(&self.db);
        let instance_id = instance_id.to_string();
        let activity_name = activity_name.map(|s| s.to_string());

        let workflow_name_clone = workflow_name.to_string();
        let instance_id_clone= instance_id.to_string();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            let start = tokio::time::Instant::now();

            loop {
                interval.tick().await;

                // Timeout check
                if start.elapsed() >= timeout {
                    let _ = sender.send(Err("Timeout waiting for result"));
                    break;
                }

                // Fetch the workflow state
                if let Ok(Some(workflow_state)) = db.get_workflow_state(&workflow_name_clone, &instance_id_clone).await {
                    // Check if we're waiting on a specific activity result or the overall output
                    let result = if let Some(ref activity) = activity_name {
                        workflow_state.results.get(activity)
                    } else {
                        workflow_state.output.as_ref()
                    };

                    if let Some(result_value) = result {
                        // Send the result through the channel and exit the loop
                        let _ = sender.send(Ok(result_value.clone()));
                        break;
                    }
                }
            }
        });

        // Wait for the result or the timeout
        let result_value = receiver.await.map_err(|_| "Failed to receive result")??;

        // Deserialize the result into the expected type
        let result: T =
            serde_json::from_value(result_value).map_err(|_| "Failed to deserialize result")?;

        Ok(result)
    }

    // Function to schedule a workflow to run immediately
}

// Run the worker, periodically checking for due workflows and executing them
//
//
