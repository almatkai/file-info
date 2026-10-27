//! Actual pinned native selection producer, with process symbols replaced before
//! evaluation. OS commands cannot execute. Native fs checks see only QA fixtures.
#![cfg(target_os = "macos")]
use serde_json::{json, Value};
use std::os::unix::process::ExitStatusExt;
use std::{io::Cursor, path::PathBuf, sync::Mutex, time::Duration};
type CoreResult = Result<Value, String>;
#[derive(Default)]
struct Scenario {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    calls: Vec<(String, Vec<String>)>,
}
static SCENARIO: Mutex<Scenario> = Mutex::new(Scenario {
    stdout: vec![],
    stderr: vec![],
    calls: vec![],
});
mod tokio {
    pub use ::tokio::{io, pin, runtime, time, try_join};
    pub mod process {
        pub use crate::FakeCommand as Command;
    }
}
mod reqwest {
    pub use url::Url;
}
pub struct FakeCommand {
    program: String,
    args: Vec<String>,
}
impl FakeCommand {
    fn new(program: &str) -> Self {
        Self {
            program: program.into(),
            args: vec![],
        }
    }
    fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.args.extend(
            args.into_iter()
                .map(|s| s.as_ref().to_string_lossy().into_owned()),
        );
        self
    }
    fn stdin(&mut self, _: std::process::Stdio) -> &mut Self {
        self
    }
    fn stdout(&mut self, _: std::process::Stdio) -> &mut Self {
        self
    }
    fn stderr(&mut self, _: std::process::Stdio) -> &mut Self {
        self
    }
    fn kill_on_drop(&mut self, _: bool) -> &mut Self {
        self
    }
    async fn output(&mut self) -> std::io::Result<std::process::Output> {
        let mut s = SCENARIO.lock().unwrap();
        s.calls.push((self.program.clone(), self.args.clone()));
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: s.stdout.clone(),
            stderr: s.stderr.clone(),
        })
    }
    fn spawn(&mut self) -> std::io::Result<FakeChild> {
        let mut s = SCENARIO.lock().unwrap();
        s.calls.push((self.program.clone(), self.args.clone()));
        Ok(FakeChild {
            stdout: Some(Cursor::new(s.stdout.clone())),
            stderr: Some(Cursor::new(s.stderr.clone())),
        })
    }
}
struct FakeChild {
    stdout: Option<Cursor<Vec<u8>>>,
    stderr: Option<Cursor<Vec<u8>>>,
}
impl FakeChild {
    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        Ok(std::process::ExitStatus::from_raw(0))
    }
}
#[path = "../fixtures/native-selection.rs"]
mod producer;
fn set_selection_output(value: Value) {
    SCENARIO.lock().unwrap().stdout = serde_json::to_vec(&value).unwrap();
}
#[::tokio::test]
async fn actual_native_selected_files_producer_and_unapplied_patch() {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".qa/host-fixtures")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("owned unicode\nфайл ; $().txt");
    std::fs::write(&p, b"generated QA").unwrap();
    let url = url::Url::from_file_path(&p).unwrap().to_string();
    set_selection_output(json!([url]));
    assert_eq!(
        producer::original::selected_files().await.unwrap(),
        json!([p])
    );
    assert_eq!(
        producer::patched::selected_files().await.unwrap(),
        json!([p])
    );
    set_selection_output(json!([url::Url::from_file_path(&d).unwrap().to_string()]));
    assert_eq!(
        producer::original::selected_files().await.unwrap(),
        json!([])
    );
    assert_eq!(
        producer::patched::selected_files().await.unwrap(),
        json!([d])
    );
    // The actual patched producer preserves directory-first ordering, including
    // both empty and nonempty owned directories. Commands own clear policy.
    let empty = d.join("empty");
    std::fs::create_dir(&empty).unwrap();
    for folder in [&empty, &d] {
        set_selection_output(json!([
            url::Url::from_file_path(folder).unwrap().to_string(),
            url
        ]));
        assert_eq!(
            producer::original::selected_files().await.unwrap(),
            json!([p])
        );
        assert_eq!(
            producer::patched::selected_files().await.unwrap(),
            json!([folder, p])
        );
    }
    set_selection_output(json!(vec![url.clone(); 129]));
    assert!(producer::patched::selected_files().await.is_err());
    for value in [
        json!([42]),
        json!(["https://example.invalid/no-local-file"]),
        json!(["not a URL"]),
    ] {
        set_selection_output(value);
        assert!(producer::patched::selected_files().await.is_err());
    }
    SCENARIO.lock().unwrap().stdout = vec![b'x'; 1024 * 1024 + 1];
    assert!(producer::patched::selected_files()
        .await
        .unwrap_err()
        .contains("bounds"));
    assert!(SCENARIO
        .lock()
        .unwrap()
        .calls
        .iter()
        .all(|(program, _)| program == "/usr/bin/osascript"));
}
struct ClipboardFake {
    written: Mutex<Vec<String>>,
    fail: bool,
}
impl ClipboardFake {
    fn write_text(&self, text: String) -> Result<(), String> {
        if self.fail {
            return Err("owned clipboard failure".into());
        }
        self.written.lock().unwrap().push(text);
        Ok(())
    }
}
struct DialogFake {
    accepted: bool,
}
impl DialogFake {
    async fn confirm(&self, _: &Value) -> Result<bool, String> {
        Ok(self.accepted)
    }
}
fn text_arg(a: &[Value], n: usize) -> Result<&str, String> {
    a.get(n)
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing text argument".into())
}
static REVEAL: Mutex<Vec<(String, Vec<String>)>> = Mutex::new(vec![]);
async fn output(program: &str, args: &[&str], _seconds: u64) -> Result<String, String> {
    REVEAL
        .lock()
        .unwrap()
        .push((program.into(), args.iter().map(|s| s.to_string()).collect()));
    Ok(String::new())
}
#[path = "../fixtures/native-acks.rs"]
mod acks;
#[::tokio::test]
async fn exact_clipboard_reveal_confirmation_producers_with_owned_effects() {
    let fake = ClipboardFake {
        written: Mutex::new(vec![]),
        fail: false,
    };
    let text = "owned unicode\nфайл ; $()";
    assert_eq!(
        acks::clipboard(&fake, &[json!(text)]).unwrap(),
        json!({"ok":true})
    );
    assert_eq!(fake.written.lock().unwrap()[0], text);
    assert!(acks::clipboard(&fake, &[json!(true)]).is_err());
    let failed = ClipboardFake {
        written: Mutex::new(vec![]),
        fail: true,
    };
    assert!(acks::clipboard(&failed, &[json!(text)]).is_err());
    assert_eq!(acks::reveal(text).await.unwrap(), json!({"ok":true}));
    assert_eq!(
        REVEAL.lock().unwrap()[0],
        ("/usr/bin/open".into(), vec!["-R".into(), text.into()])
    );
    for accepted in [false, true] {
        assert_eq!(
            acks::confirm(&DialogFake { accepted }, json!({}))
                .await
                .unwrap(),
            json!({"response":if accepted{1}else{0}})
        );
    }
}
