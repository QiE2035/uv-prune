#[cfg(feature = "time")]
use std::time::Instant;
use std::{env, fs, io, path::PathBuf};

use uv_prune::is_hardlink::IsHardLink;

use rayon::{iter::ParallelBridge, prelude::ParallelIterator};

fn main() {
    let uv_cache_dir = get_uv_cache_dir();

    #[cfg(feature = "time")]
    let start_time = Instant::now();

    prune_archive_dir(&uv_cache_dir);

    #[cfg(feature = "time")]
    {
        let end_time = Instant::now();
        println!("用时：{:?}", end_time - start_time);
    }
}

fn prune_archive_dir(uv_cache_dir: &PathBuf) {
    const DIR: &str = "archive-v0";

    let archive_dir = uv_cache_dir.join(DIR);

    if let Ok(entries) = fs::read_dir(&archive_dir) {
        entries.par_bridge().for_each(|archive_entry| {
            if let Ok(archive) = archive_entry {
                let archive_path = archive.path();
                match should_remove_archive(&archive_path) {
                    Ok(true) => {
                        println!("{}", archive_path.display());
                        if let Err(e) = fs::remove_dir_all(&archive_path) {
                            eprintln!("删除文件夹 {} 失败: {}", archive_path.display(), e);
                        }
                    }
                    Ok(false) => {
                        // 不需要删除该归档
                    }
                    Err(e) => {
                        eprintln!("检查 {} 失败: {}", archive_path.display(), e);
                    }
                }
            }
        });
    } else {
        eprintln!("Could not read directory: {}", archive_dir.display());
    }
}

fn should_remove_archive(archive_path: &PathBuf) -> io::Result<bool> {
    const HARD_LINK_CHECK_FILE: &str = "METADATA";

    let dist_info_path = fs::read_dir(archive_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with(".dist-info"));

    let dist_info_path = match dist_info_path {
        Some(path) => path,
        None => return Ok(true), // 如果没有找到.dist-info目录，则删除
    };

    let metadata_path = dist_info_path.join(HARD_LINK_CHECK_FILE);
    match metadata_path.is_hardlink() {
        Ok(is_hardlink) => Ok(!is_hardlink),
        Err(e) => {
            eprintln!(
                "警告: 无法检查 {} 的硬链接状态: {}",
                metadata_path.display(),
                e
            );
            Ok(true) // 如果无法检查硬链接状态，默认删除
        }
    }
}
fn get_uv_cache_dir() -> PathBuf {
    let uv_cache_dir = env::var("UV_CACHE_DIR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        (env::var("LOCALAPPDATA").unwrap() + r"\uv\cache")
    });
    PathBuf::from(uv_cache_dir)
}
