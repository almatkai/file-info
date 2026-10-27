use crate::{
    info::{self, FileInfo},
    secure::{self, Selected},
    tools, Result, ID, MAX_PATH, MAX_SELECTION,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};
use tezbar_extension_sdk::{
    BoxFuture, CommandConcurrency, CoreResult, Extension, ExtensionContext,
};

#[derive(Clone)]
struct Action {
    title: String,
    method: &'static str,
    params: Value,
    kind: &'static str,
    key: Option<(&'static str, bool)>,
}
#[derive(Default)]
struct Live {
    info: Option<FileInfo>,
    actions: HashMap<String, Action>,
    order: Vec<String>,
    unknown: Option<Action>,
    inflight: Option<Action>,
    apps: Option<Vec<(String, String)>>,
    report: Vec<Value>,
    message: String,
    done: bool,
    cancel_probe: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
}
#[derive(Default)]
struct Registry {
    sessions: HashMap<String, Arc<Mutex<Live>>>,
    retired: VecDeque<(String, Arc<Mutex<Live>>)>,
    spent: std::collections::HashSet<String>,
}
#[derive(Default)]
pub struct Adapter {
    registry: Mutex<Registry>,
    engine: tokio::sync::Mutex<()>,
}
fn token() -> String {
    uuid::Uuid::new_v4().to_string()
}
struct OpenGuard<'a> {
    owner: &'a Adapter,
    id: String,
    accepted: bool,
}
impl Drop for OpenGuard<'_> {
    fn drop(&mut self) {
        if !self.accepted {
            self.owner.retire(&self.id);
        }
    }
}
struct EffectGuard {
    live: Arc<Mutex<Live>>,
    finished: bool,
}
impl Drop for EffectGuard {
    fn drop(&mut self) {
        if !self.finished {
            let mut live = self.live.lock().unwrap();
            live.unknown = live.inflight.take();
            live.message="Host effect outcome is unknown after cancellation. Manual retry uses the frozen exact payload and may duplicate the effect.".into();
            live.actions.clear();
            live.order.clear();
        }
    }
}
impl Adapter {
    fn prune(&self) {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let dead: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .sessions
            .iter()
            .filter_map(|(id, live)| {
                let mut live = live.lock().unwrap();
                live.cancel_probe.as_mut().and_then(|probe| {
                    if probe.as_mut().poll(&mut context).is_ready() {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        for id in dead {
            self.retire(&id);
        }
    }
    fn retire(&self, id: &str) {
        let mut registry = self.registry.lock().unwrap();
        if let Some(live) = registry.sessions.remove(id) {
            {
                let mut state = live.lock().unwrap();
                for record in &mut state.report {
                    if record["state"] == "preparing" {
                        record["state"] = json!("unknown");
                        record["detail"]=json!("Operation ended before a definitive result. Inspect the frozen identity; do not retry automatically.");
                    }
                }
                state.cancel_probe = None;
                state.info = None;
                state.actions.clear();
                state.order.clear();
                state.apps = None;
                state.unknown = None;
                state.inflight = None;
                state.message.clear();
            }
            // Retain the same checkpoint object: a synchronous commit finishing
            // concurrently with cancellation can still publish its known result.
            registry.retired.push_back((id.into(), live));
            while registry.retired.len() > 32
                || registry
                    .retired
                    .iter()
                    .map(|(_, r)| {
                        serde_json::to_vec(&r.lock().unwrap().report)
                            .unwrap_or_default()
                            .len()
                    })
                    .sum::<usize>()
                    > 8 * 1024 * 1024
            {
                registry.retired.pop_front();
            }
        }
    }
    fn begin(&self, id: &str) -> Result<(Arc<Mutex<Live>>, OpenGuard<'_>)> {
        if id.is_empty() || id.len() > 256 {
            return Err("Invalid session identity".into());
        }
        let mut registry = self.registry.lock().unwrap();
        if let Some((_, report)) = registry.retired.iter().find(|(old, _)| old == id) {
            return Err(format!("Session identity consumed; no replay or selection reacquisition. Frozen outcomes: {}",serde_json::to_string(&report.lock().unwrap().report).map_err(|e|e.to_string())?));
        }
        if registry.spent.contains(id) {
            return Err(
                "Session identity is consumed; a cancelled or completed clear is never replayed"
                    .into(),
            );
        }
        if registry.sessions.len() >= 128 {
            return Err("File Info session capacity reached; dispose existing sessions".into());
        }
        if registry.spent.len() >= 4096 {
            return Err("Process session-identity budget (4096) reached; a fresh host process is required. No operation was replayed.".into());
        }
        registry.spent.insert(id.to_string());
        let live = Arc::new(Mutex::new(Live::default()));
        registry.sessions.insert(id.into(), live.clone());
        Ok((
            live,
            OpenGuard {
                owner: self,
                id: id.into(),
                accepted: false,
            },
        ))
    }
    fn current(&self, id: &str) -> Option<Arc<Mutex<Live>>> {
        self.registry.lock().unwrap().sessions.get(id).cloned()
    }
    async fn run(&self, ctx: &ExtensionContext, command: &str, id: &str) -> CoreResult {
        self.prune();
        let (live, mut guard) = self.begin(id)?;
        let context = ctx.clone();
        live.lock().unwrap().cancel_probe =
            Some(Box::pin(async move { context.cancelled().await }));
        let result = async {
            let snapshot = ctx.host("host.selection.read", json!({"kind": "files"})).await?;
            let selection = selection(&snapshot)?;
            if selection.is_empty() {
                let message = if command == "show-file-info" {
                    "No file selected. Please select a file in Finder first."
                } else {
                    "No files selected. Select a file in Finder to clear metadata"
                };
                return Ok(no_view(message, false, &[]));
            }
            if command == "show-file-info" {
                let selected = Selected::open(&selection[0], false)?;
                let data = info::get(&selected).await?;
                let mut state = live.lock().unwrap();
                state.info = Some(data);
                normal_actions(&mut state);
                return Ok(render(&mut state));
            }
            // One batch owns file buffers/helpers. Waiting batches have only the
            // original selection strings, never a reacquired snapshot.
            let _engine = self.engine.lock().await;
            let mut plan: Vec<_> = selection.iter().map(|p| Selected::open(p, true)).collect();
            live.lock().unwrap().report = selection.iter().zip(&plan).map(|(path, selected)| {
                let mut record = json!({"path": path, "state": "not-started"});
                if let Ok(file) = selected {
                    match file.frozen_identity() {
                        Ok(identity) => record["identity"] = identity,
                        Err(error) => record["identityError"] = json!(error),
                    }
                }
                record
            }).collect();
            let tool = tools::exiftool().await?;
            let noun = if selection.len() == 1 { "file" } else { "files" };
            let reply = ctx.host("host.dialog.confirm", json!({
                "title": "Clear File Metadata", "type": "warning", "buttons": ["Cancel", "Clear"],
                "message": format!("This will permanently remove embedded metadata and system attributes from the selected {noun}. Native ExifTool writes a private copy; encoded content is additionally checked for covered layouts. Other layouts trust the native writer; full metadata removal is not guaranteed. PDF metadata may remain recoverable in prior revisions. ICC profiles and orientation tags may be removed, as in V1, so appearance can change. A cancelled batch may already have replaced some files; never retry automatically.")
            })).await?;
            if !confirmed(&reply)? {
                return Ok(no_view("Cancelled; no files were changed", true, &[]));
            }
            for (index, entry) in plan.iter_mut().enumerate() {
                live.lock().unwrap().report[index]["state"] = json!("preparing");
                let outcome = match entry {
                    Ok(selected) => tools::clear(selected, &tool, ctx).await,
                    Err(error) => Err(error.clone()),
                };
                // No await between a committed replacement and its checkpoint.
                // This lives outside SDK staged state and remains shared on retire.
                let mut checkpoint = live.lock().unwrap();
                let record = &mut checkpoint.report[index];
                match outcome {
                    Ok(outcome) => {
                        record["state"] = json!(if outcome.complete { "committed" } else { "partial" });
                        record["committed"] = json!(true);
                        record["detail"] = json!(outcome.detail);
                    }
                    Err(error) => {
                        record["state"] = json!("failed");
                        record["committed"] = json!(false);
                        record["error"] = json!(error);
                    }
                }
            }
            let mut state = live.lock().unwrap();
            state.done = true;
            let success = state.report.iter().filter(|r| r["state"] == "committed").count();
            let partial = state.report.iter().filter(|r| r["state"] == "partial").count();
            let failed = state.report.len() - success - partial;
            let complete = failed == 0 && partial == 0;
            let mut message = if complete {
                format!("Metadata cleared. Successfully cleared {success} {}", if success == 1 { "file" } else { "files" })
            } else {
                format!("{}: Cleared {success}, Partial {partial}, Failed {failed}", if success + partial == 0 { "Failed" } else { "Partially completed" })
            };
            for record in &state.report {
                let detail = record["detail"].as_str().or_else(|| record["error"].as_str()).unwrap_or("Unknown outcome");
                message.push_str(&format!("\n{}: {detail}", record["path"].as_str().unwrap_or("")));
            }
            Ok(no_view(&message, complete, &state.report))
        }.await;
        if result.is_ok() {
            guard.accepted = true;
        }
        result
    }
    async fn action(&self, ctx: &ExtensionContext, id: &str, action_id: &str) -> CoreResult {
        let Some(live) = self.current(id) else {
            return Err("Action session expired; no operation was performed".into());
        };
        let action = {
            let mut state = live.lock().unwrap();
            let Some(action) = state.actions.remove(action_id) else {
                if state.unknown.is_some() {
                    retry_action(&mut state);
                    return Ok(render(&mut state));
                }
                // The pinned renderer action union has view/no-view/error. A
                // structured error keeps a stale action inert without a false
                // copied notice or an undefined-root unchanged response.
                return Err("Action expired; no operation was performed".into());
            };
            state.actions.clear();
            state.order.clear();
            state.unknown = None;
            state.inflight = Some(action.clone());
            action
        };
        // Action IDs are consumed outside SDK staged state, BEFORE any await.
        let mut guard = EffectGuard {
            live: live.clone(),
            finished: false,
        };
        if action.method == "local.back" {
            let mut state = live.lock().unwrap();
            state.apps = None;
            state.inflight = None;
            state.message.clear();
            normal_actions(&mut state);
            guard.finished = true;
            return Ok(render(&mut state));
        }
        let result = ctx.host(action.method, action.params.clone()).await;
        let mut state = live.lock().unwrap();
        if action.method == "host.apps.list" {
            match result
                .and_then(|v| apps(&v, state.info.as_ref().map(|i| i.path.len()).unwrap_or(0)))
            {
                Ok(apps) => {
                    state.apps = Some(apps);
                    state.inflight = None;
                    state.message.clear();
                    app_actions(&mut state);
                }
                Err(error) => {
                    state.inflight = None;
                    state.unknown = Some(action);
                    state.message =
                        format!("Could not load applications: {error}. Retry explicitly.");
                    retry_action(&mut state);
                }
            }
        } else {
            match result.and_then(|v| ack(&v)) {
                Ok(()) => {
                    state.inflight = None;
                    state.unknown = None;
                    state.message = format!("{} completed", action.title);
                    if state.apps.is_some() {
                        app_actions(&mut state);
                    } else {
                        normal_actions(&mut state);
                    }
                }
                Err(error) => {
                    state.inflight = None;
                    state.unknown = Some(action);
                    state.message=format!("Host effect outcome unknown: {error}. Manual retry may duplicate it and uses the frozen exact bytes/path.");
                    retry_action(&mut state);
                }
            }
        }
        guard.finished = true;
        Ok(render(&mut state))
    }
}
impl Extension for Adapter {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        &["show-file-info", "clear-file-metadata"]
    }
    fn concurrency(&self, _: &str) -> CommandConcurrency {
        CommandConcurrency::RejectWhileBusy
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        payload: &'a Value,
        _state: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            let id = payload["sessionId"]
                .as_str()
                .ok_or("Missing canonical session identity")?;
            match operation {
                "extension:run-command" => self.run(ctx, command, id).await,
                "extension:dispose-session" => {
                    self.retire(id);
                    Ok(tezbar_extension_sdk::unchanged())
                }
                "extension:invoke-action" if command == "show-file-info" => {
                    self.action(ctx, id, payload["actionId"].as_str().unwrap_or(""))
                        .await
                }
                "extension:refresh-session" => {
                    if let Some(live) = self.current(id) {
                        let mut live = live.lock().unwrap();
                        if live.inflight.is_some() {
                            return Ok(tezbar_extension_sdk::unchanged());
                        }
                        if live.unknown.is_some() {
                            retry_action(&mut live);
                        }
                        if live.info.is_some() {
                            return Ok(render(&mut live));
                        }
                    }
                    Ok(tezbar_extension_sdk::unchanged())
                }
                // V1 lists filter locally; no mutable query is sent back to overwrite
                // renderer input after debounce, stale file reads or refresh.
                "extension:search-text-changed" => {
                    if payload
                        .get("searchText")
                        .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > crate::MAX_QUERY))
                    {
                        return Err(
                            "Search text must be a string of at most 4096 UTF-8 bytes".into()
                        );
                    }
                    Ok(tezbar_extension_sdk::unchanged())
                }
                "extension:load-more" => Ok(tezbar_extension_sdk::unchanged()),
                _ => Err("Unsupported File Info operation".into()),
            }
        })
    }
}
pub fn selection(value: &Value) -> Result<Vec<String>> {
    let array = value
        .as_array()
        .ok_or("File selection must be an array of absolute strings")?;
    if array.len() > MAX_SELECTION {
        return Err("File selection exceeds 128 files; no truncation was applied".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    let mut total = 0;
    for entry in array {
        let path = entry
            .as_str()
            .ok_or("File selection contains a non-string path")?;
        secure::validate_path(path)?;
        total += path.len();
        if total > 1024 * 1024 {
            return Err("File selection exceeds 1 MiB".into());
        }
        if !seen.insert(path) {
            return Err("Duplicate selected path; no file was processed twice".into());
        }
        result.push(path.into());
    }
    Ok(result)
}
pub fn ack(v: &Value) -> Result<()> {
    let o = v
        .as_object()
        .ok_or("Expected closed {ok:true,message?:string} acknowledgement")?;
    if o.get("ok") != Some(&Value::Bool(true))
        || o.keys().any(|k| k != "ok" && k != "message")
        || o.get("message")
            .is_some_and(|v| !v.is_string() || v.as_str().unwrap().len() > 4096)
    {
        return Err("Invalid or contradictory host acknowledgement".into());
    }
    Ok(())
}
pub fn confirmed(v: &Value) -> Result<bool> {
    let o = v
        .as_object()
        .ok_or("Expected confirmation {response:0|1}")?;
    if o.len() != 1 {
        return Err("Invalid confirmation fields".into());
    }
    match o.get("response").and_then(Value::as_i64) {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err("Invalid confirmation response".into()),
    }
}
fn apps(v: &Value, file_len: usize) -> Result<Vec<(String, String)>> {
    let values = v.as_array().ok_or("Expected application array")?;
    if values.len() > 2048 {
        return Err("Application list exceeds 2048 entries".into());
    }
    let mut total = 0;
    let mut out = Vec::new();
    for value in values {
        let name = value["name"].as_str().ok_or("Application name missing")?;
        let path = value["path"].as_str().ok_or("Application path missing")?;
        if name.is_empty()
            || name.len() > 512
            || path.is_empty()
            || path.len() > MAX_PATH
            || path.contains('\0')
        {
            return Err("Invalid application entry".into());
        }
        total += name.len() + path.len() + file_len;
        if total > 1024 * 1024 {
            return Err("Application list exceeds 1 MiB".into());
        }
        out.push((name.into(), path.into()));
    }
    Ok(out)
}
fn add_action(live: &mut Live, action: Action) -> String {
    let id = token();
    live.order.push(id.clone());
    live.actions.insert(id.clone(), action);
    id
}
fn action(
    title: &str,
    method: &'static str,
    params: Value,
    kind: &'static str,
    key: Option<(&'static str, bool)>,
) -> Action {
    Action {
        title: title.into(),
        method,
        params,
        kind,
        key,
    }
}
fn normal_actions(live: &mut Live) {
    live.actions.clear();
    live.order.clear();
    let Some(info) = live.info.clone() else {
        return;
    };
    add_action(
        live,
        action(
            "Copy File Path",
            "host.clipboard.write",
            json!({"text":info.path}),
            "copy",
            Some(("c", false)),
        ),
    );
    add_action(
        live,
        action(
            "Show in Finder",
            "host.shell.reveal",
            json!({"path":info.path}),
            "show-in-finder",
            Some(("f", false)),
        ),
    );
    add_action(
        live,
        action(
            "Open With",
            "host.apps.list",
            json!({}),
            "push",
            Some(("o", false)),
        ),
    );
    add_action(
        live,
        action(
            "Copy All Info",
            "host.clipboard.write",
            json!({"text":info.copy_all()}),
            "copy",
            Some(("c", true)),
        ),
    );
}
fn app_actions(live: &mut Live) {
    live.actions.clear();
    live.order.clear();
    let (Some(info), Some(apps)) = (live.info.clone(), live.apps.clone()) else {
        return;
    };
    add_action(
        live,
        action("Back to File Info", "local.back", json!({}), "pop", None),
    );
    for (name, path) in apps {
        add_action(
            live,
            action(
                &format!("Open With {name}"),
                "host.shell.openWithApp",
                json!({"path":info.path,"appPath":path}),
                "open",
                None,
            ),
        );
    }
}
fn retry_action(live: &mut Live) {
    live.actions.clear();
    live.order.clear();
    if let Some(mut a) = live.unknown.clone() {
        a.key = None;
        add_action(live, a);
    }
}
fn node(ty: &str, props: Value, children: Vec<Value>) -> Value {
    json!({"type":ty,"props":props,"children":children})
}
fn label(title: &str, text: &str) -> Value {
    node(
        "List.Item.Detail.Metadata.Label",
        json!({"title":title,"text":text}),
        vec![],
    )
}
fn separator() -> Value {
    node("List.Item.Detail.Metadata.Separator", json!({}), vec![])
}
fn stat(title: &str, text: &str, icon: &str, color: &str) -> Value {
    node(
        "List.Item",
        json!({"id":title,"title":title,"icon":{"source":icon,"tintColor":color},"accessories":[{"text":text}],"actionIds":[]}),
        vec![],
    )
}
fn render(live: &mut Live) -> Value {
    let actions:Vec<_>=live.order.iter().filter_map(|id|live.actions.get(id).map(|a|{
        let mut v=json!({"id":id,"title":if live.unknown.is_some(){format!("Retry {} (may duplicate)",a.title)}else{a.title.clone()},"kind":a.kind});if let Some((key,shift))=a.key{v["shortcut"]=json!({"key":key,"modifiers":if shift{vec!["cmd","shift"]}else{vec!["cmd"]}});}v
    })).collect();
    let Some(info) = live.info.as_ref() else {
        return tezbar_extension_sdk::unchanged();
    };
    let root = if let Some(apps) = &live.apps {
        if live.unknown.is_some() {
            let rows=actions.first().map(|a|vec![node("List.Item",json!({"id":"retry","title":a["title"],"subtitle":"Outcome unknown. This explicit retry may duplicate the effect.","actionIds":[a["id"]]}),vec![])]).unwrap_or_default();
            // Escape uses normal navigation; it can never be bound to Retry.
            node("List", json!({"navigationTitle":"Open With"}), rows)
        } else {
            let back = live
                .order
                .iter()
                .find(|id| {
                    live.actions
                        .get(*id)
                        .is_some_and(|a| a.method == "local.back")
                })
                .cloned()
                .unwrap_or_default();
            let rows = apps
                .iter()
                .zip(live.order.iter().filter(|id| {
                    live.actions
                        .get(*id)
                        .is_some_and(|a| a.method == "host.shell.openWithApp")
                }))
                .map(|((name, path), id)| {
                    node(
                        "List.Item",
                        json!({"id":id,"title":name,"subtitle":path,"actionIds":[id]}),
                        vec![],
                    )
                })
                .collect();
            node(
                "List",
                json!({"navigationTitle":"Open With","backActionId":back}),
                rows,
            )
        }
    } else {
        let mut fields = vec![
            label("General Information", ""),
            label("Name", &info.name),
            label("Type", info.file_type()),
            label(
                "Size",
                &format!(
                    "{} ({} bytes)",
                    info::bytes(info.size),
                    info::grouped(info.size)
                ),
            ),
            label("Extension", &info.extension),
        ];
        if !info.mime_type.is_empty() {
            fields.push(label("MIME Type", &info.mime_type));
        }
        if !info.dimensions.is_empty() {
            fields.push(label("Dimensions", &info.dimensions));
        }
        if !info.duration.is_empty() {
            fields.push(label("Duration", &info.duration));
        }
        fields.extend([
            separator(),
            label("Location", ""),
            label("Path", &info.path),
            separator(),
            label("Timestamps", ""),
            label("Created", &info.created),
            label("Modified", &info.modified),
            label("Accessed", &info.accessed),
            separator(),
            label("Permissions", ""),
            label("Mode", &info.permissions),
        ]);
        if !info.owner.is_empty() {
            fields.push(label("Owner", &info.owner));
        }
        if !info.group.is_empty() {
            fields.push(label("Group", &info.group));
        }
        let metadata = node("List.Item.Detail.Metadata", json!({}), fields);
        let detail = node("List.Item.Detail", json!({}), vec![metadata]);
        let overview = node(
            "List.Item",
            json!({"id":"overview","title":"File Info","subtitle":info.name,"icon":{"source":info.icon(),"tintColor":"#007AFF"},"actionIds":live.order}),
            vec![detail],
        );
        let mut stats = vec![
            stat("Size", &info::bytes(info.size), "HardDrive", "#FF9500"),
            stat("Type", info.file_type(), "Document", "#AF52DE"),
        ];
        if !info.dimensions.is_empty() {
            stats.push(stat("Dimensions", &info.dimensions, "Image", "#FF2D55"));
        }
        if !info.duration.is_empty() {
            stats.push(stat("Duration", &info.duration, "PlayFilled", "#007AFF"));
        }
        stats.push(stat("Modified", &info.modified, "Clock", "#34C759"));
        stats.push(stat("Permissions", &info.permissions, "Lock", "#FF3B30"));
        node(
            "List",
            json!({"isShowingDetail":true}),
            vec![
                node(
                    "List.Section",
                    json!({"title":"File Overview"}),
                    vec![overview],
                ),
                node("List.Section", json!({"title":"Quick Stats"}), stats),
            ],
        )
    };
    let mut v = tezbar_extension_sdk::view("Show File Info", root, actions);
    v["refreshMs"] = Value::Null;
    if live.unknown.is_some() {
        v["effects"] = json!([{"kind":"toast","style":"failure","title":"Outcome unknown","message":live.message}]);
    }
    if !live.message.is_empty() {
        v["message"] = json!(live.message);
    }
    v
}
fn no_view(message: &str, success: bool, report: &[Value]) -> Value {
    let mut v = tezbar_extension_sdk::no_view(message);
    v["results"] = json!(report);
    v["effects"] = json!([{"kind":"toast","style":if success{"success"}else{"failure"},"title":if success{"File Info"}else{"Error"},"message":message}]);
    v
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_services() {
        for v in [
            Value::Null,
            json!(true),
            json!({"ok":true,"error":"bad"}),
            json!({"ok":false}),
            json!({"ok":true,"message":null}),
            json!({"success":true}),
        ] {
            assert!(ack(&v).is_err());
        }
        assert!(ack(&json!({"ok":true})).is_ok());
        assert!(ack(&json!({"ok":true,"message":"Copied"})).is_ok());
        for v in [
            json!(true),
            json!({"response":true}),
            json!({"response":2}),
            json!({"response":1,"ok":true}),
        ] {
            assert!(confirmed(&v).is_err());
        }
        assert_eq!(confirmed(&json!({"response":1})), Ok(true));
    }
    #[test]
    fn selection_shape() {
        for v in [
            json!("/file"),
            json!([{"path":"/file"}]),
            json!(["relative"]),
            json!(["/a/../b"]),
            json!(["/a", "/a"]),
        ] {
            assert!(selection(&v).is_err());
        }
        assert!(selection(&json!([])).unwrap().is_empty());
    }
    #[test]
    fn canceled_open_capacity() {
        let adapter = Adapter::default();
        for n in 0..140 {
            let id = format!("cancel-{n}");
            let (_live, _guard) = adapter.begin(&id).unwrap();
        }
        assert!(adapter.registry.lock().unwrap().sessions.is_empty());
        assert!(adapter.begin("new").is_ok());
    }
}
#[cfg(test)]
mod actual_source_view {
    use super::*;
    #[test]
    fn full_metadata_order_and_copy_all_match_actual_v1_tsx() {
        let oracle: Value =
            serde_json::from_str(include_str!("../fixtures/v1-oracle.json")).unwrap();
        let i = &oracle["info"];
        let text = |k: &str| i[k].as_str().unwrap().to_string();
        let label_text = |title: &str| {
            oracle["labels"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["title"] == title)
                .unwrap()["text"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let info = FileInfo {
            name: text("name"),
            is_directory: false,
            path: text("path"),
            size: i["size"].as_u64().unwrap(),
            extension: text("extension"),
            created: label_text("Created"),
            modified: label_text("Modified"),
            accessed: label_text("Accessed"),
            permissions: text("permissions"),
            owner: text("owner"),
            group: text("group"),
            mime_type: text("mimeType"),
            dimensions: text("dimensions"),
            duration: text("duration"),
        };
        assert_eq!(
            info.copy_all(),
            oracle["copy"][1]["content"].as_str().unwrap()
        );
        let mut live = Live {
            info: Some(info),
            ..Default::default()
        };
        normal_actions(&mut live);
        let view = render(&mut live);
        let fields = view["root"]["children"][0]["children"][0]["children"][0]["children"][0]
            ["children"]
            .as_array()
            .unwrap();
        let labels: Vec<_> = fields
            .iter()
            .filter(|v| v["type"] == "List.Item.Detail.Metadata.Label")
            .map(|v| v["props"].clone())
            .collect();
        assert_eq!(json!(labels), oracle["labels"]);
        assert_eq!(view["actions"].as_array().unwrap().len(), 4);
    }
    #[test]
    fn failed_open_cleanup_probe_prunes_late_cancel_without_eviction() {
        let adapter = Adapter::default();
        for n in 0..128 {
            let (live, mut guard) = adapter.begin(&format!("late-{n}")).unwrap();
            live.lock().unwrap().cancel_probe = Some(Box::pin(std::future::ready(())));
            guard.accepted = true;
        }
        adapter.prune();
        assert!(adapter.registry.lock().unwrap().sessions.is_empty());
        assert!(adapter.begin("after-late-cancel").is_ok());
    }
}
