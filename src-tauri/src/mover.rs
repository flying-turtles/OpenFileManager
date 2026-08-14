use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Destination sub-directory for a file, from its modified timestamp
/// ("2026-03-14 09:12:33" -> "2026-03-14").
pub fn date_dir(modified_at: Option<&str>) -> String {
    modified_at
        .and_then(|s| s.get(..10))
        .filter(|s| s.len() == 10)
        .unwrap_or("unknown")
        .to_string()
}

/// Pick a free destination path, suffixing `_1`, `_2`, ... on collision.
/// Collisions are checked against both the filesystem (`exists`) and the
/// paths already handed out for this plan (`taken`), so two sources with the
/// same name cannot be assigned the same destination.
pub fn assign_dest(
    dest_dir: &Path,
    file_name: &str,
    taken: &mut HashSet<PathBuf>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let stem = Path::new(file_name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = Path::new(file_name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut candidate = dest_dir.join(file_name);
    let mut counter = 1u32;
    while taken.contains(&candidate) || exists(&candidate) {
        candidate = dest_dir.join(format!("{}_{}{}", stem, counter, ext));
        counter += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

/// Reject move roots that would eat their own source.
pub fn validate_roots(sources: &[PathBuf], dest: &Path) -> Result<(), AppError> {
    if sources.is_empty() {
        return Err(AppError::General("No source selected".into()));
    }
    for src in sources {
        if dest == src || dest.starts_with(src) {
            return Err(AppError::General(format!(
                "Destination {} is inside the source {}",
                dest.display(),
                src.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_dir_takes_the_date_part() {
        assert_eq!(date_dir(Some("2026-03-14 09:12:33")), "2026-03-14");
    }

    #[test]
    fn date_dir_falls_back_to_unknown() {
        assert_eq!(date_dir(None), "unknown");
        assert_eq!(date_dir(Some("short")), "unknown");
    }

    #[test]
    fn assign_dest_uses_the_plain_name_when_free() {
        let mut taken = HashSet::new();
        let got = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        assert_eq!(got, PathBuf::from("/d/2026-03-14/a.cr3"));
    }

    #[test]
    fn assign_dest_suffixes_around_an_existing_file() {
        let mut taken = HashSet::new();
        let exists = |p: &Path| p == Path::new("/d/2026-03-14/a.cr3");
        let got = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &exists);
        assert_eq!(got, PathBuf::from("/d/2026-03-14/a_1.cr3"));
    }

    #[test]
    fn assign_dest_suffixes_around_names_taken_earlier_in_the_plan() {
        let mut taken = HashSet::new();
        let first = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        let second = assign_dest(Path::new("/d/2026-03-14"), "a.cr3", &mut taken, &|_| false);
        assert_eq!(first, PathBuf::from("/d/2026-03-14/a.cr3"));
        assert_eq!(second, PathBuf::from("/d/2026-03-14/a_1.cr3"));
    }

    #[test]
    fn assign_dest_keeps_extensionless_names_intact() {
        let mut taken = HashSet::new();
        let got = assign_dest(Path::new("/d/unknown"), "README", &mut taken, &|_| false);
        assert_eq!(got, PathBuf::from("/d/unknown/README"));
    }

    #[test]
    fn validate_roots_rejects_dest_inside_a_source() {
        let err = validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Shoot/Sub"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("inside"));
    }

    #[test]
    fn validate_roots_rejects_dest_equal_to_a_source() {
        let err = validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Shoot"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("inside"));
    }

    #[test]
    fn validate_roots_rejects_an_empty_source_list() {
        let err = validate_roots(&[], Path::new("/Volumes/Target")).unwrap_err();
        assert!(err.to_string().contains("No source"));
    }

    #[test]
    fn validate_roots_accepts_a_sibling_destination() {
        assert!(validate_roots(
            &[PathBuf::from("/Volumes/SD/Shoot")],
            Path::new("/Volumes/SD/Archive"),
        )
        .is_ok());
    }
}
