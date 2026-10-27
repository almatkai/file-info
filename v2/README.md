# File Info — standalone Rust v2 review candidate

Both original commands are declared under `com.tezbar.native.file-info` in a schema-3, protocol-1 external executable. The SDK is 0.1.1 pinned to HTTPS Git revision `e955911f0bf5b558f9d3eb5183df0adc95c22af2`; Rust 1.85 is the minimum. There is no Tauri registration, acquisition entry, release URL, automatic installation, JS engine, or SDK path dependency. `artifacts` is empty and both desktop verification entries remain `not-run`.

`show-file-info` reads one file-selection snapshot and shows the first selected ordinary file or directory. Directory inspection is read-only and never enumerates children. It preserves File Overview/Quick Stats, metadata order, conditional MIME/dimensions/duration, byte/date/duration formatting, file icons and the four actions: Copy File Path, Show in Finder, Open With, Copy All Info. EXIF/GPS/IPTC/XMP tags were **not** individual view fields in V1, and are not invented here. Open With uses the SDK's installed-application list and explicit open-with method. Filtering is local; refresh does not replace the user's search text.

`clear-file-metadata` remains no-view and retains V1's actual destructive confirmation. It freezes selected identities once, invokes installed ExifTool on a private destination-directory stage, checks the native format and candidate identity/readability/size, adds structural/encoded-content witnesses for covered layouts, then replaces exactly the selected name. It never asks the host to deliver a file, installs tools, recursively scans directories, or creates user-visible backups. Each committed or failed file is checkpointed synchronously outside SDK rollback. Cancelled work has no automatic retry.

## Native writer and finite validation coverage

ExifTool is a genuine existing V1 dependency. It must already be executable in a V1 common location or PATH. As in the source, ExifTool prefers `/usr/local/bin`, `/opt/homebrew/bin`, `/usr/bin`, then the Windows Program Files location before a `-ver` PATH probe. FFprobe retains its separate PATH-first order. Resolver tests use only owned fake candidates. Missing-tool errors explain the requirement; this package never runs Homebrew, Chocolatey, Perl, Python, Node, or Bun as its engine. The installed `exiftool` command owns its own implementation/runtime. ExifTool 13.40 is the isolated QA version; it is not bundled or installed by this package.

V1's optional `file`, `ls`, `ffprobe`, `sips`, `mdls` and `afinfo` inspections remain optional. General stat fields use Rust/native APIs. On Windows, absent Unix inspection tools leave the same optional fields empty. FFprobe is restricted to the original media containers and file/pipe protocols; disguised playlists cannot load unrelated content. Spotlight fallback uses the frozen selected path.

There is no format-family allowlist before the native writer. Every selected ordinary file within the safety/resource policy flows through genuine installed ExifTool unless a covered structural check confirms malformed input or exceeds a work bound. Uncovered layouts are explicitly distinguished from malformed structures. V1's exact detailed argument sequence is attempted first; a nonzero/tool failure gets a **fresh** stage containing the frozen original before the source's simple fallback. Config loading is disabled and argv is passed without a shell. The selected original is never a helper argument.

A successful helper exit is not rejected merely because stderr contains a new warning. Bounded diagnostics are retained and the result is partial when warnings remain. Before and after writing, ExifTool reads only the private candidate and returns its native type; the canonical FileTypeExtension is compared because WebP's descriptive FileType legitimately changes from “Extended WEBP (lossless)” to “WEBP (lossless)” after metadata removal. The candidate must remain ordinary, single-linked, readable, nonempty and within 256 MiB. Its handle identity/content is checked after native inspection and before replacement. These are generic postconditions, **not** a universal independent encoded-payload proof. The native writer is trusted for uncovered layouts, as in V1.

| Input/layout | Additional evidence or observed native result |
| --- | --- |
| JPEG, PNG/APNG, GIF, WebP | Existing bounded marker/chunk/CRC/animation/decode-payload witnesses remain active for covered layouts. JPEG trailers outside the scan witness defer to the native writer. |
| TIFF | Ordinary classic strip/tile witness remains active; SubIFD, BigTIFF and other layouts outside it defer to ExifTool. Two authored root/SubIFD raster payloads were independently compared. Native IFD0 warnings remain partial. |
| MP4/MOV/M4A | Ordinary container timing/sample/codec/mdat witness remains active. HEIC/AVIF, unknown brands and fragmented layouts defer to ExifTool rather than an extension capability gate. The authored fragmented MP4 reaches both genuine attempts and receives ExifTool 13.40's actual write error, retaining original bytes/xattrs. |
| XMP, PSD, JP2, HEIC, AVIF | New production SDK regressions successfully remove authored private metadata with genuine ExifTool. Five raster cases preserve independent FFmpeg decoded output; XMP is metadata-only and remains nonempty/parseable. No in-product encoded-payload witness is claimed for these layouts. Explicitly selected sidecars are legitimate inputs; unselected sidecars remain byte-identical. |
| PDF | Existing incremental/base-revision witness remains active. PDF results are partial: prior metadata can remain recoverable. This is not PDF redaction. |
| WAV, FLAC, MP3, BMP | ExifTool 13.40's genuine read-only errors remain errors; original bytes/xattrs are retained. The three original audio negative cases remain mandatory. |
| Other ExifTool families | Use the same generic native-writer path. RAW, BigTIFF, JXL, archive/document and further codec variants have not been exhaustively exercised. Native support/errors and resource/safety bounds determine outcomes; finite QA is not universal format parity. |

The original twelve fixtures still exercise nine transformations with independent decoder parity plus three native read-only errors. Eight added fixtures exercise six successful transformations (five independently decoded rasters plus metadata-only XMP), BMP read-only and fragmented-MP4 write errors. All twenty original/new fixtures run through actual production SDK JSONL and genuine ExifTool, including the extracted ARM64 package. Successful generic writer tests do not substitute a stub writer.

V1 explicitly requests removal of ICC profiles, orientation/EXIF, GPS, IPTC, XMP, comments, maker notes and previews where ExifTool supports them. Encoded payload preservation does not promise identical colour/rotation after those tags are removed. Covered essential decode structures remain protected by the finite witness; other layouts trust the native writer.

## Filesystem results and limits

Only explicit absolute selected paths are accepted. Read-only info admits ordinary files and directories, with Directory type, Folder icon and all four frozen-path actions. Clear accepts ordinary files only. Clear directories, symlinks/reparse points, traversal, special files, duplicate selections, clear hardlinks, read-only files and special Unix permission bits are rejected. Windows additionally restricts paths to ordinary local drives, excluding UNC/device/ADS path syntax. Spaces, Unicode and newline names are preserved without shell interpolation.

macOS uses no-follow descriptor-relative traversal, child-only fchdir into a private stage, final identity/content rechecks, stage fsync and renameat. POSIX owner becomes the current effective user; original group/mode and creation time are prepared on the new inode; accessed/modified become now. Original xattr streams/resource fork are not copied. Actual remaining xattrs and removal errors are reported after commit. macOS may restore protected `com.apple.provenance`; the observed QA result is **partial**, not “all attributes cleared.” ACL inheritance follows fresh-file replacement; ExifTool 13.40's actual CopyFileAttrs also copies only mode/uid/gid, not extended attributes/ACLs. Unselected sidecars are untouched; an explicitly selected XMP sidecar is processed.

Windows uses reparse rejection and native file IDs, a private protected staging ACL, current-user SID ownership, selected DACL copying, native attribute preparation and handle-relative FileRenameInfoEx replacement. Original alternate streams are not copied. Final attributes are reported from the real handle. These implementations compile under Windows GNU but have not run on Windows; filesystem/ACL/owner/stream/GUI behavior and MSVC linking require native verification.

The last identity check and OS rename provide optimistic identity fencing; ordinary pathname replacement is not a portable atomic compare-and-swap against a hostile concurrent same-user renamer. File/data flushes do not establish exactly-once processing after a process or machine crash. Private stages may remain after a hard crash; no broad cleanup is attempted. Reads may advance access time even if the eventual operation fails.

Bounds: 16 MiB SDK frames; 128 sessions; 128 selected paths / 1 MiB total selection / 32 KiB per path; 256 MiB per clear file; one clear batch owns file buffers/helpers at a time; helper pipes 64 KiB each, metadata diagnostic/stdout 4 KiB; inspection/type/version probes 10 seconds each, clear writer 60 seconds per attempt; container parts 65,536, BMFF depth 16 and 1,000,000 aggregate offsets, TIFF 128 IFDs / 8,192 tags / bounded repeated-byte work. Application snapshots are at most 2,048 entries and 1 MiB including frozen action paths; oversized inputs fail rather than truncate. Search payloads are capped at 4 KiB. Up to 4,096 spent session IDs are retained per process (at most 256 bytes each), with 32 recent checkpoint reports / 8 MiB report budget. A fresh process is required at that identity budget; IDs are never silently recycled. SDK handler/reverse/output limits remain those of the pinned SDK. No extension timers, polling threads, detached tasks, actors or resource leases are created.

## Reproduce and review

Run every QA command through `scripts/qa-run.sh`; it redirects HOME/TMP/XDG/Cargo/npm caches into ignored `.qa` before downloads or tool inspection. Its compiler paths are this machine's existing toolchains; adjust those paths on another development machine without installing anything implicitly.

```sh
./scripts/qa-run.sh python3 scripts/snapshot.py
./scripts/qa-run.sh python3 scripts/prepare-qa.py
./scripts/qa-run.sh sh -c 'cd .qa/v1 && npm ci --ignore-scripts --legacy-peer-deps --no-audit --no-fund && npm test && npm run typecheck'
./scripts/qa-run.sh npm install --prefix .qa/schema --save-exact --ignore-scripts --no-audit --no-fund ajv@8.17.1 ajv-formats@3.0.1
./scripts/qa-run.sh python3 scripts/fixtures.py
./scripts/qa-run.sh python3 scripts/round2-fixtures.py
./scripts/qa-run.sh node scripts/v1-oracle.cjs
./scripts/qa-run.sh python3 scripts/host-patch.py
./scripts/qa-run.sh python3 scripts/rust-gates.py
./scripts/qa-run.sh cargo build --locked
./scripts/qa-run.sh python3 scripts/process-qa.py
./scripts/qa-run.sh node scripts/renderer-qa.cjs
./scripts/qa-run.sh node scripts/schema-qa.cjs
./scripts/qa-run.sh python3 scripts/fresh-build.py
./scripts/qa-run.sh python3 scripts/package.py
./scripts/qa-run.sh python3 scripts/round2-report.py
./scripts/qa-run.sh python3 scripts/review.py
./scripts/qa-run.sh python3 scripts/review.py --verify
```

Fixtures are authored/generated content with notices in `fixtures/generated-media.json`. Sips/cwebp/FFmpeg/Poppler and the QA-only downloaded ExifTool operate only on generated owned QA paths. Actual V1 code executes in a closed VM with filesystem, child-process, UI and network producers replaced **before** callbacks. Actual SDK pipes use owned host-service fakes, never the user's clipboard, Finder selection, dialogs or live APIs. Native host producers run with in-memory effect receivers.

See [integration](INTEGRATION.md), [provenance](PROVENANCE.md), [validation](VALIDATION.md), [final gate evidence](evidence/rust-gates.json), [process evidence](evidence/process-qa.json) and [review inventory](REVIEW-INVENTORY.json). Parent review must rehash staged bytes with `scripts/review.py --staged` before committing. No staging, commit, push, PR, release, installation or clone deletion was performed by this implementation task.
