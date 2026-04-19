use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::error::TkError;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Internal,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub list_id: String,
    pub title: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub list_revision: u64,
}

impl Manifest {
    pub fn new(list_id: String, title: Option<String>, description: Option<String>) -> Self {
        let now = now_rfc3339();
        Self {
            schema_version: SCHEMA_VERSION,
            title: title.unwrap_or_else(|| list_id.clone()),
            description,
            list_id,
            created_at: now.clone(),
            updated_at: now,
            list_revision: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub status: TaskStatus,
    pub visibility: Visibility,
    pub owner: Option<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl TaskRecord {
    pub fn new(
        id: String,
        subject: String,
        description: String,
        active_form: Option<String>,
        owner: Option<String>,
        visibility: Visibility,
        metadata: BTreeMap<String, Value>,
    ) -> Self {
        let now = now_rfc3339();
        Self {
            schema_version: SCHEMA_VERSION,
            id,
            revision: 1,
            subject,
            description,
            active_form,
            status: TaskStatus::Pending,
            visibility,
            owner,
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            metadata,
            created_at: now.clone(),
            updated_at: now,
            started_at: None,
            completed_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskView {
    #[serde(flatten)]
    pub task: TaskRecord,
    pub open_blocked_by: Vec<String>,
    pub invalid_blocked_by: Vec<String>,
    pub claimable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Error,
    Warning,
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("rfc3339 timestamp formatting must succeed")
}

pub fn parse_timestamp(value: &str) -> Result<OffsetDateTime, TkError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| TkError::validation(format!("invalid timestamp: {value}")))
}

pub fn validate_list_id(value: &str) -> Result<(), TkError> {
    if value.is_empty() || value.len() > 128 {
        return Err(TkError::validation(
            "list ID must be between 1 and 128 characters",
        ));
    }

    let mut chars = value.chars();
    let first = chars
        .next()
        .ok_or_else(|| TkError::validation("list ID must not be empty"))?;
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(TkError::validation(
            "list ID must start with a lowercase ASCII letter or digit",
        ));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(TkError::validation(
            "list ID may contain only lowercase ASCII letters, digits, '.', '_' or '-'",
        ));
    }

    Ok(())
}

pub fn sanitize_default_list_id(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
        } else if ch.is_ascii_uppercase() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_owned();
    if out.is_empty() {
        "default".to_owned()
    } else {
        out
    }
}

pub fn validate_owner(value: &str) -> Result<(), TkError> {
    if value.is_empty() || value.len() > 128 {
        return Err(TkError::validation(
            "owner must be between 1 and 128 characters",
        ));
    }
    Ok(())
}

pub fn validate_task_record(task: &TaskRecord) -> Result<(), TkError> {
    if task.schema_version != SCHEMA_VERSION {
        return Err(TkError::validation(format!(
            "unsupported task schema version: {}",
            task.schema_version
        )));
    }
    if task.subject.is_empty() || task.subject.chars().count() > 200 {
        return Err(TkError::validation(format!(
            "task #{} subject must be between 1 and 200 characters",
            task.id
        )));
    }
    if task.description.len() > 32_768 {
        return Err(TkError::validation(format!(
            "task #{} description exceeds 32768 bytes",
            task.id
        )));
    }
    if let Some(active_form) = &task.active_form {
        if active_form.chars().count() > 120 {
            return Err(TkError::validation(format!(
                "task #{} active_form exceeds 120 characters",
                task.id
            )));
        }
    }
    if let Some(owner) = &task.owner {
        validate_owner(owner)?;
    }
    if serde_json::to_vec(&task.metadata)
        .map_err(|err| TkError::validation(format!("invalid metadata: {err}")))?
        .len()
        > 65_536
    {
        return Err(TkError::validation(format!(
            "task #{} metadata exceeds 65536 bytes",
            task.id
        )));
    }
    parse_timestamp(&task.created_at)?;
    parse_timestamp(&task.updated_at)?;
    if let Some(started_at) = &task.started_at {
        parse_timestamp(started_at)?;
    }
    if let Some(completed_at) = &task.completed_at {
        parse_timestamp(completed_at)?;
    }
    if task.blocks.iter().any(|id| id == &task.id)
        || task.blocked_by.iter().any(|id| id == &task.id)
    {
        return Err(TkError::validation(format!(
            "task #{} cannot depend on itself",
            task.id
        )));
    }

    Ok(())
}

pub fn make_task_view(task: &TaskRecord, tasks: &BTreeMap<String, TaskRecord>) -> TaskView {
    let mut open_blocked_by = Vec::new();
    let mut invalid_blocked_by = Vec::new();

    for blocker_id in &task.blocked_by {
        match tasks.get(blocker_id) {
            Some(blocker) if blocker.status != TaskStatus::Completed => {
                open_blocked_by.push(blocker_id.clone())
            }
            Some(_) => {}
            None => invalid_blocked_by.push(blocker_id.clone()),
        }
    }

    let claimable = task.status == TaskStatus::Pending
        && task.owner.is_none()
        && open_blocked_by.is_empty()
        && invalid_blocked_by.is_empty();

    TaskView {
        task: task.clone(),
        open_blocked_by,
        invalid_blocked_by,
        claimable,
    }
}

pub fn numeric_id(value: &str) -> u64 {
    value.parse::<u64>().unwrap_or(u64::MAX)
}

pub fn detect_cycle(tasks: &BTreeMap<String, TaskRecord>) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        tasks: &BTreeMap<String, TaskRecord>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if visiting.contains(node) {
            let idx = stack.iter().position(|entry| entry == node).unwrap_or(0);
            return Some(stack[idx..].to_vec());
        }
        if !visited.insert(node.to_owned()) {
            return None;
        }

        visiting.insert(node.to_owned());
        stack.push(node.to_owned());
        if let Some(task) = tasks.get(node) {
            for next in &task.blocks {
                if let Some(cycle) = visit(next, tasks, visiting, visited, stack) {
                    return Some(cycle);
                }
            }
        }
        stack.pop();
        visiting.remove(node);
        None
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = Vec::new();
    for key in tasks.keys() {
        if let Some(cycle) = visit(key, tasks, &mut visiting, &mut visited, &mut stack) {
            return Some(cycle);
        }
    }
    None
}
