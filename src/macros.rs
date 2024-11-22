#[macro_export]
macro_rules! run_activity_m {
    // Case with explicit activity type ("sync" or "async") and mandatory name
    ($workflow_state:expr, $name:expr, $kind:literal, [$($dep:ident),*], $body:block) => {{
        $(let $dep = $dep.clone();)*

        if $kind == "sync" {
            $crate::workflow::run_activity(
                &mut $workflow_state,
                $name,                     // Explicitly provided name
                None,
                |_| async move { { $body } } // Wrap sync block in async
            ).await
        } else {
            $crate::workflow::run_activity(
                &mut $workflow_state,
                $name,                     // Explicitly provided name
                None,
                |_| async move { $body }
            ).await
        }
    }};

    // Default to "async" when `kind` is not specified
    ($workflow_state:expr, $name:expr, [$($dep:ident),*], $body:block) => {{
        $(let $dep = $dep.clone();)*

        $crate::workflow::run_activity(
            &mut $workflow_state,
            $name,                         // Explicitly provided name
            None,
            |_| async move { $body }       // Defaults to async
        ).await
    }};
}

#[macro_export]
macro_rules! run_activity_with_timeout_m {
    // Case with explicit activity type ("sync" or "async") and mandatory name
    ($workflow_state:expr, $name:expr, $timeout_duration:expr, [$($dep:ident),*], $body:block) => {{
        $(let $dep = $dep.clone();)*

  let timeout_activity_name = format!("{}Timeout", $name);
        let timeout_time: std::time::SystemTime = $crate::workflow::run_activity(
            &mut $workflow_state,
            &timeout_activity_name,
            None,
            |_| async move {
                Ok(std::time::SystemTime::now() + $timeout_duration) // Calculate timeout time
            },
        ).await?;


            $crate::workflow::run_activity(
                &mut $workflow_state,
                $name,                     // Explicitly provided name
                Some(timeout_time),
                |_| async move { $body }
            ).await
        }
    }}

// Default to "async" when `kind` is not specified
