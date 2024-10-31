use std::{collections::HashMap, path::Component, time::SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ui::{
        create_context_base, tera_include,
        views::{
            components::toast::Toast,
            workflows::{WorkflowContext, WorkflowView},
        },
        BaseContext,
    },
    workflow_state::WorkflowState,
    AppState,
};
//
use axum::{
    debug_handler,
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Extension, Form, Json, Router,
};

pub fn routes() -> Router {
    Router::new().route("/", get(list)).route("/", post(add))
    // .add("new", get(new))
    // .add(":id", get(show))
    // .add(":id/edit", get(edit))
    // .add(":id", post(update))
    // .add(":id", delete(remove))
}

#[debug_handler]
async fn add(
    Extension(app_state): Extension<AppState>,
    Form(params): Form<CreateWorfklowParams>,
) -> Html<String> {
    println!("Creating workflow with params: {:?}", params);

    let input: Option<serde_json::Value> = params
        .input
        .as_ref()
        .and_then(|i| serde_json::from_str(i).ok());

    println!("parsed input is: {:?}", input);


    let _ = app_state
        .worker
        .schedule_now(&params.name, &params.id, input.clone())
        .await;

    let receiver = app_state
        .worker
        .execute(&params.name, &params.id, "CreateUserActivity", input.clone());
    let title = match receiver.await {
        Ok(result) => {
            let value = result.await;
            value.unwrap().to_string()
        }
        Err(_) => {
            // Handle the case where the sender dropped before sending
            eprintln!("The workflow activity did not complete successfully.");
            "Workflow activity did not complete successfully".to_string()
        }
    };

    render_page(
        &app_state,
        None,
        Some(Toast {
            title: title,
            variant: "success".to_string(),
        }),
    )
    .await
}

#[debug_handler]
async fn list(
    Extension(app_state): Extension<AppState>,
    Query(query): Query<ComponentQuery>,
) -> Html<String> {
    render_page(&app_state, query.component, None).await
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateWorfklowParams {
    name: String,
    id: String,
    input: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ComponentQuery {
    component: Option<HtmlComponent>,
    sucess_message: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum HtmlComponent {
    WorfklowTable,
    All,
}
async fn render_page(
    app_state: &AppState,
    component: Option<HtmlComponent>,
    toast: Option<Toast>,
) -> Html<String> {
    let tera = tera_include();
    let mut context = create_context_base(BaseContext {
        title: "workflows".to_string(),
        toast,
    });

    let workflows = app_state.worker.db.get_all().await;
    let views = workflows
        .iter()
        .map(|wf| WorkflowView::new(wf))
        .collect::<Vec<WorkflowView>>();

    let workflow_context = WorkflowContext {
        workflows: views,
        names: app_state.worker.workflow_names(),
    };
    workflow_context.add_to_context(&mut context);
    match component {
        Some(HtmlComponent::WorfklowTable) => {
            let output = tera.render("workflows/workflow_table.html", &context);
            Html(output.unwrap())
        }
        Some(HtmlComponent::All) => {
            let output = tera.render("workflows/list.html", &context);
            Html(output.unwrap())
        }
        _ => {
            let output = tera.render("workflows/list.html", &context);
            Html(output.unwrap())
        }
    }
}
