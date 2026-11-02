//! Port of afe.jar `com.adobe.fontengine.fontmanagement.DirectoryWalker`.

use std::path::{Path, PathBuf};

/// `File.listFiles()`: the directory's entries in the file system's order, as Java gets them.
fn list_files(p: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(p) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => Vec::new(),
    }
}

pub struct DirectoryWalker {
    file_list: Vec<PathBuf>,
    recurse: bool,
}

impl DirectoryWalker {
    pub fn new(file: &Path, recurse: bool) -> DirectoryWalker {
        let file_list = if file.is_dir() { list_files(file).into_iter().filter(|f| f.is_file() || f.is_dir()).collect() } else { vec![file.to_path_buf()] };
        DirectoryWalker { file_list, recurse }
    }

    /// Takes the last entry first; a directory is expanded when recursing, else skipped.
    pub fn get_next_file(&mut self) -> Option<PathBuf> {
        loop {
            let file = self.file_list.pop()?;
            if self.recurse && file.is_dir() {
                self.file_list.extend(list_files(&file));
                continue;
            }
            if file.is_dir() {
                continue;
            }
            return Some(file);
        }
    }
}
