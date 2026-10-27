// Generated from host pin ea100cee3f69711de12485ba061950c5fc65258f. QA ONLY.
pub mod original {
    use crate::{json, reqwest, tokio, CoreResult, Duration};
    pub async fn selected_files() -> CoreResult {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut cmd = tokio::process::Command::new("/usr/bin/osascript");
            cmd.args(["-l","JavaScript","-e",r#"JSON.stringify(Application('Finder').selection().slice(0,128).map(function(item){return item.url();}))"#]);
            cmd
        };
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut cmd = tokio::process::Command::new("powershell.exe");
            cmd.args(["-NoProfile","-NonInteractive","-Command",r#"$ErrorActionPreference='Stop';Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class TezbarSelectionWindow{[DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();}';$foreground=[TezbarSelectionWindow]::GetForegroundWindow().ToInt64();$paths=@();$shell=New-Object -ComObject Shell.Application;foreach($window in $shell.Windows()){if([long]$window.HWND -eq $foreground){$paths=@($window.Document.SelectedItems() | Select-Object -First 128 | ForEach-Object {$_.Path});break}};ConvertTo-Json -InputObject @($paths) -Compress"#]).creation_flags(0x08000000);
            cmd
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Err("File selection is unsupported on this platform".into());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            command
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(10), command.output())
                .await
                .map_err(|_| "File selection timed out")?
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err("Cannot read selected files from Finder/Explorer".into());
            }
            if output.stdout.len() > 1024 * 1024 {
                return Err("File selection response too large".into());
            }
            let values: Vec<String> = serde_json::from_slice(&output.stdout)
                .map_err(|_| "Invalid file selection response")?;
            if values.len() > 128 {
                return Err("Too many selected files".into());
            }
            let mut files = Vec::new();
            for value in values {
                #[cfg(target_os = "macos")]
                let path = reqwest::Url::parse(&value)
                    .map_err(|e| e.to_string())?
                    .to_file_path()
                    .map_err(|_| "Selection URL is not a local file")?;
                #[cfg(target_os = "windows")]
                let path = PathBuf::from(value);
                if path.is_absolute() && path.is_file() {
                    files.push(path);
                }
            }
            Ok(json!(files))
        }
    }
}
pub mod patched {
    use crate::{json, reqwest, tokio, CoreResult, Duration};
    pub async fn selected_files() -> CoreResult {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut cmd = tokio::process::Command::new("/usr/bin/osascript");
            cmd.args(["-l","JavaScript","-e",r#"JSON.stringify(Application('Finder').selection().map(function(item){return item.url();}))"#]);
            cmd
        };
        #[cfg(target_os = "windows")]
        let mut command = {
            let mut cmd = tokio::process::Command::new("powershell.exe");
            cmd.args(["-NoProfile","-NonInteractive","-Command",r#"$ErrorActionPreference='Stop';Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class TezbarSelectionWindow{[DllImport("user32.dll")]public static extern IntPtr GetForegroundWindow();}';$foreground=[TezbarSelectionWindow]::GetForegroundWindow().ToInt64();$paths=@();$shell=New-Object -ComObject Shell.Application;foreach($window in $shell.Windows()){if([long]$window.HWND -eq $foreground){$paths=@($window.Document.SelectedItems() | ForEach-Object {$_.Path});break}};ConvertTo-Json -InputObject @($paths) -Compress"#]).creation_flags(0x08000000);
            cmd
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Err("File selection is unsupported on this platform".into());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            command
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true);
            use tokio::io::AsyncReadExt;
            command
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            let stdout = child
                .stdout
                .take()
                .ok_or("File selection stdout unavailable")?;
            let stderr = child
                .stderr
                .take()
                .ok_or("File selection stderr unavailable")?;
            let output = tokio::time::timeout(Duration::from_secs(10), async {
                let mut out = Vec::new();
                let mut err = Vec::new();
                let mut stdout = stdout.take(1024 * 1024 + 1);
                let mut stderr = stderr.take(16 * 1024 + 1);
                let (status, _, _) = tokio::try_join!(
                    child.wait(),
                    stdout.read_to_end(&mut out),
                    stderr.read_to_end(&mut err)
                )
                .map_err(|e| e.to_string())?;
                if out.len() > 1024 * 1024 || err.len() > 16 * 1024 {
                    return Err("File selection output exceeds bounds".to_string());
                }
                Ok(std::process::Output {
                    status,
                    stdout: out,
                    stderr: err,
                })
            })
            .await
            .map_err(|_| "File selection timed out")??;
            if !output.status.success() {
                return Err("Cannot read selected files from Finder/Explorer".into());
            }
            if output.stdout.len() > 1024 * 1024 {
                return Err("File selection response too large".into());
            }
            let values: Vec<String> = serde_json::from_slice(&output.stdout)
                .map_err(|_| "Invalid file selection response")?;
            if values.len() > 128 {
                return Err("Too many selected files".into());
            }
            let mut files = Vec::new();
            for value in values {
                #[cfg(target_os = "macos")]
                let path = reqwest::Url::parse(&value)
                    .map_err(|e| e.to_string())?
                    .to_file_path()
                    .map_err(|_| "Selection URL is not a local file")?;
                #[cfg(target_os = "windows")]
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("Selection contains a non-absolute path".into());
                }
                // Preserve the complete snapshot; the command validates ordinary-file policy.
                files.push(path);
            }
            Ok(json!(files))
        }
    }
}
