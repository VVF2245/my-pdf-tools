//! Input checks and output naming shared by the tools.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use mpt_core::ConversionError;

/// Fails with `UnsupportedInput` unless the file has one of the `allowed` extensions
/// (lowercase, without the dot).
pub fn check_extension(path: &Path, allowed: &[&str]) -> Result<(), ConversionError> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if allowed.contains(&extension.as_str()) {
        Ok(())
    } else {
        Err(ConversionError::UnsupportedInput { extension })
    }
}

/// `<output_dir or input dir>/<stem>.<extension>`.
///
/// The extension is appended to the whole stem: `notes.v2.md` becomes `notes.v2.pdf`
/// (`Path::with_extension` would turn it into `notes.pdf`).
pub fn output_path(input: &Path, output_dir: Option<&str>, extension: &str) -> PathBuf {
    let dir = match output_dir {
        Some(d) => PathBuf::from(d),
        None => input.parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    dir.join(format!("{stem}.{extension}"))
}

/// `name (n).ext` for `name.ext`.
fn numbered(path: &Path, n: u32) -> PathBuf {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    dir.join(format!("{stem} ({n}).{extension}"))
}

/// Reserves a free output name without ever overwriting: `name.pdf`, then `name (1).pdf`,
/// `name (2).pdf`... The file is *created* (empty) in the same step that finds the name,
/// so two jobs writing into one folder at the same time cannot choose the same one.
///
/// `also_free` lets the caller reject a name for other reasons (e.g. a folder that goes
/// with the file already exists). Returns the name and the open file to write into.
pub fn claim_path(wanted: &Path, also_free: &dyn Fn(&Path) -> bool) -> io::Result<(PathBuf, File)> {
    let mut n = 0;
    loop {
        let candidate = if n == 0 { wanted.to_path_buf() } else { numbered(wanted, n) };
        n += 1;
        if !also_free(&candidate) {
            continue;
        }
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(check_extension(Path::new("a.PDF"), &["pdf"]).is_ok());
        assert_eq!(
            check_extension(Path::new("a.docx"), &["pdf"]),
            Err(ConversionError::UnsupportedInput { extension: "docx".into() })
        );
        assert_eq!(
            check_extension(Path::new("noextension"), &["pdf"]),
            Err(ConversionError::UnsupportedInput { extension: String::new() })
        );
    }

    #[test]
    fn output_goes_next_to_the_input_or_into_the_given_folder() {
        assert_eq!(output_path(Path::new("dir/a.md"), None, "pdf"), Path::new("dir").join("a.pdf"));
        assert_eq!(output_path(Path::new("dir/a.md"), Some("out"), "pdf"), Path::new("out").join("a.pdf"));
    }

    #[test]
    fn a_dotted_stem_keeps_its_dots() {
        assert_eq!(output_path(Path::new("notes.v2.md"), None, "pdf"), Path::new("notes.v2.pdf"));
    }

    #[test]
    fn claiming_counts_up_and_keeps_dotted_stems() {
        let dir = TempDir::new("claim");
        let wanted = dir.path().join("notes.v2.pdf");
        let (first, _) = claim_path(&wanted, &|_| true).unwrap();
        assert_eq!(first, wanted);
        assert!(wanted.exists(), "the name is reserved by creating the file");
        let (second, _) = claim_path(&wanted, &|_| true).unwrap();
        assert_eq!(second, dir.path().join("notes.v2 (1).pdf"));
        let (third, _) = claim_path(&wanted, &|_| true).unwrap();
        assert_eq!(third, dir.path().join("notes.v2 (2).pdf"));
    }

    #[test]
    fn a_name_can_be_rejected_by_the_caller() {
        let dir = TempDir::new("claim-reject");
        let wanted = dir.path().join("a.md");
        let (claimed, _) = claim_path(&wanted, &|candidate| candidate.file_name().unwrap() != "a.md").unwrap();
        assert_eq!(claimed, dir.path().join("a (1).md"));
        assert!(!wanted.exists(), "a rejected name is not created");
    }

    #[test]
    fn simultaneous_claims_never_pick_the_same_name() {
        let dir = TempDir::new("claim-race");
        let wanted = dir.path().join("same.pdf");
        let names: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| scope.spawn(|| claim_path(&wanted, &|_| true).unwrap().0))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 16, "{names:?}");
    }
}
