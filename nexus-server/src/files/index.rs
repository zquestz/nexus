//! File index module for building and searching the file index
//!
//! This module provides functionality to:
//! - Build a CSV index of all files in the file area using `walkdir`
//! - Search the index using `grep-searcher` for fast streaming search
//! - Handle atomic index updates via temp file + rename
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

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use csv::{ReaderBuilder, WriterBuilder};
use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::UTF8;
use walkdir::WalkDir;

use nexus_common::protocol::FileSearchResult;
use nexus_common::validators::extract_search_terms;

/// Maximum number of search results to return
pub const MAX_SEARCH_RESULTS: usize = 100;

/// Index file name
const INDEX_FILE_NAME: &str = "files.idx";

/// Temporary index file name (for atomic swap)
const INDEX_TEMP_FILE_NAME: &str = "files.idx.tmp";

/// File index state
pub struct FileIndex {
    /// Path to the index file
    index_path: PathBuf,
    /// Path to the temporary index file
    temp_path: PathBuf,
    /// Root path of the file area
    file_root: PathBuf,
    /// Whether the index needs to be rebuilt
    dirty: AtomicBool,
    /// Whether a reindex is currently in progress
    reindexing: AtomicBool,
}

impl FileIndex {
    /// Create a new file index
    ///
    /// The index file will be stored in the same directory as the database,
    /// not in the file area itself.
    pub fn new(data_dir: &Path, file_root: &Path) -> Self {
        Self {
            index_path: data_dir.join(INDEX_FILE_NAME),
            temp_path: data_dir.join(INDEX_TEMP_FILE_NAME),
            file_root: file_root.to_path_buf(),
            dirty: AtomicBool::new(false),
            reindexing: AtomicBool::new(false),
        }
    }

    /// Mark the index as dirty (needs rebuild)
    ///
    /// Called after file uploads, deletes, renames, or moves.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Check if the index is dirty
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    /// Check if a reindex is currently in progress
    pub fn is_reindexing(&self) -> bool {
        self.reindexing.load(Ordering::SeqCst)
    }

    /// Check if the index file exists
    pub fn exists(&self) -> bool {
        self.index_path.exists()
    }

    /// Trigger a reindex if not already running
    ///
    /// Returns `true` if reindex was started, `false` if one is already running.
    /// This method is non-blocking - the actual reindex runs in the background.
    pub fn trigger_reindex(self: &Arc<Self>) -> bool {
        // Try to set reindexing flag - if already true, another reindex is running
        if self.reindexing.swap(true, Ordering::SeqCst) {
            // Already reindexing, keep dirty flag set
            return false;
        }

        // Clear dirty flag now that we're starting
        self.dirty.store(false, Ordering::SeqCst);

        // Clone Arc for the spawned task
        let index = Arc::clone(self);

        // Spawn on blocking thread pool since build_index does synchronous I/O
        tokio::task::spawn_blocking(move || {
            match index.build_index() {
                Ok(count) => {
                    eprintln!("File index rebuilt: {} entries", count);
                }
                Err(e) => {
                    eprintln!("Failed to build file index: {}", e);
                    // Mark dirty again so we retry
                    index.mark_dirty();
                }
            }

            // Clear reindexing flag
            index.reindexing.store(false, Ordering::SeqCst);
        });

        true
    }

    /// Build the index synchronously
    ///
    /// Walks the file area, writes to temp file, then atomically swaps.
    /// Returns the number of entries indexed.
    fn build_index(&self) -> Result<usize, String> {
        // Create temp file with CSV writer
        let file = File::create(&self.temp_path)
            .map_err(|e| format!("Failed to create temp index: {}", e))?;

        // Set restrictive permissions on index file (contains file tree structure)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&self.temp_path, perms)
                .map_err(|e| format!("Failed to set index permissions: {}", e))?;
        }

        let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);

        let mut count = 0;

        // Walk the file area
        for entry in WalkDir::new(&self.file_root)
            .min_depth(1) // Skip the root itself
            .follow_links(true) // Follow symlinks (admin-trusted)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Get metadata (follows symlinks)
            let metadata = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue, // Skip files we can't stat
            };

            // Get size (0 for directories)
            let size = if metadata.is_file() {
                metadata.len()
            } else {
                0
            };

            // Get modified time as Unix timestamp
            let modified = metadata
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            // Get path relative to file_root
            let relative_path = match path.strip_prefix(&self.file_root) {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Convert to forward slashes and add leading /
            let path_str = format!("/{}", relative_path.to_string_lossy().replace('\\', "/"));

            // Get filename
            let name = entry.file_name().to_string_lossy().into_owned();
            let size_str = size.to_string();
            let modified_str = modified.to_string();
            let is_dir_str = if metadata.is_dir() { "1" } else { "0" };

            // Write CSV record: path, name, size, modified, is_directory
            writer
                .write_record([
                    path_str.as_str(),
                    name.as_str(),
                    size_str.as_str(),
                    modified_str.as_str(),
                    is_dir_str,
                ])
                .map_err(|e| format!("Failed to write index entry: {}", e))?;

            count += 1;
        }

        // Flush and close
        writer
            .flush()
            .map_err(|e| format!("Failed to flush index: {}", e))?;
        drop(writer);

        // Atomic rename
        fs::rename(&self.temp_path, &self.index_path)
            .map_err(|e| format!("Failed to swap index file: {}", e))?;

        Ok(count)
    }

    /// Search the index for matching files
    ///
    /// Returns up to `MAX_SEARCH_RESULTS` matching entries.
    /// If `area_prefix` is provided, only returns results within that area.
    ///
    /// If the index is corrupted, it will be deleted and marked dirty for rebuild,
    /// and empty results will be returned.
    pub fn search(
        &self,
        query: &str,
        area_prefix: Option<&str>,
    ) -> Result<Vec<FileSearchResult>, String> {
        // If index doesn't exist, return empty results
        if !self.index_path.exists() {
            return Ok(vec![]);
        }

        // Extract valid search terms (3+ chars, plus 2-char terms if there's a primary term)
        // Single-character terms are ignored
        let terms = extract_search_terms(query);

        // Must have at least one valid term
        if terms.is_empty() {
            return Ok(vec![]);
        }

        // Use first term for initial grep search (fastest filter)
        let first_term = regex::escape(terms[0]);
        let pattern = format!("(?i){}", first_term);
        let matcher =
            RegexMatcher::new(&pattern).map_err(|e| format!("Invalid search pattern: {}", e))?;

        // Prepare remaining terms for secondary filtering (lowercase for case-insensitive)
        let remaining_terms: Vec<String> = terms[1..].iter().map(|t| t.to_lowercase()).collect();

        let mut results = Vec::new();

        // Search the index file
        let search_result = Searcher::new().search_path(
            &matcher,
            &self.index_path,
            UTF8(|_line_num, line| {
                // Stop if we have enough results
                if results.len() >= MAX_SEARCH_RESULTS {
                    return Ok(false);
                }

                // Parse CSV line using csv crate
                if let Some(entry) = parse_csv_line(line.trim()) {
                    // Filter by area if specified
                    if let Some(prefix) = area_prefix
                        && !entry.path.starts_with(prefix)
                    {
                        return Ok(true); // Continue searching
                    }

                    // Check that ALL remaining terms match (AND logic)
                    // We check against the path which includes the filename
                    let path_lower = entry.path.to_lowercase();
                    let all_match = remaining_terms.iter().all(|term| path_lower.contains(term));
                    if !all_match {
                        return Ok(true); // Continue searching
                    }

                    results.push(entry);
                }

                Ok(true)
            }),
        );

        if let Err(e) = search_result {
            // Index may be corrupted - delete it and mark dirty for rebuild
            eprintln!(
                "Search failed (index may be corrupted): {}. Deleting index for rebuild.",
                e
            );
            if let Err(del_err) = fs::remove_file(&self.index_path) {
                eprintln!("Failed to delete corrupted index: {}", del_err);
            }
            self.mark_dirty();
            return Ok(vec![]);
        }

        Ok(results)
    }
}

/// Parse a CSV line into a FileSearchResult using the csv crate
fn parse_csv_line(line: &str) -> Option<FileSearchResult> {
    // Use csv reader to parse single line
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

        // Create some test files
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

        // Create file with comma in name
        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/file,with,commas.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Search for it
        let results = index.search("commas", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "file,with,commas.txt");
    }

    #[test]
    fn test_search_empty_index() {
        let temp_dir = TempDir::new().unwrap();
        let index = FileIndex::new(temp_dir.path(), temp_dir.path());

        // No index file exists
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

        // Create test files
        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::write(file_root.join("shared/report.pdf"), "content").unwrap();
        fs::write(file_root.join("shared/notes.txt"), "content").unwrap();
        fs::write(file_root.join("shared/other.doc"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Search for "report"
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

        // Search with different case
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

        // Create files in different areas
        fs::create_dir_all(file_root.join("shared")).unwrap();
        fs::create_dir_all(file_root.join("users/alice")).unwrap();
        fs::write(file_root.join("shared/doc.txt"), "content").unwrap();
        fs::write(file_root.join("users/alice/doc.txt"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Search all areas
        let results = index.search("doc", None).unwrap();
        assert_eq!(results.len(), 2);

        // Search only shared
        let results = index.search("doc", Some("/shared")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.starts_with("/shared"));

        // Search only alice's area
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

        // Read back and verify
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

        // Create test files
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

        // Create files with various name patterns
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
    fn test_search_terms_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("data");
        let file_root = temp_dir.path().join("files");

        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(file_root.join("shared")).unwrap();

        fs::write(file_root.join("shared/Derrick_Carter_Mix.MP3"), "content").unwrap();

        let index = FileIndex::new(&data_dir, &file_root);
        index.build_index().unwrap();

        // Mixed case search should work
        let results = index.search("DERRICK carter MP3", None).unwrap();
        assert_eq!(results.len(), 1);

        let results = index.search("derrick CARTER mp3", None).unwrap();
        assert_eq!(results.len(), 1);
    }
}
