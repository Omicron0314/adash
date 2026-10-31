//! Runtime locations, independent of the machine that compiled the application.
use std::path::{Path, PathBuf};

fn absolute(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .expect("无法确定当前目录")
            .join(path)
    }
}
pub fn data_dir() -> PathBuf {
    absolute(
        std::env::var_os("ADASH_DATA_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::data_local_dir().map(|path| path.join("adash")))
            .expect("无法确定用户数据目录，请设置 ADASH_DATA_DIR"),
    )
}
pub fn executable(name: &str) -> String {
    if !std::env::consts::EXE_SUFFIX.is_empty() && name.ends_with(std::env::consts::EXE_SUFFIX) {
        name.to_owned()
    } else {
        format!("{name}{}", std::env::consts::EXE_SUFFIX)
    }
}
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let name = executable(name);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(&name))
            .find(|path| path.is_file())
            .map(absolute)
    })
}
pub fn tool_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ADASH_TOOL").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if path.components().count() == 1
            && let Some(found) = find_on_path(&path.to_string_lossy())
        {
            return Some(absolute(found));
        }
        return Some(absolute(path));
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|parent| parent.join(executable("adash-tool")))
        })
        .filter(|path| path.is_file())
        .or_else(|| find_on_path("adash-tool"))
}
fn contains_resources(path: &Path) -> bool {
    path.join("agent/runtime/dist/index.js").is_file()
}
/// Supports a self-contained directory, <prefix>/bin + <prefix>/share/adash,
/// and a source checkout discovered from the actual executable/cwd at runtime.
pub fn resources_from(exe: &Path, cwd: &Path) -> Option<PathBuf> {
    let parent = exe.parent()?;
    let candidates = [
        parent.to_path_buf(),
        parent.join("resources/adash"),
        parent.join("../share/adash"),
    ];
    candidates
        .into_iter()
        .chain(parent.ancestors().map(Path::to_path_buf))
        .chain(cwd.ancestors().map(Path::to_path_buf))
        .find(|path| contains_resources(path))
}
pub fn resource_dir() -> Option<PathBuf> {
    if let Some(path) =
        std::env::var_os("ADASH_RESOURCES").or_else(|| std::env::var_os("ADASH_ROOT"))
    {
        return Some(absolute(path.into()));
    }
    resources_from(
        &std::env::current_exe().ok()?,
        &std::env::current_dir().ok()?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resources_follow_a_relocated_install_or_custom_cargo_directory() {
        let directory = tempfile::tempdir().unwrap();
        let install = directory.path().join("relocated/share/adash");
        std::fs::create_dir_all(install.join("agent/runtime/dist")).unwrap();
        std::fs::write(install.join("agent/runtime/dist/index.js"), "").unwrap();
        let exe = directory.path().join("relocated/bin/adash-app");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        assert_eq!(
            resources_from(&exe, Path::new("/unrelated")),
            Some(exe.parent().unwrap().join("../share/adash"))
        );
        let checkout = directory.path().join("checkout");
        std::fs::create_dir_all(checkout.join("agent/runtime/dist")).unwrap();
        std::fs::write(checkout.join("agent/runtime/dist/index.js"), "").unwrap();
        assert_eq!(
            resources_from(
                &directory.path().join("custom-target/debug/adash-app"),
                &checkout
            ),
            Some(checkout)
        );
    }
}
