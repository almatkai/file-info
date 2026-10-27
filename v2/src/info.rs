use crate::{secure::Selected, tools, Result};
use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub is_directory: bool,
    pub path: String,
    pub size: u64,
    pub extension: String,
    pub created: String,
    pub modified: String,
    pub accessed: String,
    pub permissions: String,
    pub owner: String,
    pub group: String,
    pub mime_type: String,
    pub dimensions: String,
    pub duration: String,
}
pub fn bytes(size: u64) -> String {
    if size == 0 {
        return "0 Bytes".into();
    }
    let index = ((size as f64).ln() / 1024f64.ln()).floor() as usize;
    let units = ["Bytes", "KB", "MB", "GB", "TB"];
    let value = ((size as f64 / 1024f64.powi(index as i32)) * 100.).round() / 100.;
    format!("{value} {}", units.get(index).unwrap_or(&"undefined"))
}
pub fn duration(seconds: f64) -> String {
    if seconds == f64::INFINITY {
        return "Infinity:NaN:NaN".into();
    }
    if !seconds.is_finite() {
        return "NaN:NaN".into();
    }
    let hours = (seconds / 3600.).floor();
    let min = ((seconds % 3600.) / 60.).floor();
    let sec = (seconds % 60.).floor();
    if hours > 0. {
        format!("{hours}:{min:02}:{sec:02}")
    } else {
        format!("{min}:{sec:02}")
    }
}
pub fn date(value: std::io::Result<std::time::SystemTime>) -> String {
    let Ok(time) = value else {
        return "Invalid Date".into();
    };
    let d: DateTime<Local> = time.into();
    let names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let hour = match d.hour() % 12 {
        0 => 12,
        h => h,
    };
    let am = if d.hour() < 12 { "AM" } else { "PM" };
    format!(
        "{} {}, {}, {hour:02}:{:02}:{:02} {am}",
        names[d.month0() as usize],
        d.day(),
        d.year(),
        d.minute(),
        d.second()
    )
}
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
fn first_number(v: &str) -> Option<f64> {
    // ECMAScript parseFloat prefix grammar, including incomplete exponent tails.
    let s = v.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let bytes = s.as_bytes();
    let mut p = 0;
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        p += 1;
    }
    if s.get(p..)?.starts_with("Infinity") {
        return Some(if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    let start = p;
    while p < bytes.len() && bytes[p].is_ascii_digit() {
        p += 1;
    }
    let mut digits = p - start;
    if bytes.get(p) == Some(&b'.') {
        p += 1;
        let start = p;
        while p < bytes.len() && bytes[p].is_ascii_digit() {
            p += 1;
        }
        digits += p - start;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(p), Some(b'e') | Some(b'E')) {
        let exponent = p;
        p += 1;
        if matches!(bytes.get(p), Some(b'+') | Some(b'-')) {
            p += 1;
        }
        let start = p;
        while p < bytes.len() && bytes[p].is_ascii_digit() {
            p += 1;
        }
        if p == start {
            p = exponent;
        }
    }
    s[..p].parse().ok()
}

fn number_after(v: &str, label: &str) -> Option<String> {
    let rest = v.split_once(label)?.1.trim_start();
    let rest = rest
        .strip_prefix('=')
        .or_else(|| rest.strip_prefix(':'))
        .unwrap_or(rest)
        .trim_start();
    let value: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
fn image_dimensions(v: &str) -> String {
    let t = v.trim();
    if let Some((a, b)) = t.split_once('x') {
        if !a.is_empty()
            && !b.is_empty()
            && a.bytes().all(|c| c.is_ascii_digit())
            && b.bytes().all(|c| c.is_ascii_digit())
        {
            return format!("{a} × {b}");
        }
    }
    match (
        number_after(v, "pixelWidth"),
        number_after(v, "pixelHeight"),
    ) {
        (Some(w), Some(h)) => format!("{w} × {h}"),
        _ => String::new(),
    }
}
async fn probe(s: &Selected, args: &[&str]) -> Result<String> {
    tools::inspect_with(
        s,
        "ffprobe",
        args,
        &[
            "/opt/homebrew/bin/ffprobe",
            "/usr/local/bin/ffprobe",
            "/opt/local/bin/ffprobe",
            "/usr/bin/ffprobe",
        ],
    )
    .await
}
async fn mdls_duration(s: &Selected) -> String {
    let v = tools::inspect_with(s, "mdls", &["-name", "kMDItemDurationSeconds"], &[])
        .await
        .unwrap_or_default();
    number_after(&v, "kMDItemDurationSeconds")
        .and_then(|n| first_number(&n))
        .map(duration)
        .unwrap_or_default()
}
pub async fn get(s: &Selected) -> Result<FileInfo> {
    let name = s
        .path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or("Selected filename is not UTF-8")?
        .to_string();
    // Node path.extname treats leading-dot files as extensionless, retaining case.
    let extension = name
        .rfind('.')
        .filter(|i| *i > 0)
        .map(|i| name[i..].to_string())
        .unwrap_or_else(|| "None".into());
    let ext = extension.to_lowercase();
    let mut owner = String::new();
    let mut group = String::new();
    if let Ok(v) = tools::inspect_with(s, "ls", &["-ldL"], &[]).await {
        let fields: Vec<_> = v.split_whitespace().collect();
        if fields.len() >= 4 {
            owner = fields[2].into();
            group = fields[3].into();
        }
    }
    let mime_type = tools::inspect_with(s, "file", &["--mime-type", "-b", "-L"], &[])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut dimensions = String::new();
    let mut dur = String::new();
    let images = [
        ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff", ".heic",
    ];
    let videos = [
        ".mp4", ".mov", ".avi", ".mkv", ".webm", ".m4v", ".flv", ".wmv", ".mpg", ".mpeg",
    ];
    let audio = [
        ".mp3", ".wav", ".aac", ".flac", ".m4a", ".ogg", ".wma", ".opus",
    ];
    if !s.metadata.is_dir() && (images.contains(&ext.as_str()) || videos.contains(&ext.as_str())) {
        let v = probe(
            s,
            &[
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height",
                "-of",
                "csv=s=x:p=0",
            ],
        )
        .await;
        if images.contains(&ext.as_str()) {
            if let Ok(v) = v {
                dimensions = image_dimensions(&v);
            } else if cfg!(target_os = "macos")
                && tools::find(
                    "ffprobe",
                    &[
                        "/opt/homebrew/bin/ffprobe",
                        "/usr/local/bin/ffprobe",
                        "/opt/local/bin/ffprobe",
                        "/usr/bin/ffprobe",
                    ],
                )
                .is_none()
            {
                dimensions = image_dimensions(
                    &tools::inspect_with(
                        s,
                        "sips",
                        &["-g", "pixelWidth", "-g", "pixelHeight"],
                        &[],
                    )
                    .await
                    .unwrap_or_default(),
                );
            }
        } else {
            if let Ok(v) = v {
                if v.trim().contains('x') {
                    dimensions = v.trim().replacen('x', " × ", 1);
                }
            }
            if dimensions.is_empty() {
                let v = tools::inspect_with(
                    s,
                    "mdls",
                    &["-name", "kMDItemPixelWidth", "-name", "kMDItemPixelHeight"],
                    &[],
                )
                .await
                .unwrap_or_default();
                if let (Some(w), Some(h)) = (
                    number_after(&v, "kMDItemPixelWidth"),
                    number_after(&v, "kMDItemPixelHeight"),
                ) {
                    dimensions = format!("{w} × {h}");
                }
            }
        }
    }
    if !s.metadata.is_dir() && (videos.contains(&ext.as_str()) || audio.contains(&ext.as_str())) {
        dur = probe(
            s,
            &[
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ],
        )
        .await
        .ok()
        .and_then(|v| first_number(&v))
        .map(duration)
        .unwrap_or_default();
        if dur.is_empty() && audio.contains(&ext.as_str()) {
            let v = tools::inspect_with(s, "afinfo", &[], &[])
                .await
                .unwrap_or_default();
            dur = number_after(&v, "estimated duration")
                .and_then(|x| first_number(&x))
                .map(duration)
                .unwrap_or_default();
        }
        if dur.is_empty() {
            dur = mdls_duration(s).await;
        }
    }
    s.recheck()?;
    Ok(FileInfo {
        name,
        is_directory: s.metadata.is_dir(),
        path: s.path.to_str().ok_or("Selected path is not UTF-8")?.into(),
        size: s.metadata.len(),
        extension,
        created: date(s.metadata.created()),
        modified: date(s.metadata.modified()),
        accessed: date(s.metadata.accessed()),
        permissions: s.mode(),
        owner,
        group,
        mime_type,
        dimensions,
        duration: dur,
    })
}
impl FileInfo {
    pub fn file_type(&self) -> &str {
        if self.is_directory {
            "Directory"
        } else {
            "File"
        }
    }
    pub fn copy_all(&self) -> String {
        format!("Name: {}\nPath: {}\nType: {}\nSize: {}\nExtension: {}\nCreated: {}\nModified: {}\nAccessed: {}\nPermissions: {}\nOwner: {}\nGroup: {}",self.name,self.path,self.file_type(),bytes(self.size),self.extension,self.created,self.modified,self.accessed,self.permissions,self.owner,self.group)
    }
    pub fn icon(&self) -> &str {
        if self.is_directory {
            return "Folder";
        }
        match self.extension.to_lowercase().as_str() {
            ".jpg" | ".jpeg" | ".png" | ".gif" | ".bmp" | ".svg" | ".webp" => "Image",
            ".mp4" | ".mov" | ".avi" | ".mkv" | ".webm" => "Video",
            ".mp3" | ".wav" | ".aac" | ".flac" | ".m4a" => "Music",
            ".txt" | ".md" | ".rtf" => "Text",
            ".zip" | ".tar" | ".gz" | ".rar" | ".7z" => "Box",
            ".js" | ".ts" | ".tsx" | ".jsx" | ".py" | ".java" | ".c" | ".cpp" | ".swift"
            | ".go" | ".rs" => "Code",
            _ => "Document",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn js_formatters() {
        assert_eq!(bytes(0), "0 Bytes");
        assert_eq!(bytes(1049600), "1 MB");
        assert_eq!(duration(3661.99), "1:01:01");
        assert_eq!(duration(59.99), "0:59");
        assert_eq!(grouped(1049600), "1,049,600");
        assert_eq!(image_dimensions("12x34\n"), "12 × 34");
        assert_eq!(image_dimensions("pixelWidth: 2\npixelHeight: 3"), "2 × 3");
        assert_eq!(first_number("12.5sec"), Some(12.5));
    }
}
#[cfg(test)]
mod source_oracles {
    use super::*;
    #[test]
    fn actual_v1_format_and_native_output_coercions() {
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/v1-oracle.json")).unwrap();
        for case in source["formatBytes"].as_array().unwrap() {
            assert_eq!(bytes(case[0].as_u64().unwrap()), case[1].as_str().unwrap());
        }
        for case in source["formatDuration"].as_array().unwrap() {
            assert_eq!(
                duration(case[0].as_f64().unwrap()),
                case[1].as_str().unwrap()
            );
        }
        for case in source["coercions"].as_array().unwrap() {
            assert_eq!(
                first_number(case[0].as_str().unwrap())
                    .map(duration)
                    .unwrap_or_default(),
                case[1].as_str().unwrap()
            );
        }
        let d = chrono::DateTime::parse_from_rfc3339(source["info"]["modified"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            date(Ok(d.into())),
            source["labels"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["title"] == "Modified")
                .unwrap()["text"]
        );
    }
    #[test]
    fn actual_v1_directory_type_icon_and_copy() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/v1-round2-oracle.json")).unwrap();
        for row in cases["directories"].as_array().unwrap() {
            let data = &row["info"];
            let text = |key: &str| data[key].as_str().unwrap().to_string();
            let label = |key: &str| {
                row["labels"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["title"] == key)
                    .unwrap()["text"]
                    .as_str()
                    .unwrap()
                    .to_string()
            };
            let info = FileInfo {
                name: text("name"),
                path: text("path"),
                is_directory: data["isDirectory"].as_bool().unwrap(),
                size: data["size"].as_u64().unwrap(),
                extension: text("extension"),
                created: label("Created"),
                modified: label("Modified"),
                accessed: label("Accessed"),
                permissions: text("permissions"),
                owner: text("owner"),
                group: text("group"),
                mime_type: text("mimeType"),
                dimensions: String::new(),
                duration: String::new(),
            };
            assert_eq!(info.file_type(), data["type"].as_str().unwrap());
            assert_eq!(info.icon(), row["icon"].as_str().unwrap());
            assert_eq!(
                info.copy_all(),
                row["copies"][1]["content"].as_str().unwrap()
            );
        }
    }
}
