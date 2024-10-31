use std::{sync::Arc, time::Duration};

use ask_workflow::{
    activity::Activity,
    db_trait::DB,
    workflow::{run_activity, run_sync_activity, Workflow, WorkflowErrorType},
    workflow_signal::WorkflowSignal,
    workflow_state::WorkflowState,
};
use axum::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::{sleep, Sleep};

use super::mock_db::{MockDatabase, User};

// Example of a function that performs a GET request and can be retried

#[derive(Clone)]
pub struct CreateUserWorkflow {
    pub state: WorkflowState,
    pub context: Arc<CreateUserWorkflowContext>,
}

// #[typetag::serde}

pub struct CreateUserWorkflowContext {
    pub http_client: Arc<reqwest::Client>,
    pub db: Arc<MockDatabase>,
}

#[derive(Deserialize, Serialize)]
pub struct CreateUserInput {
    pub name: String,
}

#[async_trait::async_trait]
impl Workflow for CreateUserWorkflow {
    fn name(&self) -> &str {
        "CreateUserWorkflow"
    }
    fn static_name() -> &'static str {
        "CreateUserWorkflow"
    }
    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }

    fn state(&self) -> &WorkflowState {
        &self.state
    }

    async fn run(
        &mut self,
        db: Arc<dyn DB>,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        let ctx = self.context.clone();

        // Deserialize input
        println!("Running CreateUserWorkflow");
        tracing::error!("Running CreateUserWorkflow");
        let workflow_id = self.state.instance_id.clone(); // Clone first
        let state = self.state_mut();
        let input: CreateUserInput = state
            .input
            .clone()
            .ok_or_else(|| WorkflowErrorType::PermanentError {
                message: "No input provided".to_string(),
                content: None,
            })
            .and_then(|value| {
                serde_json::from_value(value).map_err(|e| WorkflowErrorType::PermanentError {
                    message: "Failed to deserialize input".to_string(),
                    content: None,
                })
            })?;

        // Activity 1: Create user
        //
        let context_clone = ctx.clone();
        println!("Running create user activirt");
        let user = run_activity("CreateUserActivity", state, db.clone(), || async move {
            create_user(context_clone, &input).await
        })
        .await?;
        println!("user is done");
        let generated_code = run_sync_activity(
            "GenerateVerificationCodeActivity",
            state,
            db.clone(),
            || Ok(generate_verification_code()),
        )
        .await?;
        println!("generated code is done {:?}", generated_code);

        let code_signal: VerificationCodeSignal = VerificationCodeSignal::await_signal(
            db.clone(),
            &state.unique_id(),
            Duration::from_millis(100),
            Duration::from_secs(60 * 5),
        )
        .await?;
        println!("recieved signal {:?}", code_signal);

        if code_signal.code != generated_code.code {
            return Err(WorkflowErrorType::PermanentError {
                message: "Verification code mismatch".to_string(),
                content: None,
            });
        }
        run_sync_activity("UserVerified", state, db.clone(), || Ok(user.clone())).await?;
        println!("User is verified" );

        // Activity 2: Send email
        let context_clone = ctx.clone();
        run_activity("SendMailActivity", state, db.clone(), || async move {
            send_email(context_clone).await
        })
        .await?;

        // Activity 3: Send Slack notification
        let context_clone = ctx.clone();
        run_activity("SlackNotifActivity", state, db.clone(), || async move {
            send_slack_notification(context_clone).await
        })
        .await?;

        // Return user as JSON
        Ok(serde_json::to_value(user).ok())
    }
}

// Helper function for creating a user
async fn create_user(
    ctx: Arc<CreateUserWorkflowContext>,
    input: &CreateUserInput,
) -> Result<User, WorkflowErrorType> {
    let user = User {
        name: input.name.clone(),
        id: cuid::cuid1().unwrap(),
    };
    ctx.db.clone().insert_user(user.clone());
    let user = ctx.db.clone().get_user(&user.id).unwrap();
    Ok(user)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerificationCode {
    code: String,
}
fn generate_verification_code() -> VerificationCode {
    VerificationCode {
        code: cuid::cuid1().unwrap(),
    }
}

// Helper function for sending an email
// activities
async fn send_email(ctx: Arc<CreateUserWorkflowContext>) -> Result<(), WorkflowErrorType> {
    let response = ctx
        .http_client
        .get("https://httpbin.org/get")
        .send()
        .await
        .map_err(|e| WorkflowErrorType::TransientError {
            message: "Failed to send email".to_string(),
            content: None,
        })?;

    sleep(Duration::from_secs(4)).await;

    response
        .error_for_status()
        .map_err(|e| WorkflowErrorType::TransientError {
            message: "Non-200 response for email".to_string(),
            content: None,
        })?;
    Ok(())
}

// Helper function for sending a Slack notification
async fn send_slack_notification(
    ctx: Arc<CreateUserWorkflowContext>,
) -> Result<(), WorkflowErrorType> {
    let response = ctx
        .http_client
        .post("https://slack.com/api/chat.postMessage")
        .json(&serde_json::json!({ "text": "New user created" }))
        .send()
        .await
        .map_err(|e| WorkflowErrorType::TransientError {
            message: "Failed to send Slack notification".to_string(),
            content: None,
        })?;

    response
        .error_for_status()
        .map_err(|e| WorkflowErrorType::TransientError {
            message: "Non-200 response for Slack notification".to_string(),
            content: None,
        })?;
    Ok(())
}

// signals
//
//
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct VerificationCodeSignal {
    workflow_id: String,
    code: String,
}

#[async_trait]
impl WorkflowSignal for VerificationCodeSignal {
    fn signal_id(&self) -> String {
        self.workflow_id.to_string()
    }
    fn static_signal_name() -> &'static str {
        "VerificationCodeSignal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ask_workflow::db_trait::{unique_workflow_id, InMemoryDB};
    use ask_workflow::worker::Worker;
    // Adjust as necessary
    use ask_workflow::workflow_state::WorkflowState;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn test_create_user_workflow() {
        println!("Running test_create_user_workflow");
        let db: Arc<dyn ask_workflow::db_trait::DB> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        let mock_db = Arc::new(MockDatabase::new());
        let mock_db_clone = mock_db.clone();
        let create_user_context = Arc::new(CreateUserWorkflowContext {
            http_client: Arc::new(reqwest::Client::new()),
            db: mock_db_clone.clone(),
        });

        println!("adding workflow");

        worker.add_workflow(CreateUserWorkflow::static_name(), move |state| {
            return Box::new(CreateUserWorkflow {
                state,
                context: create_user_context.clone(),
            });
        });

        let worker = Arc::new(worker);

        let worker_clone = worker.clone();
        println!("spawning worker");

        // Start the worker in its own background thread
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await;
        });

        let user_input = CreateUserInput {
            name: "Aksel".to_string(),
        };

        let res: VerificationCode = worker
            .execute_and_await(
                CreateUserWorkflow::static_name(),
                "Aksel",
                "GenerateVerificationCodeActivity",
                Some(user_input),
            )
            .await
            .unwrap();

        println!("reseived verification code after starting workflow, waiting on signal");

        let res_clone = res.clone();

        VerificationCodeSignal {
            workflow_id: unique_workflow_id(CreateUserWorkflow::static_name(), "Aksel"),
            code: res.code,
        }
        .send(db)
        .await;

        println!("Signal sent, waiting for user verified");

        let res: User = worker
            .poll_result(
                CreateUserWorkflow::static_name(),
                "Aksel",
                Some("UserVerified"),
                Duration::from_secs(10),
            )
            .await
            .unwrap();

        assert_eq!(res.name, "Aksel")

        // Run your test setup and assertions here
    }
}
