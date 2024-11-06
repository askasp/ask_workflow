use std::{sync::Arc, time::Duration};

use ask_workflow::{
    db_trait::WorkflowDbTrait,
    workflow::{run_activity, run_sync_activity, Workflow, WorkflowErrorType},
    workflow_signal::WorkflowSignal,
    workflow_state::WorkflowState,
};
use axum::async_trait;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Sleep};

use super::mock_db::{MockDatabase, User};


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

#[derive(Deserialize, Serialize, Debug)]
pub struct NonVerifiedUserOut {
    pub user: User,
}
impl WorkflowSignal for NonVerifiedUserOut {
    type Workflow = CreateUserWorkflow;
}

#[derive(Deserialize, Serialize)]
pub struct GeneratedCodeOut;
impl WorkflowSignal for GeneratedCodeOut {
    type Workflow = CreateUserWorkflow;
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
        worker: Arc<ask_workflow::worker::Worker>,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        // Clone necessary data from `self` to avoid multiple borrows
        let workflow_name = self.name().to_string();
        let ctx_clone = self.context.clone();

        // Borrow `self.state_mut()` once to extract needed fields and avoid repeated mutable borrows
        let (instance_id, input) = {
            let state = self.state_mut();
            let instance_id = state.instance_id.clone();

            let input = state
                .input
                .as_ref()
                .ok_or_else(|| WorkflowErrorType::PermanentError {
                    message: "No input provided".to_string(),
                    content: None,
                })
                .and_then(|value| {
                    serde_json::from_value(value.clone()).map_err(|_| {
                        WorkflowErrorType::PermanentError {
                            message: "Failed to deserialize input".to_string(),
                            content: None,
                        }
                    })
                })?;

            (instance_id, input)
        };

        println!("Running CreateUserWorkflow");

        // Activity 1: Create user
        println!("Running create user activity");
        let user = worker
            .run_activity("CreateUserActivity", self.state_mut(), || {
                let ctx_clone = ctx_clone.clone();

                async move { create_user(ctx_clone, &input).await }
            })
            .await?;

        println!("User creation done");

        // Sync Activity: Generate verification code
        let generated_code = worker
            .run_sync_activity("GenerateVerificationCodeActivity", self.state_mut(), || {
                Ok(generate_verification_code())
            })
            .await?;

        // Send signal with the created user data
        worker
            .send_signal(&instance_id, NonVerifiedUserOut { user: user.clone() })
            .await?;

        println!("Generated code is {:?}", generated_code);

        // Await verification code signal
        let code_signal = worker
            .await_signal::<VerificationCodeSignal>(&instance_id, Duration::from_secs(60))
            .await?;

        println!("Received signal {:?}", code_signal);

        // Check if the received code matches
        if code_signal.code != generated_code.code {
            return Err(WorkflowErrorType::PermanentError {
                message: "Verification code mismatch".to_string(),
                content: None,
            });
        }

        // Sync Activity: Mark user as verified
        worker
            .run_sync_activity("UserVerified", self.state_mut(), || Ok(user.clone()))
            .await?;
        println!("User is verified");

        // Activity 2: Send email
        println!("Sending email");
        worker
            .run_activity("SendMailActivity", self.state_mut(), || {
                let ctx_clone = ctx_clone.clone();
                async move { send_email(ctx_clone).await }
            })
            .await?;

        // Activity 3: Send Slack notification
        println!("Sending Slack notification");
        worker
            .run_activity("SlackNotifActivity", self.state_mut(), || async move {
                send_slack_notification(ctx_clone.clone()).await
            })
            .await?;

        // Return the final result
        Ok(Some(serde_json::to_value(user)?))
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
        code: "123".to_string(), // code: cuid::cuid1().unwrap(),
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
    code: String,
}

#[async_trait]
impl WorkflowSignal for VerificationCodeSignal {
    type Workflow = CreateUserWorkflow;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ask_workflow::db_trait::{unique_workflow_id, InMemoryDB};
    use ask_workflow::worker::Worker;
    // Adjust as necessary
    use ask_workflow::workflow_state::{Closed, WorkflowState, WorkflowStatus};
    use serde_json::json;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn test_create_user_workflow() {
        println!("Running test_create_user_workflow");
        let db: Arc<dyn ask_workflow::db_trait::WorkflowDbTrait> = Arc::new(InMemoryDB::new());
        let mut worker = Worker::new(db.clone());
        let mock_db = Arc::new(MockDatabase::new());
        let mock_db_clone = mock_db.clone();
        let create_user_context = Arc::new(CreateUserWorkflowContext {
            http_client: Arc::new(reqwest::Client::new()),
            db: mock_db_clone.clone(),
        });

        println!("adding workflow");

        worker.add_workflow::<CreateUserWorkflow, _>(move |state| {
            return Box::new(CreateUserWorkflow {
                state,
                context: create_user_context.clone(),
            });
        });

        let worker = Arc::new(worker);
        let worker_clone = worker.clone();
        let worker_handle = tokio::spawn(async move {
            worker_clone.run(100).await;
        });

        let user_input = CreateUserInput {
            name: "Aksel".to_string(),
        };

        let _ = worker
            .schedule_now::<CreateUserWorkflow, CreateUserInput>("Aksel", Some(user_input))
            .await;

        let unverified_user: NonVerifiedUserOut = worker
            .await_signal::<NonVerifiedUserOut>("Aksel", Duration::from_secs(10))
            .await
            .unwrap();

        println!("Unverified user: {:?}", unverified_user);

        worker
            .send_signal(
                "Aksel",
                VerificationCodeSignal {
                    code: "123".to_string(),
                },
            )
            .await
            .unwrap();
        println!("Signal sent, waiting for user verified");

        let state = worker
            .await_workflow::<CreateUserWorkflow>("Aksel", Duration::from_secs(10), 100)
            .await
            .unwrap();

        assert_eq!(unverified_user.user.name, "Aksel");
        // assert_eq!(state.status == WorkflowStatus::Closed(Closed::Completed));
        assert_eq!(state.status, WorkflowStatus::Closed(Closed::Completed));
        // Run your test setup and assertions here
    }
}
