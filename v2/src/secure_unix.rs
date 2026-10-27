use super::*;
use std::{
    ffi::{CString, OsStr, OsString},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
};
fn c(s: &OsStr) -> Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| "NUL path".into())
}
fn fd(result: i32) -> Result<File> {
    if result < 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(unsafe { File::from_raw_fd(result) })
    }
}
pub fn open(path: &Path) -> Result<(File, OsString, File, Vec<File>)> {
    let mut dir = fd(unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    let parts: Vec<_> = path
        .components()
        .filter_map(|x| {
            if let Component::Normal(x) = x {
                Some(x)
            } else {
                None
            }
        })
        .collect();
    let (name, parents) = parts
        .split_last()
        .ok_or("Selection is not an ordinary file")?;
    for part in parents {
        let next = fd(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c(part)?.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        dir = next;
    }
    let file = fd(unsafe {
        libc::openat(
            dir.as_raw_fd(),
            c(name)?.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    })?;
    Ok((dir, name.to_os_string(), file, vec![]))
}
pub fn same(a: &Metadata, b: &Metadata) -> bool {
    same_directory(a, b)
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
        && a.mode() == b.mode()
        && a.nlink() == b.nlink()
}
pub fn same_file(a: &File, b: &File) -> Result<bool> {
    Ok(same_directory(
        &a.metadata().map_err(|e| e.to_string())?,
        &b.metadata().map_err(|e| e.to_string())?,
    ))
}
pub fn same_directory(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}
pub fn clear_policy(m: &Metadata) -> Result<()> {
    if m.nlink() != 1 {
        return Err(
            "Hard-linked files cannot be cleared safely (other names would retain metadata)".into(),
        );
    }
    if m.mode() & 0o7000 != 0 {
        return Err("Special permission bits are not supported for metadata replacement".into());
    }
    if m.mode() & 0o222 == 0 {
        return Err("Read-only selected file; original retained".into());
    }
    Ok(())
}
pub fn identity(_file: &File, m: &Metadata) -> Result<serde_json::Value> {
    Ok(
        serde_json::json!({"device":m.dev().to_string(),"inode":m.ino().to_string(),"bytes":m.len(),"mtimeSeconds":m.mtime(),"mtimeNanos":m.mtime_nsec(),"ctimeSeconds":m.ctime(),"ctimeNanos":m.ctime_nsec(),"links":m.nlink()}),
    )
}
pub fn mode(m: &Metadata) -> String {
    format!("{:03o}", m.mode() & 0o777)
}
pub struct Stage {
    pub file: File,
    pub dir: File,
    pub leaf: String,
    name: CString,
    parent: File,
    committed: bool,
    validated: Option<Metadata>,
}
impl Stage {
    pub fn new(s: &Selected, extension: &str) -> Result<Self> {
        let name = CString::new(format!(".tezbar-file-info-{}", uuid::Uuid::new_v4())).unwrap();
        if unsafe { libc::mkdirat(s.parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let dir = match fd(unsafe {
            libc::openat(
                s.parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }) {
            Ok(v) => v,
            Err(e) => {
                return Err(format!(
                    "Private staging directory could not be opened: {e}"
                ))
            }
        };
        let leaf = format!("content.{extension}");
        let file = fd(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                CString::new(leaf.as_str()).unwrap().as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        Ok(Self {
            file,
            dir,
            leaf,
            name,
            parent: s.parent.try_clone().map_err(|e| e.to_string())?,
            committed: false,
            validated: None,
        })
    }
    pub fn reopen(&mut self) -> Result<Vec<u8>> {
        let file = fd(unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                CString::new(self.leaf.as_str()).unwrap().as_ptr(),
                libc::O_RDWR | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.nlink() != 1 || meta.len() == 0 || meta.len() > MAX_FILE {
            return Err("Metadata tool produced a non-ordinary or oversized file".into());
        }
        self.file = file;
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        (&mut self.file)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_FILE {
            return Err("Metadata tool output exceeds limit".into());
        }
        let final_metadata = self.file.metadata().map_err(|e| e.to_string())?;
        if bytes.len() as u64 != meta.len() || !same(&meta, &final_metadata) {
            return Err("Native candidate changed while being read; original retained".into());
        }
        self.validated = Some(final_metadata);
        Ok(bytes)
    }
    pub fn verify(&self) -> Result<()> {
        let current = fd(unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                CString::new(self.leaf.as_str()).unwrap().as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        })?;
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
        // Closed exact temporary member set; never recursive cleanup.
        for suffix in ["", "_original", "_exiftool_tmp"] {
            if self.committed && suffix.is_empty() {
                continue;
            }
            if let Ok(name) = CString::new(format!("{}{suffix}", self.leaf)) {
                unsafe {
                    libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0);
                }
            }
        }
        unsafe {
            libc::unlinkat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                libc::AT_REMOVEDIR,
            );
        }
    }
}
#[cfg(target_os = "macos")]
fn xattrs(file: &File) -> Result<Vec<Vec<u8>>> {
    let n = unsafe { libc::flistxattr(file.as_raw_fd(), std::ptr::null_mut(), 0, 0) };
    if n < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if n > 64 * 1024 {
        return Err("Extended attributes exceed 64 KiB name limit".into());
    }
    let mut names = vec![0u8; n as usize];
    let size =
        unsafe { libc::flistxattr(file.as_raw_fd(), names.as_mut_ptr().cast(), names.len(), 0) };
    if size < 0 || size as usize > names.len() {
        return Err("Extended attribute names changed during enumeration".into());
    }
    names.truncate(size as usize);
    Ok(names
        .split(|x| *x == 0)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_vec())
        .collect())
}
#[cfg(target_os = "macos")]
fn clear_xattrs(file: &File) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for name in xattrs(file)? {
        let c = CString::new(name.clone()).map_err(|_| "Invalid xattr name")?;
        if unsafe { libc::fremovexattr(file.as_raw_fd(), c.as_ptr(), 0) } != 0 {
            failures.push(format!(
                "{}: {}",
                String::from_utf8_lossy(&name),
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(failures)
}
#[cfg(not(target_os = "macos"))]
fn xattrs(_file: &File) -> Result<Vec<Vec<u8>>> {
    Err("Only macOS and Windows production platforms are supported".into())
}
#[cfg(not(target_os = "macos"))]
fn clear_xattrs(_file: &File) -> Result<Vec<String>> {
    Err("Only macOS and Windows production platforms are supported".into())
}
pub fn commit(s: &Selected, t: &mut Stage) -> Result<Commit> {
    s.recheck()?;
    clear_policy(&s.metadata)?;
    let original_attrs = xattrs(&s.file)?.len();
    let attr_failures = clear_xattrs(&t.file)?;
    // Replacement's current-user ownership matches V1's attempted chown; preserve
    // the original group/mode, and fail before replacing if unavailable.
    if unsafe { libc::fchown(t.file.as_raw_fd(), libc::geteuid(), s.metadata.gid()) } != 0 {
        return Err(format!(
            "Ownership/group preparation failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe {
        libc::fchmod(
            t.file.as_raw_fd(),
            (s.metadata.mode() & 0o7777) as libc::mode_t,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let mut attrs = libc::attrlist {
            bitmapcount: 5,
            reserved: 0,
            commonattr: libc::ATTR_CMN_CRTIME,
            volattr: 0,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0,
        };
        let birth: chrono::DateTime<chrono::Utc> =
            s.metadata.created().map_err(|e| e.to_string())?.into();
        let mut created = libc::timespec {
            tv_sec: birth.timestamp(),
            tv_nsec: birth.timestamp_subsec_nanos() as _,
        };
        if unsafe {
            libc::fsetattrlist(
                t.file.as_raw_fd(),
                (&mut attrs as *mut libc::attrlist).cast(),
                (&mut created as *mut libc::timespec).cast(),
                std::mem::size_of::<libc::timespec>(),
                0,
            )
        } != 0
        {
            return Err(format!(
                "Cannot preserve creation timestamp: {}",
                std::io::Error::last_os_error()
            ));
        }
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
    let name = CString::new(t.leaf.as_str()).unwrap();
    let dest = c(&s.name)?;
    let current = fd(unsafe {
        libc::openat(
            t.dir.as_raw_fd(),
            CString::new(t.leaf.as_str()).unwrap().as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    })?;
    if !same_file(&t.file, &current)? {
        return Err("Candidate identity changed before rename; original retained".into());
    }
    // One descriptor-relative same-directory rename. There is no path lookup
    // through symlinks and no copy/delete fallback on a different volume.
    if unsafe {
        libc::renameat(
            t.dir.as_raw_fd(),
            name.as_ptr(),
            s.parent.as_raw_fd(),
            dest.as_ptr(),
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    t.committed = true;
    let synced = s.parent.sync_all().is_ok();
    let final_attrs = xattrs(&t.file);
    let complete =
        synced && attr_failures.is_empty() && final_attrs.as_ref().is_ok_and(|a| a.is_empty());
    let names = match final_attrs {
        Ok(names) => names
            .iter()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect::<Vec<_>>()
            .join(", "),
        Err(e) => format!("verification failed: {e}"),
    };
    let detail=format!("Replacement committed. Embedded metadata processed; {original_attrs} original xattr names observed; original attribute streams not copied to fresh inode. Actual remaining xattrs: [{}]. Attribute removal errors: [{}]. Owner is current effective user; group/mode/creation time preserved; accessed/modified set to now. {}. Sidecars untouched.",names,attr_failures.join("; "),if synced{"Directory synced"}else{"Directory sync failed; crash durability unknown"});
    Ok(Commit { detail, complete })
}
pub fn anchor(command: &mut tokio::process::Command, file: &File, leaf: Option<&str>) -> String {
    let descriptor = file.as_raw_fd();
    if let Some(leaf) = leaf {
        // macOS fdesc paths are not directory-traversable. A child-only fchdir
        // anchors all ExifTool relative opens/renames to the private stage fd.
        unsafe {
            command.pre_exec(move || {
                if libc::fchdir(descriptor) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        leaf.to_owned()
    } else {
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(descriptor, 100) < 0 || libc::fcntl(100, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        "/dev/fd/100".into()
    }
}
