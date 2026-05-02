use darwin_code_utils_absolute_path::AbsolutePathBuf;
use dirs::home_dir;
use std::path::PathBuf;

/// Returns the path to the DarwinCode configuration directory.
///
/// Resolution order:
///
/// 1. `DARWIN_CODE_HOME`, when set to a non-empty value.
/// 2. The DarwinCode source checkout root, when running from inside it.
/// 3. `~/.darwin`.
///
/// Explicit `DARWIN_CODE_HOME` values must exist and be directories. Source
/// checkout detection is intentionally narrow so ordinary project-local
/// `config.toml` files are not mistaken for DarwinCode homes.
pub fn find_darwin_code_home() -> std::io::Result<AbsolutePathBuf> {
    let darwin_code_home_env = std::env::var("DARWIN_CODE_HOME")
        .ok()
        .filter(|val| !val.is_empty());
    let cwd = std::env::current_dir().ok();
    find_darwin_code_home_from_env_and_cwd(darwin_code_home_env.as_deref(), cwd.as_deref())
}

fn find_darwin_code_home_from_env_and_cwd(
    darwin_code_home_env: Option<&str>,
    cwd: Option<&std::path::Path>,
) -> std::io::Result<AbsolutePathBuf> {
    // Honor the `DARWIN_CODE_HOME` environment variable when it is set to allow users
    // (and tests) to override the default location.
    match darwin_code_home_env {
        Some(val) => resolve_home_env_dir("DARWIN_CODE_HOME", val),
        None => cwd
            .and_then(find_darwin_code_source_home_from_cwd)
            .map(AbsolutePathBuf::from_absolute_path)
            .transpose()?
            .map(Ok)
            .unwrap_or_else(default_darwin_code_home),
    }
}

fn resolve_home_env_dir(env_name: &str, value: &str) -> std::io::Result<AbsolutePathBuf> {
    let path = PathBuf::from(value);
    let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{env_name} points to {value:?}, but that path does not exist"),
        ),
        _ => std::io::Error::new(
            err.kind(),
            format!("failed to read {env_name} {value:?}: {err}"),
        ),
    })?;

    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{env_name} points to {value:?}, but that path is not a directory"),
        ));
    }

    let canonical = path.canonicalize().map_err(|err| {
        std::io::Error::new(
            err.kind(),
            format!("failed to canonicalize {env_name} {value:?}: {err}"),
        )
    })?;
    AbsolutePathBuf::from_absolute_path(canonical)
}

fn find_darwin_code_source_home_from_cwd(cwd: &std::path::Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|ancestor| is_darwin_code_source_home(ancestor))
        .and_then(|ancestor| ancestor.canonicalize().ok())
}

fn is_darwin_code_source_home(path: &std::path::Path) -> bool {
    let cargo_toml = path.join("darwin-rs").join("Cargo.toml");
    if !path.join("config.toml").is_file() || !cargo_toml.is_file() {
        return false;
    }

    let cargo_marker = std::fs::read_to_string(cargo_toml)
        .is_ok_and(|cargo| cargo.contains("darwin-code-cli") || cargo.contains("darwin-code-core"));
    let readme_marker = std::fs::read_to_string(path.join("README.md")).is_ok_and(|readme| {
        readme.contains("Darwin Code Engine") || readme.contains("DarwinCode Engine")
    });
    cargo_marker || readme_marker
}

fn default_darwin_code_home() -> std::io::Result<AbsolutePathBuf> {
    let mut path = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;
    path.push(".darwin");
    AbsolutePathBuf::from_absolute_path(path)
}

#[cfg(test)]
mod tests {
    use super::find_darwin_code_home_from_env_and_cwd;
    use darwin_code_utils_absolute_path::AbsolutePathBuf;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;

    #[test]
    fn find_darwin_code_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-darwin-code-home");
        let missing_str = missing
            .to_str()
            .expect("missing darwin home path should be valid utf-8");

        let err = find_darwin_code_home_from_env_and_cwd(Some(missing_str), None)
            .expect_err("missing DARWIN_CODE_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("DARWIN_CODE_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_darwin_code_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("darwin-code-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file darwin home path should be valid utf-8");

        let err = find_darwin_code_home_from_env_and_cwd(Some(file_str), None)
            .expect_err("file DARWIN_CODE_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_darwin_code_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp darwin home path should be valid utf-8");

        let resolved = find_darwin_code_home_from_env_and_cwd(Some(temp_str), None)
            .expect("valid DARWIN_CODE_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_darwin_code_home_without_env_uses_default_home_dir() {
        let resolved =
            find_darwin_code_home_from_env_and_cwd(None, None).expect("default DARWIN_CODE_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(".darwin");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_darwin_code_home_detects_source_checkout_from_root() -> std::io::Result<()> {
        let temp_home = TempDir::new().expect("temp home");
        let root = temp_home.path();
        fs::write(root.join("config.toml"), "")?;
        fs::write(root.join("README.md"), "# Darwin Code Engine\n")?;
        fs::create_dir(root.join("darwin-rs"))?;
        fs::write(root.join("darwin-rs").join("Cargo.toml"), "")?;

        let resolved = find_darwin_code_home_from_env_and_cwd(None, Some(root))?;
        let expected = AbsolutePathBuf::from_absolute_path(root.canonicalize()?)?;
        assert_eq!(resolved, expected);
        Ok(())
    }

    #[test]
    fn find_darwin_code_home_detects_source_checkout_from_child() -> std::io::Result<()> {
        let temp_home = TempDir::new().expect("temp home");
        let root = temp_home.path();
        let child = root.join("darwin-rs").join("cli");
        fs::write(root.join("config.toml"), "")?;
        fs::create_dir_all(&child)?;
        fs::write(
            root.join("darwin-rs").join("Cargo.toml"),
            "darwin-code-cli\n",
        )?;

        let resolved = find_darwin_code_home_from_env_and_cwd(None, Some(&child))?;
        let expected = AbsolutePathBuf::from_absolute_path(root.canonicalize()?)?;
        assert_eq!(resolved, expected);
        Ok(())
    }

    #[test]
    fn source_checkout_detection_does_not_match_plain_project_config() -> std::io::Result<()> {
        let temp_home = TempDir::new().expect("temp home");
        let root = temp_home.path();
        fs::write(root.join("config.toml"), "")?;

        let resolved = find_darwin_code_home_from_env_and_cwd(None, Some(root))?;
        let mut expected = home_dir().expect("home dir");
        expected.push(".darwin");
        let expected = AbsolutePathBuf::from_absolute_path(expected)?;
        assert_eq!(resolved, expected);
        Ok(())
    }
}
