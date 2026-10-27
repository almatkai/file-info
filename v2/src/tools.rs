//! Only authentic V1 native tools. No shell, package manager, auto-install or config loading.
use crate::{format, secure::Selected, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};
pub const OUTPUT_LIMIT: u64 = 64 * 1024;
fn candidates(name: &str, common: &[&str], path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if name == "exiftool" {
        candidates.extend(common.iter().map(PathBuf::from));
    }
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            if dir.is_absolute() {
                candidates.push(dir.join(name));
                #[cfg(windows)]
                candidates.push(dir.join(format!("{name}.exe")));
            }
        }
    }
    if name != "exiftool" {
        candidates.extend(common.iter().map(PathBuf::from));
    }
    candidates
}
pub fn find(name: &str, common: &[&str]) -> Option<PathBuf> {
    candidates(name, common, std::env::var_os("PATH").as_deref())
        .into_iter()
        .filter(|p| p.is_absolute())
        .find(|p| executable(p))
}
fn executable(path: &Path) -> bool {
    let Ok(m) = path.metadata() else {
        return false;
    };
    if !m.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
pub async fn exiftool() -> Result<PathBuf> {
    const COMMON: &[&str] = &[
        "/usr/local/bin/exiftool",
        "/opt/homebrew/bin/exiftool",
        "/usr/bin/exiftool",
        r"C:\Program Files\ExifTool\exiftool.exe",
    ];
    let path = find("exiftool", COMMON).ok_or_else(|| "ExifTool is required to clear embedded metadata. Install it manually from exiftool.org, then invoke Clear File Metadata again. Nothing was changed; this extension never installs tools.".to_string())?;
    // V1 accepts common paths first and only probes the PATH fallback.
    if !COMMON.iter().any(|p| Path::new(p) == path) {
        let mut probe = Command::new(&path);
        probe.args(["-config", "", "-ver"]);
        output(probe, 10).await?;
    }
    Ok(path)
}
pub async fn output(command: Command, seconds: u64) -> Result<String> {
    output_impl(command, seconds, false).await
}
async fn output_impl(mut command: Command, seconds: u64, diagnostics: bool) -> Result<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start native tool: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Native tool stdout unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Native tool stderr unavailable")?;
    let work = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let read_out = async {
            stdout
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut out)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        let read_err = async {
            stderr
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut err)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        // Once a pipe reaches the limit it closes; a noisy child cannot grow
        // memory or block the reader indefinitely. The deadline kills it.
        let (status, _, _) = tokio::try_join!(
            async { child.wait().await.map_err(|e| e.to_string()) },
            read_out,
            read_err
        )?;
        if out.len() as u64 > OUTPUT_LIMIT || err.len() as u64 > OUTPUT_LIMIT {
            return Err("Native tool output exceeds 64 KiB".into());
        }
        if err.len() > 4096 {
            return Err(format!(
                "Native diagnostic exceeds 4 KiB; selected original retained ({} bytes)",
                err.len()
            ));
        }
        if !status.success() {
            return Err(format!(
                "Native tool failed ({}): {}",
                status,
                String::from_utf8_lossy(&err).trim()
            ));
        }
        if diagnostics {
            if out.len() > 4096 {
                return Err("Metadata tool stdout exceeds 4 KiB; original retained".into());
            }
            return Ok(String::from_utf8_lossy(&err).into_owned());
        }
        String::from_utf8(out).map_err(|_| "Native tool returned invalid UTF-8".into())
    };
    tokio::time::timeout(Duration::from_secs(seconds), work)
        .await
        .map_err(|_| {
            "Native tool timed out; selected original not modified by helper".to_string()
        })?
}
pub async fn inspect_with(
    s: &Selected,
    name: &str,
    args: &[&str],
    common: &[&str],
) -> Result<String> {
    s.recheck()?;
    let path = find(name, common).ok_or_else(|| format!("{name} is not installed"))?;
    let mut command = Command::new(path);
    if name == "ffprobe" {
        // Preserve V1's media containers while refusing playlists/nested network
        // protocols. A selected disguised playlist cannot read unrelated files.
        command.args(["-protocol_whitelist","file,pipe","-format_whitelist","png_pipe,jpeg_pipe,webp_pipe,tiff_pipe,bmp_pipe,gif,apng,mov,avi,matroska,webm,flv,asf,mpeg,m4v,mp3,wav,aac,flac,ogg"]);
    }
    command.args(args);
    #[cfg(unix)]
    let path = crate::secure::anchor(&mut command, &s.file, None);
    #[cfg(windows)]
    let path = s.path.to_string_lossy().into_owned();
    if name == "mdls" {
        command.arg(&s.path);
    } else {
        command.arg(path);
    }
    let result = output(command, 10).await;
    s.recheck()?;
    result
}
const DETAILED: &[&str] = &[
    "-all=",
    "-EXIF:all=",
    "-GPS:all=",
    "-IPTC:all=",
    "-XMP:all=",
    "-ICC_Profile:all=",
    "-ThumbnailImage=",
    "-PreviewImage=",
    "-Comment=",
    "-MakerNotes=",
    "-UserComment=",
    "-PDF:all=",
    "-ID3:all=",
    "-QuickTime:all=",
    "-overwrite_original",
    "-q",
];
/// Keep the selected suffix for native writers that use it to identify their
/// family (including XMP sidecars). No selected spelling becomes an option/path.
fn extension(s: &Selected) -> &str {
    s.path
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| s.len() <= 64 && s.bytes().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin")
}
fn staged_command(tool: &Path, stage: &crate::secure::Stage, args: &[&str]) -> Command {
    let mut command = Command::new(tool);
    command
        .args(["-config", "", "-charset", "filename=UTF8"])
        .args(args);
    #[cfg(unix)]
    let path = crate::secure::anchor(&mut command, &stage.dir, Some(&stage.leaf));
    #[cfg(windows)]
    let path = stage.path.to_string_lossy().into_owned();
    command.arg(path);
    command
}
async fn native_type(tool: &Path, stage: &crate::secure::Stage) -> Result<String> {
    let text = output(
        staged_command(
            tool,
            stage,
            &["-j", "-n", "-FileType", "-FileTypeExtension", "-Error"],
        ),
        10,
    )
    .await?;
    stage.verify()?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Invalid native format response")?;
    let rows = value
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("Expected one native file-format record")?;
    if rows[0].get("Error").is_some() {
        return Err(format!(
            "Native format reader rejected candidate: {}",
            rows[0]["Error"]
        ));
    }
    let kind = rows[0]
        .get("FileTypeExtension")
        .or_else(|| rows[0].get("FileType"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or("Native tool could not identify file format; original retained")?;
    Ok(kind.into())
}
/// Every helper writes a private destination-directory stage, never the selected
/// original. Only a native detailed-process failure triggers V1's fresh fallback.
/// Unknown witness layouts trust ExifTool, as V1 did, with explicit finite evidence.
pub async fn clear(
    s: &mut Selected,
    tool: &Path,
    ctx: &tezbar_extension_sdk::ExtensionContext,
) -> Result<crate::secure::Commit> {
    let before = s.read()?;
    let witness = format::witness(&before)?;
    let mut errors = Vec::new();
    for args in [DETAILED, &["-all=", "-overwrite_original", "-q"][..]] {
        let mut stage = s.stage(extension(s), &before)?;
        stage.reopen()?;
        let kind = native_type(tool, &stage).await?;
        match output_impl(staged_command(tool, &stage, args), 60, true).await {
            Ok(warnings) => {
                let after = stage.reopen()?;
                let after_kind = native_type(tool, &stage).await?;
                if kind != after_kind {
                    return Err(
                        "Metadata tool changed the native file format; original retained".into(),
                    );
                }
                if witness.is_some() {
                    format::equivalent(&before, &after)?;
                }
                tokio::select! {biased; _=ctx.cancelled()=>return Err("Cancelled before selected-file replacement".into()), _=std::future::ready(())=>{}}
                // No await from recheck/rename through the caller's checkpoint.
                let mut message = s.commit(&mut stage)?;
                if !warnings.is_empty() || kind == "PDF" {
                    message.complete = false;
                }
                if !warnings.is_empty() {
                    message.detail.push_str(&format!(
                        " ExifTool warning (successful exit, no fallback): {}",
                        warnings.trim()
                    ));
                }
                message.detail.push_str(&format!(
                    " Native format {kind} preserved; candidate ordinary, readable and nonempty."
                ));
                if witness.is_some() {
                    message
                        .detail
                        .push_str(" Additional finite encoded-payload witness passed.");
                } else {
                    message.detail.push_str(" This layout trusts the native ExifTool writer as in V1; no independent encoded-payload proof is available. Metadata removal is limited by the native writer.");
                }
                if kind == "PDF" {
                    message.detail.push_str(" PDF changes are incremental, as in V1: earlier metadata remains recoverable in original bytes; this is not secure PDF redaction.");
                }
                return Ok(message);
            }
            Err(e) => errors.push(e),
        }
    }
    Err(format!("ExifTool detailed and fallback attempts failed; original encoded bytes/extended attributes retained (reads may advance access time): {}", errors.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_specific_resolution_order_uses_only_owned_fake_tools() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".qa/resolution")
            .join(uuid::Uuid::new_v4().to_string());
        let common = root.join("common");
        let path = root.join("path");
        std::fs::create_dir_all(&common).unwrap();
        std::fs::create_dir_all(&path).unwrap();
        for name in ["exiftool", "ffprobe"] {
            let c = common.join(name);
            let p = path.join(name);
            for file in [&c, &p] {
                std::fs::write(file, b"owned executable fixture").unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            let list = candidates(name, &[c.to_str().unwrap()], Some(path.as_os_str()));
            let found = list.iter().find(|p| executable(p)).unwrap();
            assert_eq!(found, if name == "exiftool" { &c } else { &p });
        }
    }
}
