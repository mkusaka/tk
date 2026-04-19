use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::cli::OutputFormat;
use crate::error::TkError;
use crate::model::{Diagnostic, Manifest, TaskView, numeric_id};

#[derive(Debug, Clone)]
pub struct DirInfo {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub lists_dir: PathBuf,
    pub list_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub tasks_dir: PathBuf,
    pub lock_path: PathBuf,
    pub highwatermark_path: PathBuf,
}

pub enum CommandOutput {
    Json(Value),
    Text(String),
}

pub fn print_success(
    output: CommandOutput,
    _format: OutputFormat,
    quiet: bool,
) -> Result<(), TkError> {
    match output {
        CommandOutput::Json(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value)
                    .map_err(|err| TkError::storage(format!("failed to render JSON: {err}")))?,
            );
        }
        CommandOutput::Text(text) => {
            if !quiet {
                println!("{text}");
            }
        }
    }
    Ok(())
}

pub fn print_error(err: &TkError, format: OutputFormat) {
    match format {
        OutputFormat::Json | OutputFormat::Ndjson => {
            let value = json!({
                "ok": false,
                "error": {
                    "code": err.code.as_str(),
                    "message": err.message,
                    "details": err.details,
                }
            });
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
                    format!(
                        "{{\"ok\":false,\"error\":{{\"code\":\"{}\",\"message\":\"{}\"}}}}",
                        err.code.as_str(),
                        err.message
                    )
                })
            );
        }
        OutputFormat::Text => {
            eprintln!("{}", err.message);
        }
    }
}

pub fn envelope(command: &str, list: &Manifest, payload: Value) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("ok".to_owned(), Value::Bool(true));
    object.insert("command".to_owned(), Value::String(command.to_owned()));
    object.insert(
        "list".to_owned(),
        json!({
            "list_id": list.list_id,
            "list_revision": list.list_revision,
        }),
    );
    if let Value::Object(extra) = payload {
        for (key, value) in extra {
            object.insert(key, value);
        }
    }
    Value::Object(object)
}

pub fn text_for_dir(info: &DirInfo) -> String {
    format!(
        "root: {}\nconfig: {}\nlists: {}\nlist: {}\nmanifest: {}\ntasks: {}\nlock: {}\nhighwatermark: {}",
        info.root.display(),
        info.config_path.display(),
        info.lists_dir.display(),
        info.list_dir.display(),
        info.manifest_path.display(),
        info.tasks_dir.display(),
        info.lock_path.display(),
        info.highwatermark_path.display()
    )
}

pub fn text_for_list(tasks: &[TaskView]) -> String {
    if tasks.is_empty() {
        return "No tasks found".to_owned();
    }

    let mut ordered = tasks.to_vec();
    ordered.sort_by_key(|task| numeric_id(&task.task.id));
    ordered
        .into_iter()
        .map(|task| {
            let owner = task
                .task
                .owner
                .as_ref()
                .map(|owner| format!(" ({owner})"))
                .unwrap_or_default();
            let blocked = if task.open_blocked_by.is_empty() {
                String::new()
            } else {
                format!(
                    " [blocked by {}]",
                    task.open_blocked_by
                        .iter()
                        .map(|id| format!("#{id}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!(
                "#{} [{}] {}{}{}",
                task.task.id,
                task.task.status.as_str(),
                task.task.subject,
                owner,
                blocked
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn text_for_task(task: &TaskView) -> String {
    let mut lines = vec![
        format!("Task #{}: {}", task.task.id, task.task.subject),
        format!("Status: {}", task.task.status.as_str()),
        format!("Visibility: {}", task.task.visibility.as_str()),
        format!("Description: {}", task.task.description),
    ];
    if let Some(owner) = &task.task.owner {
        lines.push(format!("Owner: {owner}"));
    }
    if !task.task.blocks.is_empty() {
        lines.push(format!(
            "Blocks: {}",
            task.task
                .blocks
                .iter()
                .map(|id| format!("#{id}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !task.task.blocked_by.is_empty() {
        lines.push(format!(
            "Blocked by: {}",
            task.task
                .blocked_by
                .iter()
                .map(|id| format!("#{id}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !task.invalid_blocked_by.is_empty() {
        lines.push(format!(
            "Invalid blockers: {}",
            task.invalid_blocked_by
                .iter()
                .map(|id| format!("#{id}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !task.task.metadata.is_empty() {
        lines.push(format!(
            "Metadata: {}",
            serde_json::to_string_pretty(&task.task.metadata).unwrap_or_else(|_| "{}".to_owned())
        ));
    }
    lines.join("\n")
}

pub fn text_for_verify(diagnostics: &[Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return "No issues found".to_owned();
    }
    diagnostics
        .iter()
        .map(|diag| match &diag.task_id {
            Some(task_id) => format!(
                "[{:?}] {} (task #{task_id}): {}",
                diag.level, diag.code, diag.message
            ),
            None => format!("[{:?}] {}: {}", diag.level, diag.code, diag.message),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn task_map_json(tasks: &[TaskView]) -> Value {
    Value::Array(
        tasks
            .iter()
            .cloned()
            .map(|task| serde_json::to_value(task).expect("task view must serialize"))
            .collect(),
    )
}

pub fn task_view_map(tasks: &[TaskView]) -> BTreeMap<String, TaskView> {
    tasks
        .iter()
        .cloned()
        .map(|task| (task.task.id.clone(), task))
        .collect()
}

pub fn json_line<T: Serialize>(value: &T) -> Result<(), TkError> {
    println!(
        "{}",
        serde_json::to_string(value)
            .map_err(|err| TkError::storage(format!("failed to render NDJSON: {err}")))?
    );
    Ok(())
}
