//! Deterministic archive packaging.
//!
//! Bundles a fixed set of on-disk files into a `.tar.gz` or `.zip` whose bytes
//! are a pure function of the entry list — identical inputs produce
//! byte-identical archives, so a checksum taken over the output is stable
//! across machines and runs. Determinism is achieved by pinning every
//! host-derived field an archive would otherwise capture: member modification
//! times are zeroed (the gzip header `mtime` and each `tar`/`zip` member time),
//! `tar` ownership is fixed to numeric `0:0` with empty owner/group names, and
//! only the caller-supplied member name and Unix mode are recorded.
//!
//! This is the generic filesystem primitive higher layers build release
//! artifacts on; it owns *how* files become an archive, not *which* files or
//! *what* the archive is named.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use rskit_errors::{AppError, AppResult, ErrorCode};
use zip::write::SimpleFileOptions;

/// A single file to place into an archive.
///
/// The archive records only the [`name`](Self::name) (the member path inside
/// the archive) and the [`mode`](Self::mode) (Unix permission bits); the
/// on-disk [`source`](Self::source)'s own timestamps and ownership are
/// deliberately not captured, keeping the archive deterministic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ArchiveEntry {
    /// Member name recorded inside the archive (e.g. `"toven"` or
    /// `"toven.exe"`). Always stored with forward slashes.
    pub name: String,
    /// Path to the file on disk whose contents are archived.
    pub source: PathBuf,
    /// Unix permission bits recorded for the member (e.g. `0o755`).
    pub mode: u32,
}

impl ArchiveEntry {
    /// Construct an archive entry from a member name, a source file, and a Unix
    /// mode.
    #[must_use]
    pub fn new(name: impl Into<String>, source: impl Into<PathBuf>, mode: u32) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            mode,
        }
    }
}

/// Fixed epoch for gzip/tar member times (`1970-01-01T00:00:00Z`).
const UNIX_EPOCH_SECS: u64 = 0;

/// Package `entries` into a deterministic gzip-compressed tar archive at `out`.
///
/// Each member's tar header pins `mtime = 0`, numeric owner `0:0`, and empty
/// owner/group names; the gzip wrapper pins its own `mtime = 0` and a fixed
/// operating-system byte. The result is a byte-stable function of the entry
/// list (names, contents, modes), so the same inputs always checksum the same.
///
/// # Errors
/// Returns an [`ErrorCode::InvalidInput`] error when a source file is missing
/// and an [`ErrorCode::Internal`] error on any other read/compress/write
/// failure, preserving the underlying I/O cause.
pub fn tar_gz(entries: &[ArchiveEntry], out: &Path) -> AppResult<()> {
    let file = create_out(out)?;
    // Pin the gzip header (mtime + OS byte) so the wrapper contributes nothing
    // host-derived to the output.
    let encoder = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.mode(tar::HeaderMode::Deterministic);
    for entry in entries {
        // Stream the source straight into the tar writer rather than buffering
        // it, so peak memory stays bounded regardless of member size.
        let (mut source, len) = open_source(&entry.source)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(len);
        header.set_mode(entry.mode);
        header.set_mtime(UNIX_EPOCH_SECS);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, &entry.name, &mut source)
            .map_err(|error| archive_io_error(out, "append tar member", error))?;
    }
    let encoder = builder
        .into_inner()
        .map_err(|error| archive_io_error(out, "finish tar stream", error))?;
    encoder
        .finish()
        .map_err(|error| archive_io_error(out, "finish gzip stream", error))?
        .flush()
        .map_err(|error| archive_io_error(out, "flush archive", error))?;
    Ok(())
}

/// Package `entries` into a deterministic zip archive at `out`.
///
/// Each member uses the DEFLATE method, a fixed modification time (the zip
/// epoch `1980-01-01`), and the caller-supplied Unix mode; nothing host-derived
/// is recorded, so identical inputs produce byte-identical output.
///
/// # Errors
/// Returns an [`ErrorCode::InvalidInput`] error when a source file is missing
/// and an [`ErrorCode::Internal`] error on any other read/compress/write
/// failure, preserving the underlying I/O cause.
pub fn zip(entries: &[ArchiveEntry], out: &Path) -> AppResult<()> {
    let file = create_out(out)?;
    let mut writer = zip::ZipWriter::new(file);
    // The zip epoch starts at 1980; pin every member there so no wall-clock
    // time leaks into the archive.
    let fixed_time = zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!(
                "cannot construct fixed member time for '{}': {error}",
                out.display()
            ),
        )
        .with_cause(error)
    })?;
    for entry in entries {
        // Stream the source into the zip writer instead of buffering it, and
        // enable Zip64 only for members that need it (>= 4 GiB), so large
        // release binaries still pack while small members stay compact. The
        // choice is a pure function of member size, preserving determinism.
        let (mut source, len) = open_source(&entry.source)?;
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(entry.mode)
            .last_modified_time(fixed_time)
            .large_file(len >= u64::from(u32::MAX));
        writer
            .start_file(&entry.name, options)
            .map_err(|error| archive_zip_error(out, "start zip member", error))?;
        std::io::copy(&mut source, &mut writer)
            .map_err(|error| archive_io_error(out, "write zip member", error))?;
    }
    writer
        .finish()
        .map_err(|error| archive_zip_error(out, "finish zip archive", error))?
        .flush()
        .map_err(|error| archive_io_error(out, "flush archive", error))?;
    Ok(())
}

/// Bounds applied when extracting an untrusted archive.
///
/// Extraction reads attacker-controlled input, so both the number of members
/// and the total number of uncompressed bytes written are capped to defuse
/// decompression bombs. Exceeding either bound fails the whole extraction
/// closed with [`ErrorCode::InvalidInput`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ExtractLimits {
    /// Maximum total uncompressed bytes written across all members.
    pub max_total_bytes: u64,
    /// Maximum number of members processed.
    pub max_entries: usize,
}

impl ExtractLimits {
    /// Default cap on total uncompressed output (512 MiB).
    pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
    /// Default cap on the number of archive members.
    pub const DEFAULT_MAX_ENTRIES: usize = 4096;

    /// Set the maximum total uncompressed bytes written across all members.
    #[must_use]
    pub const fn with_max_total_bytes(mut self, max_total_bytes: u64) -> Self {
        self.max_total_bytes = max_total_bytes;
        self
    }

    /// Set the maximum number of members processed.
    #[must_use]
    pub const fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.max_entries = max_entries;
        self
    }
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: Self::DEFAULT_MAX_TOTAL_BYTES,
            max_entries: Self::DEFAULT_MAX_ENTRIES,
        }
    }
}

/// The classification of an archive member's recorded path relative to `dest`.
enum MemberPath {
    /// A bare directory marker (empty relative path) — nothing to extract.
    DirectoryMarker,
    /// A safe target path confined under `dest`.
    Safe(PathBuf),
}

/// Extract every member of the gzip-compressed tar archive at `archive` into
/// `dest`, returning the extracted file paths (in archive order).
///
/// The extraction is hardened against hostile archives: a member whose path is
/// absolute or escapes `dest` via `..` is rejected (tar-slip), symlink and hard
/// link members are rejected outright (a symlink could redirect a later write
/// outside `dest`), each member's materialised path is re-checked to resolve
/// within `dest` so a symlink already present in the destination tree cannot
/// redirect a write outside it, and the member count and total uncompressed
/// size are bounded by `limits` (decompression bomb). Parent directories are
/// created as needed and recorded Unix modes are re-applied on Unix.
///
/// # Errors
/// Returns an [`ErrorCode::InvalidInput`] error when the archive is missing, a
/// member escapes `dest`, a member is a link, or `limits` are exceeded, and an
/// [`ErrorCode::Internal`] error on any other read/decompress/write failure,
/// preserving the underlying I/O cause.
pub fn extract_tar_gz(
    archive: &Path,
    dest: &Path,
    limits: ExtractLimits,
) -> AppResult<Vec<PathBuf>> {
    let file = open_archive(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut reader = tar::Archive::new(decoder);
    let entries = reader
        .entries()
        .map_err(|error| extract_io_error(archive, "read tar entries", error))?;
    let root = extraction_root(archive, dest)?;
    let mut extracted = Vec::new();
    let mut written = 0_u64;
    for (index, entry) in entries.enumerate() {
        if index >= limits.max_entries {
            return Err(too_many_entries_error(archive, limits.max_entries));
        }
        let mut member =
            entry.map_err(|error| extract_io_error(archive, "read tar member", error))?;
        let entry_type = member.header().entry_type();
        let name = member
            .path()
            .map_err(|error| extract_io_error(archive, "read tar member path", error))?
            .to_path_buf();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(unsafe_link_error(archive, &name.display().to_string()));
        }
        let target = match safe_member_path(archive, dest, &name)? {
            MemberPath::DirectoryMarker => continue,
            MemberPath::Safe(target) => target,
        };
        let mode = member.header().mode().ok();
        if entry_type.is_dir() {
            create_all_dir(&target)?;
            ensure_within_root(archive, &root, &target, &name)?;
            apply_mode(&target, mode)?;
            continue;
        }
        create_parent(&target)?;
        ensure_parent_within_root(archive, &root, &target, &name)?;
        ensure_target_not_symlink(archive, &target, &name)?;
        write_member_bounded(archive, &target, &mut member, limits, &mut written)?;
        apply_mode(&target, mode)?;
        extracted.push(target);
    }
    Ok(extracted)
}

/// Extract every member of the zip archive at `archive` into `dest`, returning
/// the extracted file paths (in archive order).
///
/// Hardened identically to [`extract_tar_gz`]: escaping members are rejected
/// (zip-slip), symlink members are rejected, each member's materialised path is
/// re-checked to resolve within `dest` so a pre-existing symlink in the
/// destination tree cannot redirect a write outside it, and `limits` bound the
/// member count and total uncompressed size. Recorded Unix modes are re-applied
/// on Unix.
///
/// # Errors
/// Returns an [`ErrorCode::InvalidInput`] error when the archive is missing, a
/// member escapes `dest`, a member is a symlink, or `limits` are exceeded, and
/// an [`ErrorCode::Internal`] error on any other read/decompress/write failure,
/// preserving the underlying I/O cause.
pub fn extract_zip(archive: &Path, dest: &Path, limits: ExtractLimits) -> AppResult<Vec<PathBuf>> {
    let file = open_archive(archive)?;
    let mut reader = zip::ZipArchive::new(file)
        .map_err(|error| extract_zip_error(archive, "open zip archive", error))?;
    if reader.len() > limits.max_entries {
        return Err(too_many_entries_error(archive, limits.max_entries));
    }
    let root = extraction_root(archive, dest)?;
    let mut extracted = Vec::new();
    let mut written = 0_u64;
    for index in 0..reader.len() {
        let mut member = reader
            .by_index(index)
            .map_err(|error| extract_zip_error(archive, "read zip member", error))?;
        if member.is_symlink() {
            return Err(unsafe_link_error(archive, member.name()));
        }
        let Some(name) = member.enclosed_name() else {
            return Err(escape_error(archive, member.name()));
        };
        let target = match safe_member_path(archive, dest, &name)? {
            MemberPath::DirectoryMarker => continue,
            MemberPath::Safe(target) => target,
        };
        let mode = member.unix_mode();
        if member.is_dir() {
            create_all_dir(&target)?;
            ensure_within_root(archive, &root, &target, &name)?;
            apply_mode(&target, mode)?;
            continue;
        }
        create_parent(&target)?;
        ensure_parent_within_root(archive, &root, &target, &name)?;
        ensure_target_not_symlink(archive, &target, &name)?;
        write_member_bounded(archive, &target, &mut member, limits, &mut written)?;
        apply_mode(&target, mode)?;
        extracted.push(target);
    }
    Ok(extracted)
}

/// Resolve a member name against `dest`. Returns [`MemberPath::DirectoryMarker`]
/// for a bare directory marker (empty relative path) and an error for any path
/// that escapes `dest` (absolute, prefix, or `..`), so a hostile archive cannot
/// write outside the destination.
fn safe_member_path(archive: &Path, dest: &Path, name: &Path) -> AppResult<MemberPath> {
    let mut target = dest.to_path_buf();
    let mut components = 0usize;
    for component in name.components() {
        match component {
            std::path::Component::Normal(part) => {
                target.push(part);
                components += 1;
            }
            // A `.` segment (e.g. `./foo`) is harmless — skip it without
            // counting it as a real component.
            std::path::Component::CurDir => {}
            // Reject anything that could escape `dest`: root, prefix, `..`.
            _ => return Err(escape_error(archive, &name.display().to_string())),
        }
    }
    if components == 0 {
        Ok(MemberPath::DirectoryMarker)
    } else {
        Ok(MemberPath::Safe(target))
    }
}

/// Write a single member's contents to `target`, streaming through a reader
/// capped at the remaining byte budget so a decompression bomb cannot exceed
/// `limits.max_total_bytes`. Updates `written` with the bytes committed.
fn write_member_bounded<R: Read>(
    archive: &Path,
    target: &Path,
    reader: &mut R,
    limits: ExtractLimits,
    written: &mut u64,
) -> AppResult<()> {
    let remaining = limits.max_total_bytes.saturating_sub(*written);
    let mut out =
        File::create(target).map_err(|error| extract_io_error(archive, "create member", error))?;
    // Read one byte past the budget so an overrun is detected without trusting
    // the member's declared size.
    let mut limited = reader.by_ref().take(remaining.saturating_add(1));
    let copied = std::io::copy(&mut limited, &mut out)
        .map_err(|error| extract_io_error(archive, "write member", error))?;
    if copied > remaining {
        return Err(oversize_error(archive, limits.max_total_bytes));
    }
    *written += copied;
    Ok(())
}

fn open_archive(archive: &Path) -> AppResult<File> {
    File::open(archive).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::InvalidInput
        } else {
            ErrorCode::Internal
        };
        AppError::new(
            code,
            format!("cannot open archive '{}': {error}", archive.display()),
        )
        .with_cause(error)
    })
}

fn create_parent(target: &Path) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        create_all_dir(parent)?;
    }
    Ok(())
}

/// Ensure `dest` exists and resolve it to a canonical containment root.
///
/// Members are confined to this resolved root, so a symlink already present
/// under `dest` cannot redirect a write outside it (a symlink component
/// resolves to a path that fails the containment check).
fn extraction_root(archive: &Path, dest: &Path) -> AppResult<PathBuf> {
    create_all_dir(dest)?;
    std::fs::canonicalize(dest)
        .map_err(|error| extract_io_error(archive, "resolve destination", error))
}

/// Reject a member whose materialised `dir` resolves outside `root`, defending
/// against pre-existing symlinks in the destination tree.
fn ensure_within_root(archive: &Path, root: &Path, dir: &Path, member: &Path) -> AppResult<()> {
    let resolved = std::fs::canonicalize(dir)
        .map_err(|error| extract_io_error(archive, "resolve member path", error))?;
    if resolved.starts_with(root) {
        Ok(())
    } else {
        Err(escape_error(archive, &member.display().to_string()))
    }
}

/// Reject a member whose parent directory resolves outside `root` before its
/// contents are written.
fn ensure_parent_within_root(
    archive: &Path,
    root: &Path,
    target: &Path,
    member: &Path,
) -> AppResult<()> {
    target.parent().map_or_else(
        || Ok(()),
        |parent| ensure_within_root(archive, root, parent, member),
    )
}

/// Reject a member whose final materialised path already exists as a symlink.
///
/// [`ensure_parent_within_root`] only confines the *parent* directory, but
/// [`File::create`] follows a symlink at the final path component. A symlink
/// already present in the destination tree at `target` (e.g. `dest/payload`
/// pointing at `/etc/passwd`) would otherwise let a regular-file member
/// overwrite the link's target outside `dest`. Rejecting an existing final
/// symlink upholds the extraction guarantee that a pre-existing symlink in the
/// destination tree cannot redirect a write outside it.
fn ensure_target_not_symlink(archive: &Path, target: &Path, member: &Path) -> AppResult<()> {
    match std::fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(escape_error(archive, &member.display().to_string()))
        }
        _ => Ok(()),
    }
}

fn create_all_dir(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("cannot create directory '{}': {error}", dir.display()),
        )
        .with_cause(error)
    })
}

#[cfg(unix)]
fn apply_mode(target: &Path, mode: Option<u32>) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(mode) = mode {
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode)).map_err(
            |error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("cannot set permissions on '{}': {error}", target.display()),
                )
                .with_cause(error)
            },
        )?;
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn apply_mode(_target: &Path, _mode: Option<u32>) -> AppResult<()> {
    Ok(())
}

fn escape_error(archive: &Path, member: &str) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "archive '{}' contains an unsafe member path '{member}'",
            archive.display()
        ),
    )
}

fn unsafe_link_error(archive: &Path, member: &str) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "archive '{}' contains a link member '{member}', which is not permitted",
            archive.display()
        ),
    )
}

fn oversize_error(archive: &Path, max_total_bytes: u64) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "archive '{}' exceeds the extraction limit of {max_total_bytes} uncompressed bytes",
            archive.display()
        ),
    )
}

fn too_many_entries_error(archive: &Path, max_entries: usize) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "archive '{}' exceeds the extraction limit of {max_entries} members",
            archive.display()
        ),
    )
}

fn extract_io_error(archive: &Path, action: &str, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("cannot {action} for '{}': {error}", archive.display()),
    )
    .with_cause(error)
}

fn extract_zip_error(archive: &Path, action: &str, error: zip::result::ZipError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("cannot {action} for '{}': {error}", archive.display()),
    )
    .with_cause(error)
}

/// Open a source file for streaming into an archive, returning the open handle
/// and its length, and mapping a missing file to an actionable input error.
fn open_source(source: &Path) -> AppResult<(File, u64)> {
    let file = File::open(source).map_err(|error| open_source_error(source, error))?;
    let len = file
        .metadata()
        .map_err(|error| read_source_error(source, error))?
        .len();
    Ok((file, len))
}

/// Create (truncating) the archive output file.
fn create_out(out: &Path) -> AppResult<File> {
    File::create(out).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("cannot create archive '{}': {error}", out.display()),
        )
        .with_cause(error)
    })
}

fn open_source_error(source: &Path, error: std::io::Error) -> AppError {
    let code = if error.kind() == std::io::ErrorKind::NotFound {
        ErrorCode::InvalidInput
    } else {
        ErrorCode::Internal
    };
    AppError::new(
        code,
        format!("cannot open archive source '{}': {error}", source.display()),
    )
    .with_cause(error)
}

fn read_source_error(source: &Path, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("cannot read archive source '{}': {error}", source.display()),
    )
    .with_cause(error)
}

fn archive_io_error(out: &Path, action: &str, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("cannot {action} for '{}': {error}", out.display()),
    )
    .with_cause(error)
}

fn archive_zip_error(out: &Path, action: &str, error: zip::result::ZipError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("cannot {action} for '{}': {error}", out.display()),
    )
    .with_cause(error)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use tempfile::tempdir;

    use super::{ArchiveEntry, ExtractLimits, extract_tar_gz, extract_zip, tar_gz, zip};

    fn write_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn tar_gz_is_byte_stable_for_identical_inputs() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven", b"binary-bytes");
        let entries = vec![ArchiveEntry::new("toven", &source, 0o755)];

        let first = dir.path().join("first.tar.gz");
        let second = dir.path().join("second.tar.gz");
        tar_gz(&entries, &first).unwrap();
        tar_gz(&entries, &second).unwrap();

        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap(),
            "identical inputs must produce byte-identical tar.gz"
        );
    }

    #[test]
    fn tar_gz_records_member_name_mode_and_contents() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven", b"hello-tar");
        let out = dir.path().join("out.tar.gz");
        tar_gz(&[ArchiveEntry::new("toven", &source, 0o755)], &out).unwrap();

        let decoder = flate2::read::GzDecoder::new(std::fs::File::open(&out).unwrap());
        let mut archive = tar::Archive::new(decoder);
        let mut members = archive.entries().unwrap();
        let mut member = members.next().unwrap().unwrap();
        assert_eq!(member.path().unwrap().to_str().unwrap(), "toven");
        assert_eq!(member.header().mode().unwrap() & 0o777, 0o755);
        assert_eq!(member.header().uid().unwrap(), 0);
        assert_eq!(member.header().gid().unwrap(), 0);
        assert_eq!(member.header().mtime().unwrap(), 0);
        let mut contents = Vec::new();
        member.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, b"hello-tar");
        assert!(members.next().is_none(), "exactly one member expected");
    }

    #[test]
    fn zip_is_byte_stable_for_identical_inputs() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven.exe", b"win-bytes");
        let entries = vec![ArchiveEntry::new("toven.exe", &source, 0o755)];

        let first = dir.path().join("first.zip");
        let second = dir.path().join("second.zip");
        zip(&entries, &first).unwrap();
        zip(&entries, &second).unwrap();

        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&second).unwrap(),
            "identical inputs must produce byte-identical zip"
        );
    }

    #[test]
    fn zip_records_member_name_and_contents() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven.exe", b"hello-zip");
        let out = dir.path().join("out.zip");
        zip(&[ArchiveEntry::new("toven.exe", &source, 0o755)], &out).unwrap();

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(archive.len(), 1);
        let mut member = archive.by_index(0).unwrap();
        assert_eq!(member.name(), "toven.exe");
        let mut contents = Vec::new();
        member.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, b"hello-zip");
    }

    #[test]
    fn tar_gz_fails_closed_on_missing_source() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("absent");
        let out = dir.path().join("out.tar.gz");
        let error = tar_gz(&[ArchiveEntry::new("toven", &missing, 0o755)], &out)
            .expect_err("a missing source must fail closed");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn zip_fails_closed_on_missing_source() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("absent");
        let out = dir.path().join("out.zip");
        let error = zip(&[ArchiveEntry::new("toven.exe", &missing, 0o755)], &out)
            .expect_err("a missing source must fail closed");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn tar_gz_round_trips_through_extraction() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven", b"binary-bytes");
        let out = dir.path().join("out.tar.gz");
        tar_gz(&[ArchiveEntry::new("toven", &source, 0o755)], &out).unwrap();

        let dest = dir.path().join("extract-tar");
        let extracted = extract_tar_gz(&out, &dest, ExtractLimits::default()).unwrap();
        assert_eq!(extracted, vec![dest.join("toven")]);
        assert_eq!(std::fs::read(dest.join("toven")).unwrap(), b"binary-bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dest.join("toven"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    #[test]
    fn zip_round_trips_through_extraction() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "toven.exe", b"win-bytes");
        let out = dir.path().join("out.zip");
        zip(&[ArchiveEntry::new("toven.exe", &source, 0o755)], &out).unwrap();

        let dest = dir.path().join("extract-zip");
        let extracted = extract_zip(&out, &dest, ExtractLimits::default()).unwrap();
        assert_eq!(extracted, vec![dest.join("toven.exe")]);
        assert_eq!(std::fs::read(dest.join("toven.exe")).unwrap(), b"win-bytes");
    }

    #[test]
    fn extract_tar_gz_fails_closed_on_missing_archive() {
        let dir = tempdir().unwrap();
        let error = extract_tar_gz(
            &dir.path().join("absent.tar.gz"),
            dir.path(),
            ExtractLimits::default(),
        )
        .expect_err("a missing archive must fail closed");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    /// Build a gzip-compressed tar containing a single member with an arbitrary
    /// header, bypassing the packaging API so hostile inputs can be exercised.
    fn write_raw_tar_gz(
        out: &std::path::Path,
        configure: impl FnOnce(&mut tar::Header),
        name: &str,
        body: &[u8],
    ) {
        let file = std::fs::File::create(out).unwrap();
        let encoder = flate2::GzBuilder::new().write(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        configure(&mut header);
        // Inject the member name directly into the GNU header, bypassing the
        // high-level writer's refusal to store paths containing `..`.
        {
            let gnu = header.as_gnu_mut().expect("gnu header");
            let bytes = name.as_bytes();
            gnu.name[..bytes.len()].copy_from_slice(bytes);
        }
        header.set_cksum();
        builder.append(&header, body).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn extract_tar_gz_rejects_a_traversal_member() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("evil.tar.gz");
        write_raw_tar_gz(
            &out,
            |header| header.set_entry_type(tar::EntryType::Regular),
            "../escape",
            b"pwned",
        );
        let dest = dir.path().join("dest");
        let error = extract_tar_gz(&out, &dest, ExtractLimits::default())
            .expect_err("a traversal member must be rejected");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        assert!(
            !dir.path().join("escape").exists(),
            "nothing may escape dest"
        );
    }

    #[test]
    fn extract_tar_gz_rejects_a_symlink_member() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("evil.tar.gz");
        write_raw_tar_gz(
            &out,
            |header| {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("/etc").unwrap();
                header.set_size(0);
            },
            "link",
            b"",
        );
        let dest = dir.path().join("dest");
        let error = extract_tar_gz(&out, &dest, ExtractLimits::default())
            .expect_err("a symlink member must be rejected");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn extract_tar_gz_bounds_total_uncompressed_size() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "big", &vec![0_u8; 4096]);
        let out = dir.path().join("big.tar.gz");
        tar_gz(&[ArchiveEntry::new("big", &source, 0o644)], &out).unwrap();

        let dest = dir.path().join("dest");
        let limits = ExtractLimits::default().with_max_total_bytes(1024);
        let error =
            extract_tar_gz(&out, &dest, limits).expect_err("an oversized member must fail closed");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn extract_tar_gz_bounds_the_member_count() {
        let dir = tempdir().unwrap();
        let source = write_file(dir.path(), "a", b"x");
        let out = dir.path().join("many.tar.gz");
        tar_gz(
            &[
                ArchiveEntry::new("a", &source, 0o644),
                ArchiveEntry::new("b", &source, 0o644),
            ],
            &out,
        )
        .unwrap();

        let dest = dir.path().join("dest");
        let limits = ExtractLimits::default().with_max_entries(1);
        let error =
            extract_tar_gz(&out, &dest, limits).expect_err("too many members must fail closed");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn extract_tar_gz_accepts_a_current_dir_prefixed_member() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("dot.tar.gz");
        write_raw_tar_gz(
            &out,
            |header| header.set_entry_type(tar::EntryType::Regular),
            "./toven",
            b"dot-bytes",
        );
        let dest = dir.path().join("dest");
        let extracted = extract_tar_gz(&out, &dest, ExtractLimits::default())
            .expect("a `./` prefix is harmless and must extract");
        assert_eq!(extracted, vec![dest.join("toven")]);
        assert_eq!(std::fs::read(dest.join("toven")).unwrap(), b"dot-bytes");
    }

    #[cfg(unix)]
    #[test]
    fn extract_tar_gz_applies_directory_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let out = dir.path().join("dir.tar.gz");
        write_raw_tar_gz(
            &out,
            |header| {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_mode(0o750);
                header.set_size(0);
            },
            "nested/",
            b"",
        );
        let dest = dir.path().join("dest");
        extract_tar_gz(&out, &dest, ExtractLimits::default()).unwrap();
        let mode = std::fs::metadata(dest.join("nested"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o750, "directory mode must be re-applied");
    }

    #[cfg(unix)]
    #[test]
    fn extract_tar_gz_rejects_a_symlink_in_the_destination_tree() {
        let dir = tempdir().unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let out = dir.path().join("via-link.tar.gz");
        write_raw_tar_gz(
            &out,
            |header| header.set_entry_type(tar::EntryType::Regular),
            "link/escape",
            b"pwned",
        );
        let dest = dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();
        // Pre-place a symlink under dest that redirects `link/` outside it.
        std::os::unix::fs::symlink(&outside, dest.join("link")).unwrap();

        let error = extract_tar_gz(&out, &dest, ExtractLimits::default())
            .expect_err("a pre-existing symlink in dest must be rejected");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        assert!(
            !outside.join("escape").exists(),
            "nothing may be written through the symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_tar_gz_rejects_a_final_symlink_target_in_the_destination_tree() {
        let dir = tempdir().unwrap();
        let secret = dir.path().join("secret");
        std::fs::write(&secret, b"original").unwrap();
        let out = dir.path().join("via-final-link.tar.gz");
        // A regular-file member whose name matches a pre-existing symlink at the
        // final path component (not a parent directory component).
        write_raw_tar_gz(
            &out,
            |header| header.set_entry_type(tar::EntryType::Regular),
            "payload",
            b"pwned",
        );
        let dest = dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();
        std::os::unix::fs::symlink(&secret, dest.join("payload")).unwrap();

        let error = extract_tar_gz(&out, &dest, ExtractLimits::default())
            .expect_err("a pre-existing final symlink in dest must be rejected");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        assert_eq!(
            std::fs::read(&secret).unwrap(),
            b"original",
            "the symlink target outside dest must not be overwritten"
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_zip_rejects_a_final_symlink_target_in_the_destination_tree() {
        let dir = tempdir().unwrap();
        let secret = dir.path().join("secret");
        std::fs::write(&secret, b"original").unwrap();
        let out = dir.path().join("via-final-link.zip");
        {
            let file = std::fs::File::create(&out).unwrap();
            let mut writer = ::zip::ZipWriter::new(file);
            writer
                .start_file("payload", ::zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, b"pwned").unwrap();
            writer.finish().unwrap();
        }
        let dest = dir.path().join("dest");
        std::fs::create_dir(&dest).unwrap();
        std::os::unix::fs::symlink(&secret, dest.join("payload")).unwrap();

        let error = extract_zip(&out, &dest, ExtractLimits::default())
            .expect_err("a pre-existing final symlink in dest must be rejected");
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        assert_eq!(
            std::fs::read(&secret).unwrap(),
            b"original",
            "the symlink target outside dest must not be overwritten"
        );
    }
}
