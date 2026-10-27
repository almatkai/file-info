//! Windows native handle identity, reparse rejection, protected staging ACL and
//! handle-relative replacement. Requires Windows 10 FileRenameInfoEx semantics.
use super::*;
use std::{
    ffi::OsString,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
};
use windows_sys::Win32::{
    Foundation::{LocalFree, HANDLE},
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SetSecurityInfo,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::*,
};
fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}
fn fail() -> String {
    std::io::Error::last_os_error().to_string()
}
fn handle(f: &File) -> HANDLE {
    f.as_raw_handle() as HANDLE
}
fn info(f: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut i = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(handle(f), &mut i) } == 0 {
        Err(fail())
    } else {
        Ok(i)
    }
}
fn opened(p: &Path, directory: bool, write: bool) -> Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .write(write)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
    if write {
        // Stage handles need DELETE for FileRenameInfoEx, plus read/write,
        // READ_CONTROL, WRITE_DAC and WRITE_OWNER for metadata preparation.
        options.access_mode(0xC00F0000);
    }
    let file = options.open(p).map_err(|e| e.to_string())?;
    if info(&file)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Reparse points are not selected ordinary files/directories".into());
    }
    Ok(file)
}
pub fn open(path: &Path) -> Result<(File, OsString, File, Vec<File>)> {
    let mut current = PathBuf::new();
    let mut dirs = Vec::new();
    let name = path
        .file_name()
        .ok_or("Missing ordinary selected filename")?
        .to_os_string();
    for part in path.parent().ok_or("Missing selected parent")?.components() {
        current.push(part.as_os_str());
        if current.is_absolute() {
            // Deny delete sharing on every ancestor: native helper absolute paths
            // cannot be redirected by renaming an ancestor during the operation.
            let f = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(&current)
                .map_err(|e| e.to_string())?;
            if info(&f)?.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("Selected path has a reparse-point ancestor".into());
            }
            dirs.push(f);
        }
    }
    let parent = dirs.pop().ok_or("Missing ordinary parent directory")?;
    let file = opened(path, true, false)?;
    if file.metadata().map_err(|e| e.to_string())?.is_file() && info(&file)?.nNumberOfLinks != 1 {
        return Err("Hard-linked files are not supported on Windows".into());
    }
    Ok((parent, name, file, dirs))
}
pub fn same_file(a: &File, b: &File) -> Result<bool> {
    let a = info(a)?;
    let b = info(b)?;
    Ok(a.dwVolumeSerialNumber == b.dwVolumeSerialNumber
        && a.nFileIndexHigh == b.nFileIndexHigh
        && a.nFileIndexLow == b.nFileIndexLow)
}
pub fn same(a: &Metadata, b: &Metadata) -> bool {
    a.file_size() == b.file_size()
        && a.creation_time() == b.creation_time()
        && a.last_write_time() == b.last_write_time()
        && a.file_attributes() == b.file_attributes()
}
pub fn clear_policy(m: &Metadata) -> Result<()> {
    if m.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
        Err("Read-only selected file; original retained".into())
    } else {
        Ok(())
    }
}
pub fn identity(file: &File, _m: &Metadata) -> Result<serde_json::Value> {
    let i = info(file)?;
    Ok(
        serde_json::json!({"volume":i.dwVolumeSerialNumber.to_string(),"fileId":format!("{:08x}{:08x}",i.nFileIndexHigh,i.nFileIndexLow),"bytes":((i.nFileSizeHigh as u64)<<32)|i.nFileSizeLow as u64,"links":i.nNumberOfLinks}),
    )
}
pub fn mode(m: &Metadata) -> String {
    if m.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
        "444"
    } else {
        "666"
    }
    .into()
}
pub struct Stage {
    pub file: File,
    pub dir: File,
    pub leaf: String,
    pub path: PathBuf,
    directory: PathBuf,
    committed: bool,
    validated: Option<Metadata>,
}
impl Stage {
    pub fn new(s: &Selected, extension: &str) -> Result<Self> {
        let directory = s
            .path
            .parent()
            .ok_or("Missing destination directory")?
            .join(format!(".tezbar-file-info-{}", uuid::Uuid::new_v4()));
        let mut descriptor = std::ptr::null_mut();
        let sddl = wide(std::ffi::OsStr::new("D:P(A;OICI;FA;;;OW)"));
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(fail());
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let ok = unsafe { CreateDirectoryW(wide(directory.as_os_str()).as_ptr(), &attrs) };
        unsafe {
            LocalFree(descriptor as _);
        }
        if ok == 0 {
            return Err(fail());
        }
        let dir = opened(&directory, true, false)?;
        let leaf = format!("content.{extension}");
        let path = directory.join(&leaf);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            dir,
            leaf,
            path,
            directory,
            committed: false,
            validated: None,
        })
    }
    pub fn reopen(&mut self) -> Result<Vec<u8>> {
        let file = opened(&self.path, false, true)?;
        let m = file.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() || m.len() == 0 || m.len() > MAX_FILE || info(&file)?.nNumberOfLinks != 1 {
            return Err("Metadata tool output is non-ordinary or over limit".into());
        }
        self.file = file;
        let mut bytes = Vec::new();
        (&mut self.file)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_FILE {
            return Err("Metadata tool output exceeds limit".into());
        }
        let final_metadata = self.file.metadata().map_err(|e| e.to_string())?;
        if bytes.len() as u64 != m.len() || !same(&m, &final_metadata) {
            return Err("Native candidate changed while being read; original retained".into());
        }
        self.validated = Some(final_metadata);
        Ok(bytes)
    }
    pub fn verify(&self) -> Result<()> {
        let current = opened(&self.path, false, false)?;
        if !same_file(&self.file, &current)? {
            return Err("Validated candidate identity changed; original retained".into());
        }
        if let Some(validated) = &self.validated {
            if !same(validated, &current.metadata().map_err(|e| e.to_string())?) {
                return Err("Validated candidate content changed; original retained".into());
            }
        }
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        for suffix in ["", "_original", "_exiftool_tmp"] {
            if self.committed && suffix.is_empty() {
                continue;
            }
            let path = self.directory.join(format!("{}{suffix}", self.leaf));
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir(&self.directory);
    }
}
pub fn commit(s: &Selected, t: &mut Stage) -> Result<Commit> {
    s.recheck()?;
    clear_policy(&s.metadata)?;
    // Set the actual current process user's SID through the file handle.
    let mut token = std::ptr::null_mut();
    if unsafe {
        windows_sys::Win32::System::Threading::OpenProcessToken(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            windows_sys::Win32::Security::TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(fail());
    }
    let mut needed = 0u32;
    unsafe {
        windows_sys::Win32::Security::GetTokenInformation(
            token,
            windows_sys::Win32::Security::TokenUser,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 || needed > 65536 {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(token);
        }
        return Err("Current token owner exceeds bound".into());
    }
    let mut token_data = vec![0u64; (needed as usize).div_ceil(8)];
    let ok = unsafe {
        windows_sys::Win32::Security::GetTokenInformation(
            token,
            windows_sys::Win32::Security::TokenUser,
            token_data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(token);
    }
    if ok == 0 {
        return Err(fail());
    }
    let user = unsafe {
        &*token_data
            .as_ptr()
            .cast::<windows_sys::Win32::Security::TOKEN_USER>()
    };
    let owner_result = unsafe {
        SetSecurityInfo(
            handle(&t.file),
            SE_FILE_OBJECT,
            windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION,
            user.User.Sid,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if owner_result != 0 {
        return Err(format!(
            "Cannot set actual current-user owner: {owner_result}"
        ));
    }
    let original_info = info(&s.file)?;
    let created = ((original_info.ftCreationTime.dwHighDateTime as u64) << 32)
        | original_info.ftCreationTime.dwLowDateTime as u64;
    let basic = FILE_BASIC_INFO {
        CreationTime: created as i64,
        LastAccessTime: 0,
        LastWriteTime: 0,
        ChangeTime: 0,
        FileAttributes: FILE_ATTRIBUTE_NORMAL,
    };
    if unsafe {
        SetFileInformationByHandle(
            handle(&t.file),
            FileBasicInfo,
            (&basic as *const FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(format!(
            "Cannot prepare Windows file attributes: {}",
            fail()
        ));
    }
    // Preserve selected DACL; stage creation gives current-token ownership.
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let code = unsafe {
        GetSecurityInfo(
            handle(&s.file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(format!("Cannot read selected file DACL: {code}"));
    }
    let result = unsafe {
        SetSecurityInfo(
            handle(&t.file),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        )
    };
    unsafe {
        LocalFree(descriptor as _);
    }
    if result != 0 {
        return Err(format!("Cannot preserve selected DACL: {result}"));
    }
    let now = std::time::SystemTime::now();
    t.file
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(now)
                .set_modified(now),
        )
        .map_err(|e| e.to_string())?;
    t.file.sync_all().map_err(|e| e.to_string())?;
    s.recheck()?;
    #[repr(C)]
    struct Rename {
        flags: u32,
        root: HANDLE,
        length: u32,
        name: [u16; 1],
    }
    let name: Vec<u16> = s.name.encode_wide().collect();
    let offset = std::mem::offset_of!(Rename, name);
    let length = offset + name.len() * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    let pointer = buffer.as_mut_ptr().cast::<Rename>();
    unsafe {
        (*pointer).flags = 3;
        (*pointer).root = handle(&s.parent);
        (*pointer).length = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            buffer.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
    }
    if unsafe {
        SetFileInformationByHandle(
            handle(&t.file),
            FileRenameInfoEx,
            buffer.as_ptr().cast(),
            length as u32,
        )
    } == 0
    {
        return Err(format!(
            "Handle-relative atomic replacement failed: {}",
            fail()
        ));
    }
    t.committed = true;
    let actual = info(&t.file);
    let complete = actual
        .as_ref()
        .is_ok_and(|i| i.dwFileAttributes == FILE_ATTRIBUTE_NORMAL);
    let attributes = actual
        .map(|i| format!("0x{:08x}", i.dwFileAttributes))
        .unwrap_or_else(|e| format!("verification failed: {e}"));
    Ok(Commit{complete,detail:format!("Replacement committed. Embedded metadata processed; current-user owner set by native SID API and selected DACL copied. Actual file attributes: {attributes}. Original alternate streams not copied to fresh inode; creation time preserved; accessed/modified set to now. Sidecars untouched. Windows crash durability and desktop behavior require native verification.")})
}
pub fn anchor(_command: &mut tokio::process::Command, _file: &File, _leaf: Option<&str>) -> String {
    String::new()
}
