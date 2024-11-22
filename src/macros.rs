

#[macro_export]
macro_rules! run_activity_m {
    // Case with explicit activity type ("sync" or "async") and mandatory name
    ($workflow_state:expr, $name:expr, $kind:literal, [$($dep:ident),*], $body:block) => {{
        $(let $dep = $dep.clone();)*

        if $kind == "sync" {
            $crate::workflow::run_activity(
                &mut $workflow_state,
                $name,                     // Explicitly provided name
                |_| async move { { $body } } // Wrap sync block in async
            ).await
        } else {
            $crate::workflow::run_activity(
                &mut $workflow_state,
                $name,                     // Explicitly provided name
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
            |_| async move { $body }       // Defaults to async
        ).await
    }};
}
#[macro_export]
macro_rules! run_poll_activity {
    (
        $workflow_state:expr,
        $activity_name:expr,
        $timeout_duration:expr,
        [$($dep:ident),*],
        $poller_body:block
    ) => {{
        $(let $dep = $dep.clone();)*

        // Run the timeout activity (cached automatically by `run_activity`)
        let timeout_activity_name = format!("{}Timeout", $activity_name);
        let timeout_time: std::time::SystemTime = $crate::workflow::run_activity(
            &mut $workflow_state,
            &timeout_activity_name,
            |_| async move {
                Ok(std::time::SystemTime::now() + $timeout_duration) // Calculate timeout time
            },
        ).await?;

        // Check if the timeout has been exceeded
        if std::time::SystemTime::now() >= timeout_time {
            return Err($crate::workflow::WorkflowErrorType::PermanentError {
                message: "Timeout reached during polling".to_string(),
                content: None,
            });
        }

        // Run the polling activity
        $crate::workflow::run_activity(
            &mut $workflow_state,
            $activity_name,
            |_| async move { $poller_body },
        ).await
    }};
}
