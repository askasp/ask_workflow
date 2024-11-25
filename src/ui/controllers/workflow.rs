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
    workflow_signal::Signal,
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
    Router::new()
        .route("/", get(list))
        .route("/", post(add))
        .route("/table", get(table))
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
        .schedule_now_with_name(&params.name, &params.id, input.clone())
        .await;

    render_page(
        &app_state,
        Some(Toast {
            title: "Workflow Created".to_string(),
            variant: "success".to_string(),
        }),
        TableQueryParams {
            cursor: None,
            limit: Some(10),
            reverse: Some(false),
        },
    )
    .await
}

#[debug_handler]
async fn list(
    Extension(app_state): Extension<AppState>,
    Query(params): Query<TableQueryParams>,
) -> Html<String> {
    render_page(&app_state, None, params).await
}

#[debug_handler]
async fn table(
    Extension(app_state): Extension<AppState>,
    Query(params): Query<TableQueryParams>,
) -> Html<String> {
    let cursor = params.cursor.clone();
    let limit = params.limit.unwrap_or(10);

    println!("Cursor: {:?}", cursor);
    println!("Limit: {:?}", limit);

    render_table(&app_state, cursor, limit, params.reverse.unwrap_or(false)).await
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TableQueryParams {
    cursor: Option<String>,
    limit: Option<usize>,
    reverse: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateWorfklowParams {
    name: String,
    id: String,
    input: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ComponentQuery {
    sucess_message: Option<String>,
}

async fn render_page(
    app_state: &AppState,
    toast: Option<Toast>,
    table_params: TableQueryParams,
) -> Html<String> {
    let tera = tera_include();
    let mut context = create_context_base(BaseContext {
        title: "workflows".to_string(),
        toast,
    });

    // Default behavior: Fetch the latest 10 workflows
    let workflows = app_state
        .worker
        .db
        .get_by_cursor(
            table_params.cursor.clone(),
            table_params.limit.unwrap_or(10),
            table_params.reverse.unwrap_or(false),
        )
        .await
        .unwrap();

    let signals = app_state
        .worker
        .db
        .get_signals_for_workflows(workflows.iter().map(|w| w.run_id.clone()).collect())
        .await
        .unwrap();

    let views = workflows
        .iter()
        .map(|wf| {
            let filtered_signals: Vec<Signal> = signals
                .iter()
                .filter(|s| s.target_or_source_run_id == Some(wf.run_id.clone()))
                .cloned()
                .collect();

            WorkflowView::new(wf, &filtered_signals)
        })
        .collect::<Vec<_>>();

    let next_cursor = workflows
        .last()
        .and_then(|w| w.updated_at) // Access the `updated_at` field if it exists
        .map(system_time_to_rfc3339);

    let prev_cursor = workflows
        .first()
        .and_then(|w| w.updated_at) // Access the `updated_at` field if it exists
        .map(system_time_to_rfc3339);

    let workflow_context = WorkflowContext {
        workflows: views,
        names: app_state.worker.workflow_names(),
        next_cursor,
        prev_cursor,
        limit: 10,
    };

    workflow_context.add_to_context(&mut context);

    let output = tera.render("workflows/list.html", &context);
    Html(output.unwrap())
}

async fn render_table(
    app_state: &AppState,
    cursor: Option<String>,
    limit: usize,
    reverse: bool,
) -> Html<String> {
    let tera = tera_include();
    let mut context = create_context_base(BaseContext {
        title: "workflow table".to_string(),
        toast: None,
    });

    let workflows = app_state
        .worker
        .db
        .get_by_cursor(cursor.clone(), limit, false)
        .await
        .unwrap();

    let signals = app_state
        .worker
        .db
        .get_signals_for_workflows(workflows.iter().map(|w| w.run_id.clone()).collect())
        .await
        .unwrap();

    let views = workflows
        .iter()
        .map(|wf| {
            let filtered_signals: Vec<Signal> = signals
                .iter()
                .filter(|s| s.target_or_source_run_id == Some(wf.run_id.clone()))
                .cloned()
                .collect();

            WorkflowView::new(wf, &filtered_signals)
        })
        .collect::<Vec<_>>();
    let next_cursor = workflows
        .last()
        .and_then(|w| w.updated_at) // Access the `updated_at` field if it exists
        .map(system_time_to_rfc3339);

    let prev_cursor = workflows
        .first()
        .and_then(|w| w.updated_at) // Access the `updated_at` field if it exists
        .map(system_time_to_rfc3339);

    let workflow_context = WorkflowContext {
        workflows: views,
        names: vec![], // No workflow names required for the table
        next_cursor,
        prev_cursor: cursor.clone(),
        limit,
    };

    workflow_context.add_to_context(&mut context);

    let output = tera.render("workflows/components/workflow_table.html", &context);
    Html(output.unwrap())
}
// Convert SystemTime to RFC 3339 string
fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = DateTime::<Utc>::from(time);
    datetime.to_rfc3339()
}
