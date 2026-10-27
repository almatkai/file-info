# File Info — ARM64 macOS development package

Canonical ID: com.tezbar.native.file-info. Version 2.0.0. Schema 3, protocol 1,
standalone Rust executable, SDK 0.1.1. This is an unpublished review artifact;
no catalog or acquisition entry is provided. Desktop verification remains not-run.

Commands: show-file-info and clear-file-metadata. The former reads one selected
ordinary file or read-only directory. The latter remains no-view, confirms once and processes only the
selected ordinary files using an already installed ExifTool. Nothing is installed
automatically. Copy/reveal/Open With require the generic host services.

Clear uses the genuine installed ExifTool for all its native writable families,
with V1's detailed arguments and fresh-copy fallback. No format allowlist blocks
XMP, HEIC/AVIF, RAW or other layouts just because a Rust witness is unavailable.
Finite extra payload witnesses cover ordinary JPEG/PNG/GIF/WebP/TIFF/MP4/MOV/PDF
layouts. Generic cases trust ExifTool with native type, identity, readability,
nonempty and size checks; universal payload/metadata proof is not claimed.
WAV/FLAC/MP3/BMP and the tested fragmented MP4 retain their genuine native errors.

Files are limited to 256 MiB each and selections to 128 files / 1 MiB of paths.
Helpers operate on a private destination-filesystem stage; selected originals
remain frozen until the candidate succeeds. Successful native warnings are
reported as partial without unnecessary fallback. Clear rejects directories,
symlinks, hardlinks, read-only files and special paths. View supports Directory
and Folder plus Copy Path/Reveal/Open With/Copy All Info without child enumeration.
ICC/orientation removal can change appearance. PDF metadata remains recoverable
and is not redaction; TIFF may retain IFD0 tags. Protected macOS provenance may
remain. A cancelled batch may already have committed files; never auto-retry.
Same-user POSIX pathname races are optimistically fenced, not an atomic CAS.
Process/machine crashes do not provide exactly-once durability.

The host needs the reviewed generic complete/bounded file-selection prerequisite.
Full source, supported-format adaptations, QA and immutable provenance are in
this candidate's repository v2 documentation. LICENSE, NOTICE and licenses/ carry
the authentic grants. ExifTool and QA media tools are not bundled.
