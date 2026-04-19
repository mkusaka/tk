use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::json;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::error::TkError;
use crate::output::{json_line, task_map_json, task_view_map};
use crate::storage::{ListFilters, ListStore};

pub fn watch_list(
    store: &ListStore,
    interval_ms: u64,
    include_internal: bool,
) -> Result<(), TkError> {
    store.ensure_storage_dirs()?;
    let filters = ListFilters {
        statuses: Default::default(),
        owner: None,
        unowned: false,
        claimable: false,
        include_internal,
        limit: None,
    };
    let (manifest, tasks) = store.list_task_views(&filters)?;
    let mut previous_manifest_revision = manifest.list_revision;
    let mut previous = task_view_map(&tasks);
    json_line(&json!({
        "type": "snapshot",
        "list": {
            "list_id": manifest.list_id,
            "list_revision": manifest.list_revision,
        },
        "tasks": task_map_json(&tasks),
    }))?;

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let watcher = RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        Config::default(),
    );

    let mut event_driven = false;
    let mut maybe_watcher = match watcher {
        Ok(mut watcher) => {
            if watcher
                .watch(&store.paths.list_dir, RecursiveMode::Recursive)
                .is_ok()
            {
                event_driven = true;
                Some(watcher)
            } else {
                None
            }
        }
        Err(_) => None,
    };

    loop {
        if event_driven {
            match rx.recv_timeout(Duration::from_millis(interval_ms.max(100))) {
                Ok(Ok(_event)) => {}
                Ok(Err(_err)) => {}
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    maybe_watcher = None;
                    event_driven = false;
                    continue;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(interval_ms.max(100)));
        }
        let (manifest, tasks) = store.list_task_views(&filters)?;
        let current = task_view_map(&tasks);

        if current.is_empty() && !previous.is_empty() {
            json_line(&json!({
                "type": "list_reset",
                "list": {
                    "list_id": manifest.list_id,
                    "list_revision": manifest.list_revision,
                }
            }))?;
            previous.clear();
            previous_manifest_revision = manifest.list_revision;
            continue;
        }

        for (task_id, task) in &current {
            match previous.get(task_id) {
                None => json_line(&json!({
                    "type": "task_created",
                    "list": {
                        "list_id": manifest.list_id,
                        "list_revision": manifest.list_revision,
                    },
                    "task": task,
                }))?,
                Some(previous_task)
                    if serde_json::to_value(previous_task).ok()
                        != serde_json::to_value(task).ok() =>
                {
                    json_line(&json!({
                        "type": "task_updated",
                        "list": {
                            "list_id": manifest.list_id,
                            "list_revision": manifest.list_revision,
                        },
                        "task": task,
                    }))?
                }
                _ => {}
            }
        }

        for (task_id, task) in &previous {
            if !current.contains_key(task_id) {
                json_line(&json!({
                    "type": "task_deleted",
                    "list": {
                        "list_id": manifest.list_id,
                        "list_revision": manifest.list_revision,
                    },
                    "task_id": task_id,
                    "task": task,
                }))?;
            }
        }

        if manifest.list_revision != previous_manifest_revision && current == previous {
            json_line(&json!({
                "type": "snapshot",
                "list": {
                    "list_id": manifest.list_id,
                    "list_revision": manifest.list_revision,
                },
                "tasks": task_map_json(&tasks),
            }))?;
        }

        previous = current;
        previous_manifest_revision = manifest.list_revision;
        if maybe_watcher.is_none() {
            event_driven = false;
        }
    }
}
