use tera::Tera;
use views::components::toast::Toast;

pub mod controllers;
pub mod views;

pub fn common_context() -> tera::Context {
    let mut context = tera::Context::new();
    context.insert("title", "axum-tera");
    context
}

pub fn tera_include() -> Tera {
    let tera = Tera::new("src/ui/views/**/*").unwrap();
    tera
}

fn create_context_base(base: BaseContext) -> tera::Context {
    let mut context = common_context();
    context.insert("page_title", &base.title);
    context.insert("toast", &base.toast);
    context.insert("toast", &base.toast);
    context

}

pub struct BaseContext {
    title: String,
    toast: Option<Toast> 
}

