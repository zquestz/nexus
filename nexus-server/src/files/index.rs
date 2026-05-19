//! File index: build a CSV index of the file area (`walkdir`), search it with
//! `grep-searcher`, and swap it atomically via temp file + rename.
//!
//! ## Index Format
//!
//! The index is a CSV file with the following columns:
//! - path: Full path relative to file root (e.g., "/shared/Documents/report.pdf")
//! - name: Filename only (e.g., "report.pdf")
//! - size: File size in bytes (0 for directories)
//! - modified: Last modified time as Unix timestamp
//! - is_directory: "1" for directories, "0" for files
//!
//! ## Thread Safety
//!
//! The index state (`dirty`, `reindexing`) uses `AtomicBool` for lock-free access.
//! Only one reindex can run at a time - concurrent requests are ignored.

use tracing::{error, info};

use crate::constants::*;

use std::fs::{self, File};
#[cfg(test)]
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use csv::{ReaderBuilder, WriterBuilder};
use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::UTF8;
use walkdir::WalkDir;

use nexus_common::protocol::FileSearchResult;
use nexus_common::validators::extract_search_terms;

use super::path::is_hidden_name;

pub const MAX_SEARCH_RESULTS: usize = 100;

const INDEX_FILE_NAME: &str = "files.idx";

/// Temporary index file written then renamed over `INDEX_FILE_NAME` (atomic swap).
const INDEX_TEMP_FILE_NAME: &str = "files.idx.tmp";

pub struct FileIndex {
    index_path: PathBuf,
    temp_path: PathBuf,
    file_root: PathBuf,
    dirty: AtomicBool,
    reindexing: AtomicBool,
    /// Unix timestamp (seconds) of the last successful rebuild; 0 means never.
    last_rebuild_secs: AtomicU64,
}

impl FileIndex {
    /// The index file is stored in the data directory (alongside the
    /// database), not in the file area — so it isn't moved by `--file-root`.
    pub fn new(data_dir: &Path, file_root: &Path) -> Self {
        Self {
            index_path: data_dir.join(INDEX_FILE_NAME),
            temp_path: data_dir.join(INDEX_TEMP_FILE_NAME),
            file_root: file_root.to_path_buf(),
            dirty: AtomicBool::new(false),
            reindexing: AtomicBool::new(false),
            last_rebuild_secs: AtomicU64::new(0),
        }
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    pub fn is_reindexing(&self) -> bool {
        self.reindexing.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn exists(&self) -> bool {
        self.index_path.exists()
    }

    /// Whether more than `max_age` has passed since the last successful rebuild.
    /// Catches external filesystem changes that never went through a handler.
    pub fn is_stale(&self, max_age: Duration) -> bool {
        let last = self.last_rebuild_secs.load(Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(last) >= max_age.as_secs()
    }

    /// Returns `true` if a reindex was started, `false` if one is already
    /// running. Non-blocking — the actual reindex runs in the background.
    pub fn trigger_reindex(self: &Arc<Self>) -> bool {
        // Single-reindex guard: a losing swap leaves dirty set so we retry later.
        if self.reindexing.swap(true, Ordering::SeqCst) {
            return false;
        }

        // Clear dirty only after winning the race to start.
        self.dirty.store(false, Ordering::SeqCst);

        let index = Arc::clone(self);

        // build_index does synchronous I/O, so keep it off the async runtime.
        tokio::task::spawn_blocking(move || {
            match index.build_index() {
                Ok(count) => {
                    info!(entries = count, "{}", LOG_FILE_INDEX_REBUILT);
                }
                Err(e) => {
                    error!(err = %e, "{}", LOG_FILE_INDEX_BUILD_FAILED);
                    // Re-mark dirty so the failed build is retried.
                    index.mark_dirty();
                }
            }

            index.reindexing.store(false, Ordering::SeqCst);
        });

        true
    }

    /// Walks the file area into a temp file, then atomically swaps it into
    /// place. Returns the number of entries indexed.
    fn build_index(&self) -> Result<usize, String> {
        let file = File::create(&self.temp_path)
            .map_err(|e| format!("{}{}", ERR_FILE_INDEX_CREATE_TEMP, e))?;

        // The index lists the whole file tree, so keep it owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&self.temp_path, perms)
                .map_err(|e| format!("{}{}", ERR_FILE_INDEX_SET_PERMISSIONS, e))?;
        }

        let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);

        let mut count = 0;

        // Skip hidden entries and their subtrees: dotfiles, NAS metadata
        // (@eaDir, @tmp), NAS recycle (#recycle).
        for entry in WalkDir::new(&self.file_root)
            .min_depth(1) // Skip the root itself
            .follow_links(true) // Follow symlinks (admin-trusted)
            .into_iter()
            .filter_entry(|e| {
                e.file_name()
                    .to_str()
                    .is_none_or(|name| !is_hidden_name(name))
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // metadata (not symlink_metadata) so we record the symlink target.
            let metadata = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Size is 0 for directories.
            let size = if metadata.is_file() {
                metadata.len()
            } else {
                0
            };

            let modified = metadata
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let relative_path = match path.strip_prefix(&self.file_root) {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Normalize to forward slashes with a leading / for the index path.
            let path_str = format!("/{}", relative_path.to_string_lossy().replace('\\', "/"));

            let name = entry.file_name().to_string_lossy().into_owned();
            let size_str = size.to_string();
            let modified_str = modified.to_string();
            let is_dir_str = if metadata.is_dir() { "1" } else { "0" };

            // CSV columns: path, name, size, modified, is_directory.
            writer
                .write_record([
                    path_str.as_str(),
                    name.as_str(),
                    size_str.as_str(),
                    modified_str.as_str(),
                    is_dir_str,
                ])
                .map_err(|e| format!("{}{}", ERR_FILE_INDEX_WRITE_ENTRY, e))?;

            count += 1;
        }

        writer
            .flush()
            .map_err(|e| format!("{}{}", ERR_FILE_INDEX_FLUSH, e))?;
        drop(writer);

        // Atomic swap so readers never observe a half-written index.
        fs::rename(&self.temp_path, &self.index_path)
            .map_err(|e| format!("{}{}", ERR_FILE_INDEX_SWAP, e))?;

        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.last_rebuild_secs.store(now_secs, Ordering::Relaxed);

        Ok(count)
    }

    /// Returns up to `MAX_SEARCH_RESULTS` entries, optionally restricted to
    /// `area_prefix`. A corrupted index is deleted and marked dirty for
    /// rebuild, returning empty results.
    pub fn search(
        &self,
        query: &str,
        area_prefix: Option<&str>,
    ) -> Result<Vec<FileSearchResult>, String> {
        if !self.index_path.exists() {
            return Ok(vec![]);
        }

        // 3+ char terms, plus 2-char terms when there's a primary term;
        // single-character terms are ignored.
        let terms = extract_search_terms(query);

        if terms.is_empty() {
            return Ok(vec![]);
        }

        // First term drives the grep pass (fastest filter); the rest are
        // applied as a case-insensitive AND filter below.
        let first_term = regex::escape(terms[0]);
        let pattern = format!("(?i){}", first_term);
        let matcher = RegexMatcher::new(&pattern)
            .map_err(|e| format!("{}{}", ERR_FILE_INDEX_SEARCH_PATTERN, e))?;

        let remaining_terms: Vec<String> = terms[1..].iter().map(|t| t.to_lowercase()).collect();

        let mut results = Vec::new();

        let search_result = Searcher::new().search_path(
            &matcher,
            &self.index_path,
            UTF8(|_line_num, line| {
                if results.len() >= MAX_SEARCH_RESULTS {
                    return Ok(false);
                }

                if let Some(entry) = parse_csv_line(line.trim()) {
                    if let Some(prefix) = area_prefix
                        && !entry.path.starts_with(prefix)
                    {
                        return Ok(true);
                    }

                    // ALL remaining terms must match (AND), checked against the
                    // full path so filename matches count too.
                    let path_lower = entry.path.to_lowercase();
                    let all_match = remaining_terms.iter().all(|term| path_lower.contains(term));
                    if !all_match {
                        return Ok(true);
                    }

                    results.push(entry);
                }

                Ok(true)
            }),
        );

        if let Err(e) = search_result {
            // A search error means the index is likely corrupt: drop it and
            // mark dirty so the next trigger rebuilds from scratch.
            error!(err = %e, "{}", LOG_FILE_INDEX_SEARCH_FAILED);
            if let Err(del_err) = fs::remove_file(&self.index_path) {
                error!(err = %del_err, "{}", LOG_FILE_INDEX_DELETE_FAILED);
            }
            self.mark_dirty();
            return Ok(vec![]);
        }

        Ok(results)
    }
}

/// Parse one CSV index line; returns `None` for malformed lines (fewer than 5
/// fields or unparseable numerics) so a few bad rows don't fail the search.
fn parse_csv_line(line: &str) -> Option<FileSearchResult> {
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(line.as_bytes());

    let mut record = csv::StringRecord::new();
    if reader.read_record(&mut record).ok()? && record.len() >= 5 {
        let path = record.get(0)?.to_string();
        let name = record.get(1)?.to_string();
        let size = record.get(2)?.parse().ok()?;
        let modified = record.get(3)?.parse().ok()?;
        let is_directory = record.get(4)? == "1";

        Some(FileSearchResult {
            path,
            name,
            size,
            modified,
            is_directory,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_csv_line_simple() {
        let result = parse_csv_line("/shared/docs/report.pdf,report.pdf,12345,1704567890,0");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.path, "/shared/docs/report.pdf");
        assert_eq!(entry.name, "report.pdf");
        assert_eq!(entry.size, 12345);
        assert_eq!(entry.modified, 1704567890);
        assert!(!entry.is_directory);
    }

    #[test]
    fn test_parse_csv_line_directory() {
        let result = parse_csv_line("/shared/docs,docs,0,1704567890,1");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert!(entry.is_directory);
        assert_eq!(entry.size, 0);
    }

    #[test]
    fn test_parse_csv_line_with_comma_in_name() {
        // CSV crate handles quoted fields properly
        let result =
            parse_csv_line("\"/shared/file,with,commas.txt\",\"file,with,commas.txt\",100,0,0");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.path, "/shared/file,with,commas.txt");
        assert_eq!(entry.name, "file,with,commas.txt");
    }

    #[test]
    fn test_parse_csv_line_with_quotes_in_name() {
        // CSV crate handles escaped quotes (doubled quotes) properly
        let result =
            parse_csv_line("\"/shared/file\"\"quoted\"\".txt\",\"file\"\"quoted\"\".txt\",100,0,0");
        assert!(result.is_some());
        let entry = result.unwrap();
        assert_eq!(entry.path, "/shared/file\"quoted\".txt");
        assert_eq!(entry.name, "file\"quoted\".txt");
    }

    #[test]
    fn test_file_index_new() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        let index = FileIndex::new(&data_dir, &file_root);

        assert!(!index.is_dirty());
        assert!(!index.is_reindexing());
        assert!(!index.exists());
    }

    #[test]
    fn test_file_index_mark_dirty() {
        let temp_dir = TempDir::new().unwrap();
        let index = FileIndex::new(temp_dir.path(), temp_dir.path());

        assert!(!index.is_dirty());
        index.mark_dirty();
        assert!(index.is_dirty());
    }

    #[test]
    fn test_is_stale() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        let index = FileIndex::new(&data_dir, &file_root);

        // Never rebuilt → stale under any max_age.
        assert!(index.is_stale(Duration::from_secs(60)));
        assert!(index.is_stale(Duration::from_secs(86400)));

        // After a successful rebuild, not stale under a large max_age.
        index.build_index().unwrap();
        assert!(!index.is_stale(Duration::from_secs(3600)));

        // Stale under a zero max_age (any elapsed time qualifies).
        assert!(index.is_stale(Duration::from_secs(0)));
    }

    #[test]
    fn test_build_index_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        let count = index.build_index().unwrap();

        assert_eq!(count, 0);
        assert!(index.exists());
    }

    #[test]
    fn test_build_index_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        fs::create_dir_all(file_root.join("shared/docs")).unwrap();
        fs::write(file_root.join("shared/docs/report.pdf"), "test content").unwrap();
        fs::write(file_root.join("shared/readme.txt"), "readme").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        let count = index.build_index().unwrap();

        // Should have: shared, shared/docs, shared/docs/report.pdf, shared/readme.txt
        assert_eq!(count, 4);
        assert!(index.exists());
    }

    #[test]
    fn test_build_index_with_special_chars() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/file,with,commas.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("commas", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "file,with,commas.txt");
    }

    #[test]
    fn test_search_empty_index() {
        let temp_dir = TempDir::new().unwrap();
        let index = FileIndex::new(temp_dir.path(), temp_dir.path());

        let results = index.search("test", None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_finds_matches() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/report.pdf"), "content").unwrap();
        fs::write(file_root.join("shared/notes.txt"), "content").unwrap();
        fs::write(file_root.join("shared/other.doc"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("report", None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.contains("report"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/Report.PDF"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("REPORT", None).unwrap();
        assert_eq!(results.len(), 1);

        let results = index.search("report", None).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_with_area_filter() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::create_dir_all(file_root.join("users/alice")).unwrap();
        fs::write(file_root.join("shared/doc.txt"), "content").unwrap();
        fs::write(file_root.join("users/alice/doc.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("doc", None).unwrap();
        assert_eq!(results.len(), 2);

        let results = index.search("doc", Some("/shared")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.starts_with("/shared"));

        let results = index.search("doc", Some("/users/alice")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.starts_with("/users/alice"));
    }

    #[test]
    fn test_search_respects_max_results() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        // Create more files than MAX_SEARCH_RESULTS
        for i in 0..150 {
            fs::write(file_root.join(format!("shared/file{}.txt", i)), "content").unwrap();
        }

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("file", None).unwrap();
        assert_eq!(results.len(), MAX_SEARCH_RESULTS);
    }

    #[test]
    fn test_search_literal_special_chars() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        // Create file with regex special characters in name
        fs::write(file_root.join("shared/file[1].txt"), "content").unwrap();
        fs::write(file_root.join("shared/file.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Search for literal "[1]" - should not be treated as regex
        let results = index.search("[1]", None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].name.contains("[1]"));
    }

    #[test]
    fn test_roundtrip_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&file_root).unwrap();

        // Create files with various special characters
        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/normal.txt"), "content").unwrap();
        fs::write(file_root.join("shared/has spaces.txt"), "content").unwrap();
        fs::write(file_root.join("shared/has,comma.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let file = File::open(&index.index_path).unwrap();
        let mut reader = ReaderBuilder::new()
            .has_headers(false)
            .from_reader(BufReader::new(file));

        let records: Vec<_> = reader.records().filter_map(|r| r.ok()).collect();

        // Should have shared dir + 3 files = 4 entries
        assert_eq!(records.len(), 4);

        // Verify the file with comma was properly escaped and read back
        let comma_record = records.iter().find(|r| r.get(1) == Some("has,comma.txt"));
        assert!(comma_record.is_some());
    }

    #[test]
    fn test_search_multiple_terms_and_logic() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared/music")).unwrap();

        fs::write(
            file_root.join("shared/music/Derrick Carter - Live Mix.mp3"),
            "content",
        )
        .unwrap();
        fs::write(
            file_root.join("shared/music/Derrick Carter - Studio Session.flac"),
            "content",
        )
        .unwrap();
        fs::write(
            file_root.join("shared/music/Derrick May - Techno Mix.mp3"),
            "content",
        )
        .unwrap();
        fs::write(
            file_root.join("shared/music/Ron Carter - Jazz.mp3"),
            "content",
        )
        .unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Search for "derrick carter mp3" - should match only the first file
        let results = index.search("derrick carter mp3", None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].name.contains("Derrick Carter"));
        assert!(results[0].name.contains(".mp3"));

        // Search for "derrick mp3" - should match Derrick Carter and Derrick May mp3s
        let results = index.search("derrick mp3", None).unwrap();
        assert_eq!(results.len(), 2);

        // Search for "carter" - should match both Derrick Carter and Ron Carter
        let results = index.search("carter", None).unwrap();
        assert_eq!(results.len(), 3); // 2 Derrick Carter + 1 Ron Carter
    }

    #[test]
    fn test_search_term_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        fs::write(file_root.join("shared/test_file.txt"), "content").unwrap();
        fs::write(file_root.join("shared/test_ab_cd.txt"), "content").unwrap();
        fs::write(file_root.join("shared/other.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Single-char terms filtered in AND mode
        // "test a b" -> only "test" used (a, b filtered as single char)
        let results = index.search("test a b", None).unwrap();
        assert_eq!(results.len(), 2); // matches both test_file.txt and test_ab_cd.txt

        // 2+ char terms included in AND mode
        // "test ab" -> "test" AND "ab" both used
        let results = index.search("test ab", None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].name.contains("test_ab"));

        // "test" alone matches both test files
        let results = index.search("test", None).unwrap();
        assert_eq!(results.len(), 2);

        // All terms < 3 chars - treated as literal search "a b c"
        // No file contains literal "a b c", so empty results
        let results = index.search("a b c", None).unwrap();
        assert!(results.is_empty());

        // All 2-char terms - literal mode "ab cd"
        let results = index.search("ab cd", None).unwrap();
        assert!(results.is_empty()); // no file contains literal "ab cd"
    }

    #[test]
    fn test_build_index_skips_hidden_entries() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        fs::write(file_root.join("shared/visible.txt"), "content").unwrap();

        // Create hidden entries that should be skipped
        fs::create_dir_all(file_root.join("shared/@eaDir")).unwrap();
        fs::write(file_root.join("shared/@eaDir/metadata.db"), "meta").unwrap();
        fs::create_dir_all(file_root.join("shared/#recycle")).unwrap();
        fs::write(file_root.join("shared/#recycle/deleted.txt"), "trash").unwrap();
        fs::write(file_root.join("shared/.hidden"), "secret").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        let count = index.build_index().unwrap();

        // Should only have: shared, shared/visible.txt
        assert_eq!(count, 2);

        // Search should not find hidden entries
        let results = index.search("metadata", None).unwrap();
        assert_eq!(results.len(), 0);

        let results = index.search("deleted", None).unwrap();
        assert_eq!(results.len(), 0);

        let results = index.search("hidden", None).unwrap();
        assert_eq!(results.len(), 0);

        let results = index.search("visible", None).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_terms_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        fs::write(file_root.join("shared/Derrick_Carter_Mix.MP3"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        let results = index.search("DERRICK carter MP3", None).unwrap();
        assert_eq!(results.len(), 1);

        let results = index.search("derrick CARTER mp3", None).unwrap();
        assert_eq!(results.len(), 1);
    }
}
