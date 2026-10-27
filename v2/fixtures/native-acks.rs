// Exact pinned ACK producer bodies with owned effect receivers. QA ONLY.
use crate::{json, output, text_arg, ClipboardFake, CoreResult, DialogFake, Value};
#[allow(clippy::needless_return)] // Preserve the actual pinned early-return producer.
pub fn clipboard(fake: &ClipboardFake, a: &[Value]) -> CoreResult {
    fake.write_text(text_arg(a, 0)?.into())?;
    return Ok(json!({"ok":true}));
}
pub async fn reveal(text: &str) -> CoreResult {
    #[cfg(target_os = "macos")]
    output("/usr/bin/open", &["-R", text], 10).await?;
    #[cfg(target_os = "windows")]
    {
        tokio::process::Command::new("explorer.exe")
            .arg(format!("/select,{text}"))
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    open_target(
        Path::new(text)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(text),
    )
    .await?;
    Ok(json!({"ok":true}))
}
pub async fn confirm(fake: &DialogFake, value: Value) -> CoreResult {
    Ok(json!({"response":if fake.confirm(&value).await?{1}else{0}}))
}
