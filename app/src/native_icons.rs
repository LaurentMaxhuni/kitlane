use kitlane_core::{inventory::LocalIconSource, model::InventorySnapshot};
use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct LocalIcon {
    pub uri: String,
    pub bytes: Arc<[u8]>,
}
pub type Icons = BTreeMap<String, LocalIcon>;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

fn hash(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn icon_reference(value: &str) -> Option<(PathBuf, i32)> {
    let value = value.strip_prefix('@').unwrap_or(value);
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return None;
    }
    let (path, index) = match value.rsplit_once(',') {
        Some((path, index)) => (path, index.trim().parse::<i32>().ok()?),
        None => (value, 0),
    };
    if index.unsigned_abs() > 65535 {
        return None;
    }
    let path = path.trim().trim_matches('"');
    let bytes = path.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || path[2..].contains([':', '"'])
        || path.split(['\\', '/']).any(|part| part == "..")
    {
        return None;
    }
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    if !matches!(extension.as_str(), "exe" | "dll" | "ico" | "png") {
        return None;
    }
    Some((PathBuf::from(path), index))
}

fn png(bytes: &[u8], format: image::ImageFormat) -> Option<Vec<u8>> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?.thumbnail(96, 96);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).ok()?;
    let bytes = output.into_inner();
    (bytes.len() <= 65536).then_some(bytes)
}

fn read_bounded(path: &Path, limit: usize) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > limit as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

#[cfg(windows)]
fn extract(path: &Path, index: i32) -> Option<Vec<u8>> {
    use std::os::windows::fs::MetadataExt;
    for part in path.ancestors() {
        if fs::symlink_metadata(part).ok()?.file_attributes() & 0x400 != 0 {
            return None;
        }
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "ico") {
        let bytes = read_bounded(path, MAX_IMAGE_BYTES)?;
        return png(
            &bytes,
            if extension == "ico" {
                image::ImageFormat::Ico
            } else {
                image::ImageFormat::Png
            },
        );
    }
    if fs::metadata(path).ok()?.len() > 512 * 1024 * 1024 {
        return None;
    }
    let map = pelite::FileMap::open(path).ok()?;
    let pe = pelite::PeFile::from_bytes(map.as_ref()).ok()?;
    let resources = pe.resources().ok()?;
    for (position, (name, group)) in resources
        .icons()
        .filter_map(Result::ok)
        .enumerate()
        .take(128)
    {
        let selected = if index < 0 {
            name == pelite::resources::Name::Id(index.unsigned_abs())
        } else {
            position == index as usize
        };
        if !selected {
            continue;
        }
        if group.entries().len() > 64 {
            return None;
        }
        let mut size = 0usize;
        for entry in group.entries() {
            size = size.checked_add(group.image(entry.nId).ok()?.len())?;
        }
        if size > MAX_IMAGE_BYTES - 4096 {
            return None;
        }
        let mut ico = Vec::new();
        group.write(&mut ico).ok()?;
        return png(&ico, image::ImageFormat::Ico);
    }
    None
}

#[cfg(not(windows))]
fn extract(_path: &Path, _index: i32) -> Option<Vec<u8>> {
    None
}

fn icon(id: &str, bytes: Vec<u8>) -> LocalIcon {
    LocalIcon {
        uri: format!(
            "bytes://installed/{:016x}-{:016x}.png",
            hash(id),
            hash(&bytes)
        ),
        bytes: bytes.into(),
    }
}

pub fn from_cache(snapshot: &InventorySnapshot, directory: Option<&Path>) -> Icons {
    let mut icons = Icons::new();
    let mut total = 0;
    if let Some(directory) = directory {
        for app in &snapshot.apps {
            if let Some(bytes) = read_bounded(
                &directory.join(format!("{:016x}.png", hash(&app.id))),
                65536,
            )
            .and_then(|bytes| png(&bytes, image::ImageFormat::Png))
            {
                total += bytes.len();
                if total > MAX_TOTAL_BYTES {
                    break;
                }
                icons.insert(app.id.clone(), icon(&app.id, bytes));
            }
        }
    }
    icons
}

pub fn load(sources: &[LocalIconSource], directory: Option<&Path>) -> Icons {
    let mut icons = Icons::new();
    let mut total = 0;
    if let Some(directory) = directory {
        let _ = fs::create_dir_all(directory);
    }
    for source in sources {
        let mut candidates = Vec::new();
        if let Some(reference) = &source.display_icon {
            let mut reference = reference.clone();
            for name in [
                "SystemRoot",
                "ProgramFiles",
                "ProgramFiles(x86)",
                "LOCALAPPDATA",
            ] {
                if let Ok(value) = std::env::var(name) {
                    reference = reference.replace(&format!("%{name}%"), &value);
                }
            }
            if let Some(candidate) = icon_reference(&reference) {
                candidates.push(candidate);
            }
        }
        if let Some(directory) = &source.install_location
            && let Some((directory, _)) = icon_reference(&format!(
                "{}\\placeholder.ico",
                directory.trim_end_matches(['\\', '/'])
            ))
            && let Some(parent) = directory.parent()
            && let Ok(entries) = fs::read_dir(parent)
        {
            for entry in entries.flatten().take(32) {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ico"))
                {
                    candidates.push((path, 0));
                } else if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                {
                    let stem = path
                        .file_stem()
                        .and_then(|v| v.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    let normalized: String = source
                        .app_name
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect();
                    let candidate: String =
                        stem.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                    if candidate.len() >= 3
                        && normalized.contains(&candidate)
                        && !["unins", "uninstall", "update", "setup", "helper", "crash"]
                            .iter()
                            .any(|word| stem.contains(word))
                    {
                        candidates.push((path, 0));
                    }
                }
            }
        }
        if let Some(uninstall) = &source.uninstall {
            let value = uninstall.trim();
            let executable = if let Some(quoted) = value.strip_prefix('"') {
                quoted.split('"').next()
            } else {
                value
                    .to_ascii_lowercase()
                    .find(".exe")
                    .map(|end| &value[..end + 4])
            };
            if let Some(executable) = executable
                && let Some(candidate) = icon_reference(executable)
            {
                candidates.push(candidate);
            }
        }
        if let Some(bytes) = candidates
            .iter()
            .find_map(|(path, index)| extract(path, *index))
        {
            total += bytes.len();
            if total > MAX_TOTAL_BYTES {
                break;
            }
            if let Some(directory) = directory {
                let _ = fs::write(
                    directory.join(format!("{:016x}.png", hash(&source.app_id))),
                    &bytes,
                );
            }
            icons.insert(source.app_id.clone(), icon(&source.app_id, bytes));
        }
    }
    icons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ico_files_decode_and_oversized_or_invalid_images_are_rejected() {
        let bytes = include_bytes!("../../design-system/assets/icons/app-icon.ico");
        let converted = png(bytes, image::ImageFormat::Ico).unwrap();
        assert!(converted.starts_with(b"\x89PNG"));
        assert!(png(b"not an icon", image::ImageFormat::Ico).is_none());
        assert!(png(&vec![0; MAX_IMAGE_BYTES + 1], image::ImageFormat::Png).is_none());
    }

    #[test]
    fn rejects_remote_paths_and_command_strings() {
        for value in [
            r"\\server\share\icon.exe",
            "https://example.com/icon.png",
            r"C:\tool.exe --run",
            r"C:\folder\..\tool.exe",
        ] {
            assert!(icon_reference(value).is_none());
        }
        assert_eq!(
            icon_reference(r#""C:\Program Files\App\app.exe",-4"#)
                .unwrap()
                .1,
            -4
        );
    }
}
