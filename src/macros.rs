// #[macro_export]
// macro_rules! run_activity_m {
//     // Case with explicit name and specified function type (async or sync)
//     ($worker:expr, $kind:literal, $name:expr, $state:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*
//         if $kind == "sync" {
//             $worker.run_activity(
//                 $name,
//                 $state,
//                 || async move { { $body } } // Wrap sync block in async
//             ).await
//         } else {
//             $worker.run_activity(
//                 $name,
//                 $state,
//                 || async move { $body }
//             ).await
//         }
//     }};

//     // Case with inferred name and specified function type (async or sync)
//     ($worker:expr, $kind:literal, $state:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*
//         let inferred_name = stringify!($body);
//         if $kind == "sync" {
//             $worker.run_activity(
//                 inferred_name,
//                 $state,
//                 || async move { { $body } } // Wrap sync block in async
//             ).await
//         } else {
//             $worker.run_activity(
//                 inferred_name,
//                 $state,
//                 || async move { $body }
//             ).await
//         }
//     }};

//     // Default async case with explicit name (no kind specified)
//     ($worker:expr, $name:expr, $state:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*
//         $worker.run_activity(
//             $name,
//             $state,
//             || async move { $body }
//         ).await
//     }};
//
//     // Default async case with inferred name (no kind specified)
//     ($worker:expr, $state:expr, [$($dep:ident),*], $body:block) => {{
//         let inferred_name = stringify!($body);
//         $(let $dep = $dep.clone();)*
//         $worker.run_activity(
//             inferred_name,
//             $state,
//             || async move { $body }
//         ).await
//     }};
// }
// #[macro_export]
// macro_rules! run_activities_in_parallel {
//     // Case with named async or sync functions
//     ($worker:expr, $state:expr, [
//         $(( $kind:literal, $name:expr, [$($dep:ident),*], $body:block )),*
//     ]) => {{
//         use tokio::join;
//         use std::sync::Arc;
//         use tokio::sync::Mutex;

//         join!(
//             $(
//                 tokio::spawn({
//                     $(let $dep = $dep.clone();)*
//                     let state = Arc::clone(&$state); // Clone Arc for each task
//                     async move {
//                         let mut state_guard = state.lock().await; // Lock the mutex for state
//                         if $kind == "sync" {
//                             $worker.run_activity(
//                                 $name,
//                                 &mut *state_guard,  // Pass unique access to the state
//                                 || async move { { $body } }
//                             ).await
//                         } else {
//                             $worker.run_activity(
//                                 $name,
//                                 &mut *state_guard,  // Pass unique access to the state
//                                 || async move { $body }
//                             ).await
//                         }
//                     }
//                 })
//             ),*
//         )
//     }};
//
//     // Case with inferred names (no explicit `name` argument) and async as default
//     ($worker:expr, $state:expr, [
//         $(( $kind:literal, [$($dep:ident),*], $body:block )),*
//     ]) => {{
//         use tokio::join;
//         use std::sync::Arc;
//         use tokio::sync::Mutex;

//         join!(
//             $(
//                 tokio::spawn({
//                     $(let $dep = $dep.clone();)*
//                     let state = Arc::clone(&$state); // Clone Arc for each task
//                     async move {
//                         let mut state_guard = state.lock().await; // Lock the mutex for state
//                         let inferred_name = stringify!($body);
//                         if $kind == "sync" {
//                             $worker.run_activity(
//                                 inferred_name,
//                                 &mut *state_guard, // Pass unique access to the state
//                                 || async move { { $body } }
//                             ).await
//                         } else {
//                             $worker.run_activity(
//                                 inferred_name,
//                                 &mut *state_guard, // Pass unique access to the state
//                                 || async move { $body }
//                             ).await
//                         }
//                     }
//                 })
//             ),*
//         )
//     }};
//}

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

// #[macro_export]
// macro_rules! run_activity_m {
//     // Case with explicit activity type (e.g., "sync" or "async") and explicit name
//     ($workflow_state:expr, $kind:literal, $name:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*

//         if $kind == "sync" {
//             $crate::workflow::run_activity(
//                 &mut $workflow_state,
//                 $name,                    // Use the provided name
//                 |state| async move { { $body } } // Wrap sync block in async
//             ).await
//         } else {
//             $crate::workflow::run_activity(
//                 &mut $workflow_state,
//                 $name,                    // Use the provided name
//                 |state| async move { $body }
//             ).await
//         }
//     }};

//     // Default async case with inferred name (no kind or name specified)
//     ($workflow_state:expr, [$($dep:ident),*], $body:block) => {{
//         let inferred_name = stringify!($body);  // Infer the name from the body
//         run_activity_m!($workflow_state, "async", inferred_name, [$($dep),*], $body)
//     }};

//     // Default async case with specified name (no kind specified)
//     ($workflow_state:expr, $name:expr, [$($dep:ident),*], $body:block) => {{
//         run_activity_m!($workflow_state, "async", $name, [$($dep),*], $body)
//     }};
// }

// #[macro_export]
// macro_rules! run_activity_m {
//     // Case with explicit activity type (e.g., "sync" or "async") and name
//     ($workflow_state:expr, $kind:literal, $name:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*
//
//         if $kind == "sync" {
//             $crate::workflow::run_activity(
//                 &mut $workflow_state,   // Pass mutable reference to `workflow_state`
//                 $name,
//                 |state| async move { { $body } } // Wrap sync block in async
//             ).await
//         } else {
//             $crate::workflow::run_activity(
//                 &mut $workflow_state,   // Pass mutable reference to `workflow_state`
//                 $name,
//                 |state| async move { $body }
//             ).await
//         }
//     }};
//
//     // Default async case with inferred name (no kind or name specified)
//     ($workflow_state:expr, [$($dep:ident),*], $body:block) => {{
//         let inferred_name = stringify!($body);
//         run_activity_m!($workflow_state, "async", inferred_name, [$($dep),*], $body)
//     }};
//
//     // Default async case with specified name (no kind specified)
//     ($workflow_state:expr, $name:expr, [$($dep:ident),*], $body:block) => {{
//         run_activity_m!($workflow_state, "async", $name, [$($dep),*], $body)
//     }};
// }

// #[macro_export]
// macro_rules! run_activity_m {
//     // Case with specified sync/async type and explicit name
//     ($db:expr, $workflow_id:expr, $kind:literal, $name:expr, [$($dep:ident),*], $body:block) => {{
//         $(let $dep = $dep.clone();)*

//         if $kind == "sync" {
//             $crate::workflow::run_activity(
//                 $db,
//                 $workflow_id,
//                 $name,
//                 |state| async move { { $body } } // Wrap sync block in async
//             ).await
//         } else {
//             $crate::workflow::run_activity(
//                 $db,
//                 $workflow_id,
//                 $name,
//                 |state| async move { $body }
//             ).await
//         }
//     }};

//     // Default async case with inferred name (no kind or name specified)
//     ($db:expr, $workflow_id:expr, [$($dep:ident),*], $body:block) => {{
//         let inferred_name = stringify!($body);
//         $crate::run_activity_m!($db, $workflow_id, "async", inferred_name, [$($dep),*], $body)
//     }};

//     // Default async case with specified name (no kind specified)
//     ($db:expr, $workflow_id:expr, $name:expr, [$($dep:ident),*], $body:block) => {{
//         $crate::run_activity_m!($db, $workflow_id, "async", $name, [$($dep),*], $body)
//     }};
// }
