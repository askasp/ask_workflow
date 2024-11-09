// Define the async version of the macro
//
// use crate::workflow::run_sync_activity;
// use crate::workflow::run_activity;
#[macro_export]
macro_rules! run_activity_m {
    ($worker:expr, $name:expr, $state:expr, [$($dep:ident),*], $body:block) => {
        {
            $(let $dep = $dep.clone();)*
            $worker.run_activity(
                $name,
                $state,
                || async move { $body }
            ).await
        }
    };
}
#[macro_export]
macro_rules! run_sync_activity_m {
    ($worker:expr, $name:expr, $state:expr, [$($dep:ident),*], $body:block) => {
        {
            $(let $dep = $dep.clone();)*
            $worker.run_sync_activity(
                $name,
                $state,
                || { $body }
            ).await
        }
    };
}

