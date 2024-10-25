
use std::{collections::HashMap, time::SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{common_context, tera_include, workflow_state::WorkflowState, AppState};
//
use axum::{
    debug_handler, http::StatusCode, response::{Html, IntoResponse}, routing::get, Extension, Router
};

#[derive(Serialize, Deserialize)]
pub struct WorkflowView {
    pub id: String,
    pub name: String,
    pub status: String,
    pub last_run: String,
    pub results: HashMap<String, serde_json::Value>,
}
impl WorkflowView {
    fn new(workflow: &WorkflowState) -> WorkflowView {
        WorkflowView{
            name: workflow.workflow_type.clone(),
            id: workflow.instance_id.clone(),
            last_run: system_time_to_string(workflow.scheduled_at),
            results: workflow.results.clone(),
            status: workflow.status.to_string(),

        }
    }
} 
fn system_time_to_string(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn routes() -> Router {
    Router::new()
        .route("/", get(list))
        // .add("/", post(add))
        // .add("new", get(new))
        // .add(":id", get(show))
        // .add(":id/edit", get(edit))
        // .add(":id", post(update))
        // .add(":id", delete(remove))
}

#[debug_handler]
async fn list(
    Extension(app_state): Extension<AppState>,

) -> Html<String> {
    let tera = tera_include();
    let mut context = common_context();
    context.insert("page_title", "workflows");
    context.insert("message", "This is Index page.");
    let workflows = app_state.db.get_all().await;
    let views = workflows.iter().map(|wf| WorkflowView::new(wf)).collect::<Vec<WorkflowView>>();

    context.insert("workflows", &views);

    let output = tera.render("workflows/list.html", &context);
    Html(output.unwrap())
}

// #[debug_handler]
// pub async fn list(
// ) -> Result<Response> {
//     let item = Entity::find()
//         .order_by(Column::Id, Order::Desc)
//         .all(&ctx.db)
//         .await?;
//     views::workflow::list(&v, &item)
// }

// #[debug_handler]
// pub async fn new(
//     ViewEngine(v): ViewEngine<TeraView>,
//     State(_ctx): State<AppContext>,
// ) -> Result<Response> {
//     views::workflow::create(&v)
// }

// #[debug_handler]
// pub async fn update(
//     Path(id): Path<i32>,
//     State(ctx): State<AppContext>,
//     Json(params): Json<Params>,
// ) -> Result<Response> {
//     let item = load_item(&ctx, id).await?;
//     let mut item = item.into_active_model();
//     params.update(&mut item);
//     let item = item.update(&ctx.db).await?;
//     format::json(item)
// }

// #[debug_handler]
// pub async fn edit(
//     Path(id): Path<i32>,
//     ViewEngine(v): ViewEngine<TeraView>,
//     State(ctx): State<AppContext>,
// ) -> Result<Response> {
//     let item = load_item(&ctx, id).await?;
//     views::workflow::edit(&v, &item)
// }

// #[debug_handler]
// pub async fn show(
//     Path(id): Path<i32>,
//     ViewEngine(v): ViewEngine<TeraView>,
//     State(ctx): State<AppContext>,
// ) -> Result<Response> {
//     let item = load_item(&ctx, id).await?;
//     views::workflow::show(&v, &item)
// }

// #[debug_handler]
// pub async fn add(
//     ViewEngine(v): ViewEngine<TeraView>,
//     State(ctx): State<AppContext>,
//     Json(params): Json<Params>,
// ) -> Result<Response> {
//     let mut item = ActiveModel {
//         ..Default::default()
//     };
//     params.update(&mut item);
//     let item = item.insert(&ctx.db).await?;
//     views::workflow::show(&v, &item)
// }

// #[debug_handler]
// pub async fn remove(Path(id): Path<i32>, State(ctx): State<AppContext>) -> Result<Response> {
//     load_item(&ctx, id).await?.delete(&ctx.db).await?;
//     format::empty()
// }

