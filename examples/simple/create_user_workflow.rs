use std::{sync::Arc, time::Duration};

use ask_workflow::{
    run_activity_m,
    workflow::{parse_input, Workflow, WorkflowErrorType},
    workflow_signal::{SignalDirection, WorkflowSignal},
    workflow_state::WorkflowState,
};
use axum::async_trait;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use super::mock_db::{MockDatabase, User};

#[derive(Clone)]
pub struct CreateUserWorkflow {
    pub context: Arc<CreateUserWorkflowContext>,
}

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
    fn direction() -> ask_workflow::workflow_signal::SignalDirection {
        SignalDirection::FromWorkflow
    }
}

#[derive(Deserialize, Serialize)]
pub struct GeneratedCodeOut;
impl WorkflowSignal for GeneratedCodeOut {
    type Workflow = CreateUserWorkflow;
    fn direction() -> ask_workflow::workflow_signal::SignalDirection {
        SignalDirection::ToWorkflow
    }
}

#[async_trait::async_trait]
impl Workflow for CreateUserWorkflow {
    fn name(&self) -> &str {
        "CreateUserWorkflow"
    }
    fn static_name() -> &'static str {
        "CreateUserWorkflow"
    }
    async fn run(
        &mut self,
        worker: Arc<ask_workflow::worker::Worker>,
        mut state: &mut WorkflowState,
    ) -> Result<Option<serde_json::Value>, WorkflowErrorType> {
        let ctx_clone = self.context.clone();

        let (instance_id, input) = {
            let instance_id = state.instance_id.clone();
            let input = parse_input::<CreateUserInput>(state)?;
            (instance_id, input)
        };

        println!("Running CreateUserWorkflow");
        println!("Running create user activity");
        let user = run_activity_m!(state, "create_user", "async", [ctx_clone], {
            create_user(ctx_clone, &input).await
        })?;

        println!("User creation done");

        let generated_code =
            run_activity_m!(state, "generte_code", "sync", [], { Ok(generate_verification_code()) })?;
        worker
            .send_signal(
                &instance_id,
                NonVerifiedUserOut { user: user.clone() },
                Some(state.run_id.clone()),
            )
            .await?;

        println!("Generated code is {:?}", generated_code);

        // Await verification code signal
        let code_signal = worker
            .await_signal::<VerificationCodeSignal>(
                &instance_id,
                Some(state.run_id.clone()),
                Duration::from_secs(60),
            )
            .await?;

        println!("Received signal {:?}", code_signal);

        // Check if the received code matches
        if code_signal.code != generated_code.code {
            return Err(WorkflowErrorType::PermanentError {
                message: "Verification code mismatch".to_string(),
                content: None,
            });
        }

        println!("Sending email");
        run_activity_m!(state, "send_email", [ctx_clone], { send_email(ctx_clone).await })?;

        println!("Sending Slack notification");
        run_activity_m!(state, "send_to_slack", [ctx_clone], {
            send_slack_notification(ctx_clone).await
        })?;
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
        .map_err(|_e| WorkflowErrorType::TransientError {
            message: "Failed to send email".to_string(),
            content: None,
        })?;

    sleep(Duration::from_secs(4)).await;

    response
        .error_for_status()
        .map_err(|_e| WorkflowErrorType::TransientError {
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
        .map_err(|_e| WorkflowErrorType::TransientError {
            message: "Failed to send Slack notification".to_string(),
            content: None,
        })?;

    response
        .error_for_status()
        .map_err(|_e| WorkflowErrorType::TransientError {
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
    pub code: String,
}

#[async_trait]
impl WorkflowSignal for VerificationCodeSignal {
    type Workflow = CreateUserWorkflow;
    fn direction() -> ask_workflow::workflow_signal::SignalDirection {
        SignalDirection::ToWorkflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ask_workflow::db_trait::InMemoryDB;
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

        worker.add_workflow::<CreateUserWorkflow, _>(move || {
            return Box::new(CreateUserWorkflow {
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

        let run_id = worker
            .schedule_now::<CreateUserWorkflow, CreateUserInput>("Aksel", Some(user_input))
            .await
            .unwrap();

        let unverified_user: NonVerifiedUserOut = worker
            .await_signal::<NonVerifiedUserOut>("Aksel", None, Duration::from_secs(10))
            .await
            .unwrap();

        println!("Unverified user: {:?}", unverified_user);

        worker
            .send_signal(
                "Aksel",
                VerificationCodeSignal {
                    code: "123".to_string(),
                },
                None,
            )
            .await
            .unwrap();
        println!("Signal sent, waiting for user verified");

        let state = worker
            .await_workflow::<CreateUserWorkflow>(&run_id, Duration::from_secs(10), 100)
            .await
            .unwrap();

        assert_eq!(unverified_user.user.name, "Aksel");
        // assert_eq!(state.status == WorkflowStatus::Closed(Closed::Completed));
        assert_eq!(state.status, WorkflowStatus::Closed(Closed::Completed));
        // Run your test setup and assertions here
    }
}
