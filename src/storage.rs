use fs4::fs_std::FileExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use crate::cli::OutputFormat;
use crate::error::TkError;
use crate::model::{
    Diagnostic, DiagnosticLevel, Manifest, TaskRecord, TaskStatus, TaskView, Visibility,
    detect_cycle, make_task_view, now_rfc3339, numeric_id, parse_timestamp,
    sanitize_default_list_id, validate_list_id, validate_owner, validate_task_record,
};

#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    pub default_list_id: Option<String>,
    pub default_owner: Option<String>,
    pub output_format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPaths {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub lists_dir: PathBuf,
    pub list_id: String,
    pub list_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub tasks_dir: PathBuf,
    pub lock_path: PathBuf,
    pub highwatermark_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolvedContext {
    pub paths: ResolvedPaths,
    pub default_owner: Option<String>,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub struct CreateTaskInput {
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub visibility: Visibility,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateTaskInput {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
    pub clear_owner: bool,
    pub visibility: Option<Visibility>,
    pub set_metadata: BTreeMap<String, Value>,
    pub unset_metadata: Vec<String>,
    pub if_revision: Option<u64>,
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct ListFilters {
    pub statuses: BTreeSet<TaskStatus>,
    pub owner: Option<String>,
    pub unowned: bool,
    pub claimable: bool,
    pub include_internal: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ListState {
    pub manifest: Manifest,
    pub tasks: BTreeMap<String, TaskRecord>,
    pub highwatermark: u64,
}

pub struct ListLock {
    file: File,
}

impl ListLock {
    fn acquire(paths: &ResolvedPaths) -> Result<Self, TkError> {
        fs::create_dir_all(&paths.list_dir)
            .map_err(|err| TkError::storage(format!("failed to create list directory: {err}")))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&paths.lock_path)
            .map_err(|err| TkError::storage(format!("failed to open lock file: {err}")))?;
        file.lock_exclusive()
            .map_err(|err| TkError::storage(format!("failed to lock list: {err}")))?;
        Ok(Self { file })
    }
}

impl Drop for ListLock {
    fn drop(&mut self) {
        let _ = fs4::fs_std::FileExt::unlock(&self.file);
    }
}

pub fn resolve_context(
    root_arg: Option<&str>,
    list_arg: Option<&str>,
    format_arg: Option<OutputFormat>,
) -> Result<ResolvedContext, TkError> {
    let cwd = std::env::current_dir()
        .map_err(|err| TkError::storage(format!("failed to read current directory: {err}")))?;
    let root = match root_arg {
        Some(path) => PathBuf::from(path),
        None => match std::env::var("TK_ROOT") {
            Ok(path) => PathBuf::from(path),
            Err(_) => detect_vcs_root(&cwd)
                .map(|root| root.join(".tk"))
                .unwrap_or_else(|| cwd.join(".tk")),
        },
    };
    let config_path = root.join("config.toml");
    let config = read_config_file(&config_path)?;
    let list_id = match list_arg
        .map(str::to_owned)
        .or_else(|| std::env::var("TK_LIST_ID").ok())
        .or_else(|| config.default_list_id.clone())
        .or_else(|| {
            detect_vcs_root(&cwd)
                .and_then(|root| {
                    root.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                })
                .map(|name| sanitize_default_list_id(&name))
        }) {
        Some(value) => value,
        None => "default".to_owned(),
    };
    validate_list_id(&list_id)?;

    let output_format = match format_arg {
        Some(format) => format,
        None => parse_output_format(config.output_format.as_deref())?.unwrap_or(OutputFormat::Text),
    };

    let default_owner = match std::env::var("TK_OWNER").ok() {
        Some(owner) => Some(owner),
        None => config.default_owner.clone(),
    };
    if let Some(owner) = &default_owner {
        validate_owner(owner)?;
    }

    let lists_dir = root.join("lists");
    let list_dir = lists_dir.join(&list_id);
    let tasks_dir = list_dir.join("tasks");
    let manifest_path = list_dir.join("manifest.json");
    let lock_path = list_dir.join(".lock");
    let highwatermark_path = list_dir.join(".highwatermark");

    Ok(ResolvedContext {
        default_owner,
        output_format,
        paths: ResolvedPaths {
            root,
            config_path,
            lists_dir,
            list_id,
            list_dir,
            manifest_path,
            tasks_dir,
            lock_path,
            highwatermark_path,
        },
    })
}

pub fn read_config_file(path: &Path) -> Result<ConfigFile, TkError> {
    match fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents)
            .map_err(|err| TkError::validation(format!("invalid config.toml: {err}"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(err) => Err(TkError::storage(format!(
            "failed to read config file {}: {err}",
            path.display()
        ))),
    }
}

fn parse_output_format(value: Option<&str>) -> Result<Option<OutputFormat>, TkError> {
    match value {
        None => Ok(None),
        Some("text") => Ok(Some(OutputFormat::Text)),
        Some("json") => Ok(Some(OutputFormat::Json)),
        Some("ndjson") => Ok(Some(OutputFormat::Ndjson)),
        Some(other) => Err(TkError::validation(format!(
            "unsupported output format in config: {other}"
        ))),
    }
}

fn detect_vcs_root(start: &Path) -> Option<PathBuf> {
    for path in start.ancestors() {
        if path.join(".git").exists() || path.join(".jj").exists() {
            return Some(path.to_path_buf());
        }
    }
    None
}

fn read_manifest(path: &Path) -> Result<Option<Manifest>, TkError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let manifest: Manifest = serde_json::from_str(&contents).map_err(|err| {
                TkError::validation(format!(
                    "failed to parse manifest {}: {err}",
                    path.display()
                ))
            })?;
            Ok(Some(manifest))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(TkError::storage(format!(
            "failed to read manifest {}: {err}",
            path.display()
        ))),
    }
}

fn write_manifest(path: &Path, manifest: &Manifest) -> Result<(), TkError> {
    atomic_write_json(path, manifest)
}

fn read_highwatermark(path: &Path) -> Result<u64, TkError> {
    match fs::read_to_string(path) {
        Ok(contents) => contents.trim().parse::<u64>().map_err(|err| {
            TkError::validation(format!("invalid highwatermark {}: {err}", path.display()))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(TkError::storage(format!(
            "failed to read highwatermark {}: {err}",
            path.display()
        ))),
    }
}

fn write_highwatermark(path: &Path, value: u64) -> Result<(), TkError> {
    atomic_write_string(path, &value.to_string())
}

fn read_task_file(path: &Path) -> Result<TaskRecord, TkError> {
    let contents = fs::read_to_string(path)
        .map_err(|err| TkError::storage(format!("failed to read {}: {err}", path.display())))?;
    let task: TaskRecord = serde_json::from_str(&contents)
        .map_err(|err| TkError::validation(format!("failed to parse {}: {err}", path.display())))?;
    validate_task_record(&task)?;
    Ok(task)
}

fn write_task_file(path: &Path, task: &TaskRecord) -> Result<(), TkError> {
    validate_task_record(task)?;
    atomic_write_json(path, task)
}

fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), TkError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| TkError::storage(format!("failed to serialize JSON: {err}")))?;
    atomic_write_bytes(path, &bytes)
}

fn atomic_write_string(path: &Path, value: &str) -> Result<(), TkError> {
    atomic_write_bytes(path, value.as_bytes())
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), TkError> {
    let dir = path
        .parent()
        .ok_or_else(|| TkError::storage(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(dir)
        .map_err(|err| TkError::storage(format!("failed to create {}: {err}", dir.display())))?;
    let mut file = NamedTempFile::new_in(dir)
        .map_err(|err| TkError::storage(format!("failed to create temp file: {err}")))?;
    file.write_all(bytes)
        .map_err(|err| TkError::storage(format!("failed to write temp file: {err}")))?;
    file.flush()
        .map_err(|err| TkError::storage(format!("failed to flush temp file: {err}")))?;
    file.as_file()
        .sync_all()
        .map_err(|err| TkError::storage(format!("failed to sync temp file: {err}")))?;
    file.persist(path)
        .map_err(|err| TkError::storage(format!("failed to persist {}: {err}", path.display())))?;
    Ok(())
}

fn read_tasks(paths: &ResolvedPaths) -> Result<BTreeMap<String, TaskRecord>, TkError> {
    let mut tasks = BTreeMap::new();
    let entries = match fs::read_dir(&paths.tasks_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(tasks),
        Err(err) => {
            return Err(TkError::storage(format!(
                "failed to read tasks directory {}: {err}",
                paths.tasks_dir.display()
            )));
        }
    };

    for entry in entries {
        let entry = entry
            .map_err(|err| TkError::storage(format!("failed to read directory entry: {err}")))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let task = read_task_file(&path)?;
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if task.id != file_stem {
            return Err(TkError::validation(format!(
                "task filename {} does not match payload id {}",
                file_stem, task.id
            )));
        }
        if tasks.insert(task.id.clone(), task).is_some() {
            return Err(TkError::validation(format!(
                "duplicate task ID detected in {}",
                paths.tasks_dir.display()
            )));
        }
    }

    Ok(tasks)
}

fn default_manifest(paths: &ResolvedPaths) -> Manifest {
    Manifest::new(paths.list_id.clone(), None, None)
}

fn load_state(paths: &ResolvedPaths) -> Result<ListState, TkError> {
    let manifest = match read_manifest(&paths.manifest_path)? {
        Some(manifest) => manifest,
        None => default_manifest(paths),
    };
    let tasks = read_tasks(paths)?;
    let highwatermark = read_highwatermark(&paths.highwatermark_path)?;
    Ok(ListState {
        manifest,
        tasks,
        highwatermark,
    })
}

fn ensure_manifest(
    paths: &ResolvedPaths,
    title: Option<String>,
    description: Option<String>,
) -> Result<Manifest, TkError> {
    fs::create_dir_all(&paths.tasks_dir)
        .map_err(|err| TkError::storage(format!("failed to create tasks directory: {err}")))?;
    match read_manifest(&paths.manifest_path)? {
        Some(manifest) => Ok(manifest),
        None => {
            let manifest = Manifest::new(paths.list_id.clone(), title, description);
            write_manifest(&paths.manifest_path, &manifest)?;
            write_highwatermark(&paths.highwatermark_path, 0)?;
            Ok(manifest)
        }
    }
}

fn persist_state(
    paths: &ResolvedPaths,
    before: &ListState,
    after: &ListState,
) -> Result<(), TkError> {
    fs::create_dir_all(&paths.tasks_dir)
        .map_err(|err| TkError::storage(format!("failed to create tasks directory: {err}")))?;

    for task in after.tasks.values() {
        let path = paths.tasks_dir.join(format!("{}.json", task.id));
        write_task_file(&path, task)?;
    }

    for task_id in before.tasks.keys() {
        if !after.tasks.contains_key(task_id) {
            let path = paths.tasks_dir.join(format!("{task_id}.json"));
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(TkError::storage(format!(
                        "failed to delete {}: {err}",
                        path.display()
                    )));
                }
            }
        }
    }

    write_highwatermark(&paths.highwatermark_path, after.highwatermark)?;
    write_manifest(&paths.manifest_path, &after.manifest)?;
    Ok(())
}

fn bump_manifest(manifest: &mut Manifest) {
    manifest.list_revision += 1;
    manifest.updated_at = now_rfc3339();
}

fn bump_task(task: &mut TaskRecord) {
    task.revision += 1;
    task.updated_at = now_rfc3339();
}

fn apply_revision_guard(task: &TaskRecord, if_revision: Option<u64>) -> Result<(), TkError> {
    if let Some(expected) = if_revision
        && task.revision != expected
    {
        return Err(TkError::with_details(
            crate::error::ErrorCode::Conflict,
            format!("task #{} revision conflict", task.id),
            json!({
                "code": "revision_conflict",
                "task_id": task.id,
                "expected_revision": expected,
                "actual_revision": task.revision,
            }),
        ));
    }
    Ok(())
}

fn ensure_status_transition(
    current: TaskStatus,
    next: TaskStatus,
    force: bool,
) -> Result<(), TkError> {
    if current == next {
        return Ok(());
    }
    match (current, next) {
        (TaskStatus::Pending, TaskStatus::InProgress)
        | (TaskStatus::InProgress, TaskStatus::Pending)
        | (TaskStatus::InProgress, TaskStatus::Completed) => Ok(()),
        (TaskStatus::Pending, TaskStatus::Completed) if force => Ok(()),
        (TaskStatus::Completed, TaskStatus::Pending) if force => Ok(()),
        (TaskStatus::Completed, TaskStatus::InProgress) => Err(TkError::conflict(
            "completed task cannot transition directly to in_progress; reopen it first",
        )),
        _ => Err(TkError::conflict(format!(
            "invalid status transition: {} -> {}",
            current.as_str(),
            next.as_str()
        ))),
    }
}

fn apply_status_update(
    task: &mut TaskRecord,
    next: TaskStatus,
    force: bool,
) -> Result<bool, TkError> {
    ensure_status_transition(task.status, next, force)?;
    if task.status == next {
        return Ok(false);
    }
    let now = now_rfc3339();
    task.status = next;
    match next {
        TaskStatus::Pending => {
            task.completed_at = None;
        }
        TaskStatus::InProgress => {
            if task.started_at.is_none() {
                task.started_at = Some(now.clone());
            }
            task.completed_at = None;
        }
        TaskStatus::Completed => {
            if task.started_at.is_none() {
                task.started_at = Some(now.clone());
            }
            task.completed_at = Some(now);
        }
    }
    Ok(true)
}

fn ensure_task_exists<'a>(
    tasks: &'a BTreeMap<String, TaskRecord>,
    task_id: &str,
) -> Result<&'a TaskRecord, TkError> {
    tasks
        .get(task_id)
        .ok_or_else(|| TkError::not_found(format!("task #{task_id} not found")))
}

fn ensure_task_exists_mut<'a>(
    tasks: &'a mut BTreeMap<String, TaskRecord>,
    task_id: &str,
) -> Result<&'a mut TaskRecord, TkError> {
    tasks
        .get_mut(task_id)
        .ok_or_else(|| TkError::not_found(format!("task #{task_id} not found")))
}

pub struct ListStore {
    pub paths: ResolvedPaths,
}

impl ListStore {
    pub fn new(paths: ResolvedPaths) -> Self {
        Self { paths }
    }

    pub fn init(
        &self,
        title: Option<String>,
        description: Option<String>,
    ) -> Result<Manifest, TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, title, description)
    }

    pub fn dir_info(&self) -> crate::output::DirInfo {
        crate::output::DirInfo {
            root: self.paths.root.clone(),
            config_path: self.paths.config_path.clone(),
            lists_dir: self.paths.lists_dir.clone(),
            list_dir: self.paths.list_dir.clone(),
            manifest_path: self.paths.manifest_path.clone(),
            tasks_dir: self.paths.tasks_dir.clone(),
            lock_path: self.paths.lock_path.clone(),
            highwatermark_path: self.paths.highwatermark_path.clone(),
        }
    }

    pub fn ensure_storage_dirs(&self) -> Result<(), TkError> {
        fs::create_dir_all(&self.paths.tasks_dir)
            .map_err(|err| TkError::storage(format!("failed to create tasks directory: {err}")))?;
        Ok(())
    }

    pub fn list_task_views(
        &self,
        filters: &ListFilters,
    ) -> Result<(Manifest, Vec<TaskView>), TkError> {
        let state = load_state(&self.paths)?;
        let mut tasks = state
            .tasks
            .values()
            .filter(|task| filters.include_internal || task.visibility == Visibility::Public)
            .map(|task| make_task_view(task, &state.tasks))
            .filter(|task| {
                (filters.statuses.is_empty() || filters.statuses.contains(&task.task.status))
                    && filters
                        .owner
                        .as_ref()
                        .map(|owner| task.task.owner.as_ref() == Some(owner))
                        .unwrap_or(true)
                    && (!filters.unowned || task.task.owner.is_none())
                    && (!filters.claimable || task.claimable)
            })
            .collect::<Vec<_>>();
        tasks.sort_by_key(|task| numeric_id(&task.task.id));
        if let Some(limit) = filters.limit {
            tasks.truncate(limit);
        }
        Ok((state.manifest, tasks))
    }

    pub fn get_task_view(&self, task_id: &str) -> Result<(Manifest, TaskView), TkError> {
        let state = load_state(&self.paths)?;
        let task = ensure_task_exists(&state.tasks, task_id)?;
        Ok((state.manifest, make_task_view(task, &state.tasks)))
    }

    pub fn create_task(&self, input: CreateTaskInput) -> Result<(Manifest, TaskView), TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        let manifest = ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        after.manifest = manifest;
        validate_owner_option(input.owner.as_ref())?;

        let next_id = after
            .tasks
            .keys()
            .map(|id| numeric_id(id))
            .max()
            .unwrap_or(0)
            .max(after.highwatermark)
            + 1;
        let id = next_id.to_string();
        let task = TaskRecord::new(
            id.clone(),
            input.subject,
            input.description,
            input.active_form,
            input.owner,
            input.visibility,
            input.metadata,
        );
        validate_task_record(&task)?;
        after.tasks.insert(id.clone(), task);
        after.highwatermark = next_id;
        bump_manifest(&mut after.manifest);
        persist_state(&self.paths, &before, &after)?;
        let created = after.tasks.get(&id).expect("created task must exist");
        Ok((after.manifest, make_task_view(created, &after.tasks)))
    }

    pub fn update_task(
        &self,
        task_id: &str,
        input: UpdateTaskInput,
    ) -> Result<(Manifest, TaskView, Vec<String>), TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        let mut updated_fields = Vec::new();
        {
            let task = ensure_task_exists_mut(&mut after.tasks, task_id)?;
            apply_revision_guard(task, input.if_revision)?;
            if let Some(subject) = input.subject
                && task.subject != subject
            {
                task.subject = subject;
                updated_fields.push("subject".to_owned());
            }
            if let Some(description) = input.description
                && task.description != description
            {
                task.description = description;
                updated_fields.push("description".to_owned());
            }
            if let Some(active_form) = input.active_form
                && task.active_form.as_ref() != Some(&active_form)
            {
                task.active_form = Some(active_form);
                updated_fields.push("active_form".to_owned());
            }
            if let Some(visibility) = input.visibility
                && task.visibility != visibility
            {
                task.visibility = visibility;
                updated_fields.push("visibility".to_owned());
            }
            if input.clear_owner && task.owner.is_some() {
                task.owner = None;
                updated_fields.push("owner".to_owned());
            }
            if let Some(owner) = input.owner {
                validate_owner(&owner)?;
                if task.owner.as_ref() != Some(&owner) {
                    task.owner = Some(owner);
                    updated_fields.push("owner".to_owned());
                }
            }
            if !input.set_metadata.is_empty() || !input.unset_metadata.is_empty() {
                for (key, value) in input.set_metadata {
                    task.metadata.insert(key, value);
                }
                for key in input.unset_metadata {
                    task.metadata.remove(&key);
                }
                updated_fields.push("metadata".to_owned());
            }
            if let Some(status) = input.status
                && apply_status_update(task, status, input.force)?
            {
                updated_fields.push("status".to_owned());
            }
            if !updated_fields.is_empty() {
                bump_task(task);
            }
        }
        if !updated_fields.is_empty() {
            bump_manifest(&mut after.manifest);
            persist_state(&self.paths, &before, &after)?;
        }
        let task = ensure_task_exists(&after.tasks, task_id)?;
        Ok((
            after.manifest,
            make_task_view(task, &after.tasks),
            updated_fields,
        ))
    }

    pub fn claim_task(
        &self,
        task_id: &str,
        owner: &str,
        start: bool,
        check_busy: bool,
        if_revision: Option<u64>,
    ) -> Result<(Manifest, TaskView), TkError> {
        validate_owner(owner)?;
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        let mut changed = false;

        if check_busy {
            let busy = after.tasks.values().any(|task| {
                task.owner.as_deref() == Some(owner)
                    && task.id != task_id
                    && task.status != TaskStatus::Completed
            });
            if busy {
                return Err(TkError::with_details(
                    crate::error::ErrorCode::Conflict,
                    format!("owner {owner} already has unresolved tasks"),
                    json!({
                        "code": "agent_busy",
                        "owner": owner,
                    }),
                ));
            }
        }

        {
            let current_snapshot = after.tasks.clone();
            let task = ensure_task_exists_mut(&mut after.tasks, task_id)?;
            apply_revision_guard(task, if_revision)?;

            if let Some(existing_owner) = &task.owner {
                if existing_owner != owner {
                    return Err(TkError::with_details(
                        crate::error::ErrorCode::Conflict,
                        format!("task #{task_id} is already owned by {existing_owner}"),
                        json!({
                            "code": "already_owned",
                            "task_id": task_id,
                            "owner": existing_owner,
                        }),
                    ));
                }
            } else {
                let view = make_task_view(task, &current_snapshot);
                if !view.claimable {
                    return Err(TkError::with_details(
                        crate::error::ErrorCode::Conflict,
                        format!("task #{task_id} is not claimable"),
                        json!({
                            "code": "blocked",
                            "task_id": task_id,
                            "open_blocked_by": view.open_blocked_by,
                            "invalid_blocked_by": view.invalid_blocked_by,
                        }),
                    ));
                }
                task.owner = Some(owner.to_owned());
                changed = true;
            }

            if start && task.status == TaskStatus::Pending {
                apply_status_update(task, TaskStatus::InProgress, false)?;
                changed = true;
            }

            if changed {
                bump_task(task);
            }
        }

        if changed {
            bump_manifest(&mut after.manifest);
            persist_state(&self.paths, &before, &after)?;
        }
        let task = ensure_task_exists(&after.tasks, task_id)?;
        Ok((after.manifest, make_task_view(task, &after.tasks)))
    }

    pub fn unclaim_task(
        &self,
        task_id: &str,
        requeue: bool,
        if_revision: Option<u64>,
    ) -> Result<(Manifest, TaskView), TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        {
            let task = ensure_task_exists_mut(&mut after.tasks, task_id)?;
            apply_revision_guard(task, if_revision)?;
            let mut changed = false;
            if task.owner.take().is_some() {
                changed = true;
            }
            if requeue && task.status != TaskStatus::Pending {
                apply_status_update(task, TaskStatus::Pending, true)?;
                changed = true;
            }
            if changed {
                bump_task(task);
                bump_manifest(&mut after.manifest);
                persist_state(&self.paths, &before, &after)?;
            }
        }
        let task = ensure_task_exists(&after.tasks, task_id)?;
        Ok((after.manifest, make_task_view(task, &after.tasks)))
    }

    pub fn next_task(
        &self,
        claim: bool,
        owner: Option<&str>,
        start: bool,
        check_busy: bool,
    ) -> Result<(Manifest, TaskView), TkError> {
        if claim && owner.is_none() {
            return Err(TkError::usage("--owner is required when --claim is used"));
        }
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        let mut task_ids = after.tasks.keys().cloned().collect::<Vec<_>>();
        task_ids.sort_by_key(|id| numeric_id(id));
        let selected_id = task_ids
            .into_iter()
            .find(|id| {
                after
                    .tasks
                    .get(id)
                    .map(|task| make_task_view(task, &after.tasks).claimable)
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                TkError::with_details(
                    crate::error::ErrorCode::Conflict,
                    "no claimable task available",
                    json!({ "code": "no_available_task" }),
                )
            })?;

        if !claim {
            let task = ensure_task_exists(&after.tasks, &selected_id)?;
            return Ok((after.manifest, make_task_view(task, &after.tasks)));
        }

        let owner = owner.expect("owner must be set when claim is true");
        validate_owner(owner)?;
        if check_busy {
            let busy = after.tasks.values().any(|task| {
                task.owner.as_deref() == Some(owner) && task.status != TaskStatus::Completed
            });
            if busy {
                return Err(TkError::with_details(
                    crate::error::ErrorCode::Conflict,
                    format!("owner {owner} already has unresolved tasks"),
                    json!({
                        "code": "agent_busy",
                        "owner": owner,
                    }),
                ));
            }
        }

        {
            let task = ensure_task_exists_mut(&mut after.tasks, &selected_id)?;
            task.owner = Some(owner.to_owned());
            if start {
                apply_status_update(task, TaskStatus::InProgress, false)?;
            }
            bump_task(task);
        }
        bump_manifest(&mut after.manifest);
        persist_state(&self.paths, &before, &after)?;
        let task = ensure_task_exists(&after.tasks, &selected_id)?;
        Ok((after.manifest, make_task_view(task, &after.tasks)))
    }

    pub fn add_blockers(
        &self,
        task_id: &str,
        blocker_ids: &[String],
    ) -> Result<(Manifest, TaskView), TkError> {
        if blocker_ids.is_empty() {
            return Err(TkError::usage("at least one blocker ID is required"));
        }
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        ensure_task_exists(&after.tasks, task_id)?;
        for blocker_id in blocker_ids {
            if blocker_id == task_id {
                return Err(TkError::conflict("task cannot block itself"));
            }
            ensure_task_exists(&after.tasks, blocker_id)?;
        }

        let mut touched = BTreeSet::new();
        for blocker_id in blocker_ids {
            {
                let task = ensure_task_exists_mut(&mut after.tasks, task_id)?;
                if !task.blocked_by.iter().any(|id| id == blocker_id) {
                    task.blocked_by.push(blocker_id.clone());
                    touched.insert(task.id.clone());
                }
            }
            {
                let blocker = ensure_task_exists_mut(&mut after.tasks, blocker_id)?;
                if !blocker.blocks.iter().any(|id| id == task_id) {
                    blocker.blocks.push(task_id.to_owned());
                    touched.insert(blocker.id.clone());
                }
            }
        }
        if let Some(cycle) = detect_cycle(&after.tasks) {
            return Err(TkError::with_details(
                crate::error::ErrorCode::Conflict,
                "dependency cycle detected",
                json!({
                    "code": "cycle_detected",
                    "cycle": cycle,
                }),
            ));
        }
        for task_id in &touched {
            let task = ensure_task_exists_mut(&mut after.tasks, task_id)?;
            task.blocks.sort_by_key(|id| numeric_id(id));
            task.blocks.dedup();
            task.blocked_by.sort_by_key(|id| numeric_id(id));
            task.blocked_by.dedup();
            bump_task(task);
        }
        if !touched.is_empty() {
            bump_manifest(&mut after.manifest);
            persist_state(&self.paths, &before, &after)?;
        }
        let task = ensure_task_exists(&after.tasks, task_id)?;
        Ok((after.manifest, make_task_view(task, &after.tasks)))
    }

    pub fn remove_blockers(
        &self,
        task_id: &str,
        blocker_ids: &[String],
    ) -> Result<(Manifest, TaskView), TkError> {
        if blocker_ids.is_empty() {
            return Err(TkError::usage("at least one blocker ID is required"));
        }
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        ensure_task_exists(&after.tasks, task_id)?;

        let mut touched = BTreeSet::new();
        for blocker_id in blocker_ids {
            if let Some(task) = after.tasks.get_mut(task_id) {
                let before_len = task.blocked_by.len();
                task.blocked_by.retain(|id| id != blocker_id);
                if task.blocked_by.len() != before_len {
                    touched.insert(task.id.clone());
                }
            }
            if let Some(blocker) = after.tasks.get_mut(blocker_id) {
                let before_len = blocker.blocks.len();
                blocker.blocks.retain(|id| id != task_id);
                if blocker.blocks.len() != before_len {
                    touched.insert(blocker.id.clone());
                }
            }
        }
        for task_id in touched {
            let task = ensure_task_exists_mut(&mut after.tasks, &task_id)?;
            bump_task(task);
        }
        if before.tasks != after.tasks {
            bump_manifest(&mut after.manifest);
            persist_state(&self.paths, &before, &after)?;
        }
        let task = ensure_task_exists(&after.tasks, task_id)?;
        Ok((after.manifest, make_task_view(task, &after.tasks)))
    }

    pub fn delete_task(
        &self,
        task_id: &str,
        detach: bool,
        if_revision: Option<u64>,
    ) -> Result<Manifest, TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        let mut after = before.clone();
        let current = ensure_task_exists(&after.tasks, task_id)?.clone();
        apply_revision_guard(&current, if_revision)?;

        let has_edges = !current.blocks.is_empty() || !current.blocked_by.is_empty();
        if has_edges && !detach {
            return Err(TkError::conflict(
                "task participates in dependency edges; use --detach to delete it",
            ));
        }
        if detach {
            let mut touched = BTreeSet::new();
            for blocker_id in &current.blocked_by {
                if let Some(blocker) = after.tasks.get_mut(blocker_id) {
                    blocker.blocks.retain(|id| id != task_id);
                    touched.insert(blocker.id.clone());
                }
            }
            for blocked_id in &current.blocks {
                if let Some(blocked) = after.tasks.get_mut(blocked_id) {
                    blocked.blocked_by.retain(|id| id != task_id);
                    touched.insert(blocked.id.clone());
                }
            }
            for task_id in touched {
                let task = ensure_task_exists_mut(&mut after.tasks, &task_id)?;
                bump_task(task);
            }
        }
        after.tasks.remove(task_id);
        after.highwatermark = after.highwatermark.max(numeric_id(task_id));
        bump_manifest(&mut after.manifest);
        persist_state(&self.paths, &before, &after)?;
        Ok(after.manifest)
    }

    pub fn reset(&self, force: bool) -> Result<Manifest, TkError> {
        let _lock = ListLock::acquire(&self.paths)?;
        ensure_manifest(&self.paths, None, None)?;
        let before = load_state(&self.paths)?;
        if !force
            && before
                .tasks
                .values()
                .any(|task| task.status != TaskStatus::Completed)
        {
            return Err(TkError::conflict(
                "cannot reset list while unresolved tasks exist; use --force",
            ));
        }
        let mut after = before.clone();
        after.tasks.clear();
        bump_manifest(&mut after.manifest);
        persist_state(&self.paths, &before, &after)?;
        Ok(after.manifest)
    }

    pub fn verify(&self) -> Result<(Option<Manifest>, Vec<Diagnostic>), TkError> {
        let mut diagnostics = Vec::new();
        let manifest = match fs::read_to_string(&self.paths.manifest_path) {
            Ok(contents) => match serde_json::from_str::<Manifest>(&contents) {
                Ok(manifest) => {
                    if manifest.schema_version != crate::model::SCHEMA_VERSION {
                        diagnostics.push(Diagnostic {
                            level: DiagnosticLevel::Error,
                            code: "manifest_schema_version".to_owned(),
                            message: format!(
                                "unsupported manifest schema version {}",
                                manifest.schema_version
                            ),
                            task_id: None,
                        });
                    }
                    if parse_timestamp(&manifest.created_at).is_err()
                        || parse_timestamp(&manifest.updated_at).is_err()
                    {
                        diagnostics.push(Diagnostic {
                            level: DiagnosticLevel::Error,
                            code: "manifest_timestamp_invalid".to_owned(),
                            message: "manifest timestamp is invalid".to_owned(),
                            task_id: None,
                        });
                    }
                    Some(manifest)
                }
                Err(err) => {
                    diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        code: "manifest_parse_error".to_owned(),
                        message: err.to_string(),
                        task_id: None,
                    });
                    None
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                diagnostics.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    code: "manifest_missing".to_owned(),
                    message: "manifest.json is missing".to_owned(),
                    task_id: None,
                });
                None
            }
            Err(err) => {
                return Err(TkError::storage(format!(
                    "failed to read manifest {}: {err}",
                    self.paths.manifest_path.display()
                )));
            }
        };

        let mut tasks = BTreeMap::new();
        let mut seen_ids = BTreeSet::new();
        if let Ok(entries) = fs::read_dir(&self.paths.tasks_dir) {
            for entry in entries {
                let entry = entry.map_err(|err| {
                    TkError::storage(format!("failed to read directory entry: {err}"))
                })?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let file_id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default();
                match fs::read_to_string(&path) {
                    Ok(contents) => match serde_json::from_str::<TaskRecord>(&contents) {
                        Ok(task) => {
                            if task.id != file_id {
                                diagnostics.push(Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    code: "filename_id_mismatch".to_owned(),
                                    message: format!(
                                        "filename {} does not match task id {}",
                                        file_id, task.id
                                    ),
                                    task_id: Some(task.id.clone()),
                                });
                            }
                            if let Err(err) = validate_task_record(&task) {
                                diagnostics.push(Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    code: "task_validation_error".to_owned(),
                                    message: err.message,
                                    task_id: Some(task.id.clone()),
                                });
                            }
                            if !seen_ids.insert(task.id.clone()) {
                                diagnostics.push(Diagnostic {
                                    level: DiagnosticLevel::Error,
                                    code: "duplicate_task_id".to_owned(),
                                    message: "duplicate task id detected".to_owned(),
                                    task_id: Some(task.id.clone()),
                                });
                            }
                            tasks.insert(task.id.clone(), task);
                        }
                        Err(err) => diagnostics.push(Diagnostic {
                            level: DiagnosticLevel::Error,
                            code: "task_parse_error".to_owned(),
                            message: err.to_string(),
                            task_id: Some(file_id.to_owned()),
                        }),
                    },
                    Err(err) => diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        code: "task_read_error".to_owned(),
                        message: err.to_string(),
                        task_id: Some(file_id.to_owned()),
                    }),
                }
            }
        }

        for task in tasks.values() {
            for blocker_id in &task.blocked_by {
                match tasks.get(blocker_id) {
                    Some(blocker) => {
                        if !blocker.blocks.iter().any(|id| id == &task.id) {
                            diagnostics.push(Diagnostic {
                                level: DiagnosticLevel::Error,
                                code: "edge_asymmetry".to_owned(),
                                message: format!(
                                    "task #{} lists blocker #{} but reverse edge is missing",
                                    task.id, blocker_id
                                ),
                                task_id: Some(task.id.clone()),
                            });
                        }
                    }
                    None => diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        code: "blocker_missing".to_owned(),
                        message: format!("task references missing blocker #{blocker_id}"),
                        task_id: Some(task.id.clone()),
                    }),
                }
            }
            for blocked_id in &task.blocks {
                match tasks.get(blocked_id) {
                    Some(blocked) => {
                        if !blocked.blocked_by.iter().any(|id| id == &task.id) {
                            diagnostics.push(Diagnostic {
                                level: DiagnosticLevel::Error,
                                code: "edge_asymmetry".to_owned(),
                                message: format!(
                                    "task #{} blocks #{} but reverse edge is missing",
                                    task.id, blocked_id
                                ),
                                task_id: Some(task.id.clone()),
                            });
                        }
                    }
                    None => diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        code: "blocked_task_missing".to_owned(),
                        message: format!("task references missing blocked task #{blocked_id}"),
                        task_id: Some(task.id.clone()),
                    }),
                }
            }
        }

        if let Some(cycle) = detect_cycle(&tasks) {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Error,
                code: "cycle_detected".to_owned(),
                message: format!("dependency cycle detected: {}", cycle.join(" -> ")),
                task_id: cycle.first().cloned(),
            });
        }

        diagnostics.sort_by(|a, b| a.code.cmp(&b.code).then(a.task_id.cmp(&b.task_id)));
        Ok((manifest, diagnostics))
    }
}

fn validate_owner_option(owner: Option<&String>) -> Result<(), TkError> {
    if let Some(owner) = owner {
        validate_owner(owner)?;
    }
    Ok(())
}
