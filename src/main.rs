#[cfg(feature = "time")]
use std::time::Instant;
use std::{
    env,
    fs::{self},
    path::{Path, PathBuf},
    sync::Mutex,
};

use uv_prune::is_hardlink::IsHardLink;

use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use walkdir::WalkDir;

fn main() {
    let uv_cache_dir = get_uv_cache_dir();

    #[cfg(feature = "time")]
    let start_time = Instant::now();

    prune_archive_dir(&uv_cache_dir);
    prune_wheels_dir(&uv_cache_dir);

    #[cfg(feature = "time")]
    {
        let end_time = Instant::now();
        println!("用时：{:?}", end_time - start_time);
    }
}

fn prune_wheels_dir(uv_cache_dir: &PathBuf) {
    const DIR: &str = "wheels-v5";
    print_msg(DIR);
    // TOOD: 非Windows，重构逻辑
    WalkDir::new(uv_cache_dir.join(DIR))
        .into_iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().is_dir())
        .for_each(|entry| {
            for path in entry
                .path()
                .read_dir()
                .unwrap()
                .map(Result::unwrap)
                .filter_map(|entry| {
                    let path = entry.path();
                    path.is_file().then_some(path)
                })
            {
                let ext = path.extension().unwrap_or_default();
                if ext != "lock"
                    && ext != "msgpack"
                    && ext != "http"
                    && !uv_cache_dir
                        .join(fs::read_to_string(&path).unwrap())
                        .exists()
                {
                    let path = path.parent().unwrap();
                    println!("{}", path.to_str().unwrap());
                    fs::remove_dir_all(path).unwrap();
                }
            }
        });
}

fn print_msg(dir: &str) {
    println!("清理 {} 目录...", dir);
}

fn prune_archive_dir(uv_cache_dir: &PathBuf) {
    const DIR: &str = "archive-v0";
    print_msg(DIR);
    let paths = Mutex::new(Vec::<Box<Path>>::new());

    WalkDir::new(uv_cache_dir.join(DIR))
        .into_iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(Result::unwrap)
        .for_each(|entry| {
            let path = entry.path();
            if path.is_file()
                && entry.file_name() != "RECORD"
                && let Ok(false) = path.is_hardlink()
            {
                println!("{}", path.to_str().unwrap());
                fs::remove_file(path).unwrap();
                paths.lock().unwrap().push(path.parent().unwrap().into());
            }
        });

    while !paths.lock().unwrap().is_empty() {
        let paths_clone = {
            let mut old_paths = paths.lock().unwrap();
            let paths_clone = old_paths.clone();
            old_paths.clear();
            paths_clone
        };
        paths_clone.into_par_iter().for_each(|path| {
            if fs::remove_dir(&path).is_ok() {
                println!("{}", path.to_str().unwrap());
                paths.lock().unwrap().push(path.parent().unwrap().into());
            }
        });
    }
}

fn get_uv_cache_dir() -> PathBuf {
    let uv_cache_dir = env::var("UV_CACHE_DIR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        (env::var("LOCALAPPDATA").unwrap() + r"\uv\cache")
    });
    PathBuf::from(uv_cache_dir)
}
