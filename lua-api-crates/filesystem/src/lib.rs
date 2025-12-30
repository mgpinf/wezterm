use anyhow::anyhow;
use config::lua::get_or_create_module;
use config::lua::mlua::{self, Lua, Value};
use globset::{Glob, GlobSet, GlobSetBuilder};
use smol::prelude::*;
use std::collections::HashSet;
use std::path::Path;
use wezterm_dynamic::FromDynamic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FileType {
    File,
    Directory,
    Symlink,
    #[cfg(unix)]
    Executable,
    Empty,
}

impl FileType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "file" | "f" => Some(FileType::File),
            "directory" | "dir" | "d" => Some(FileType::Directory),
            "symlink" | "l" => Some(FileType::Symlink),
            #[cfg(unix)]
            "executable" | "x" => Some(FileType::Executable),
            "empty" | "e" => Some(FileType::Empty),
            _ => None,
        }
    }

    fn matches(&self, path: &Path) -> bool {
        match self {
            FileType::File => path.is_file(),
            FileType::Directory => path.is_dir(),
            FileType::Symlink => path.is_symlink(),
            #[cfg(unix)]
            FileType::Executable => {
                use std::os::unix::fs::PermissionsExt;
                path.is_file()
                    && path
                        .metadata()
                        .map(|m| m.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
            }
            FileType::Empty => {
                if path.is_file() {
                    path.metadata().map(|m| m.len() == 0).unwrap_or(false)
                } else if path.is_dir() {
                    path.read_dir()
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
        }
    }
}

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let wezterm_mod = get_or_create_module(lua, "wezterm")?;
    wezterm_mod.set("read_dir", lua.create_async_function(read_dir)?)?;
    wezterm_mod.set("glob", lua.create_async_function(glob)?)?;
    wezterm_mod.set("find_files", lua.create_async_function(find_files)?)?;
    Ok(())
}

async fn read_dir<'lua>(_: &'lua Lua, path: String) -> mlua::Result<Vec<String>> {
    let mut dir = smol::fs::read_dir(path)
        .await
        .map_err(mlua::Error::external)?;
    let mut entries = vec![];
    while let Some(entry) = dir.next().await {
        let entry = entry.map_err(mlua::Error::external)?;
        if let Some(utf8) = entry.path().to_str() {
            entries.push(utf8.to_string());
        } else {
            return Err(mlua::Error::external(anyhow!(
                "path entry {} is not representable as utf8",
                entry.path().display()
            )));
        }
    }
    Ok(entries)
}

async fn glob<'lua>(
    _: &'lua Lua,
    (pattern, path): (String, Option<String>),
) -> mlua::Result<Vec<String>> {
    let entries = smol::unblock(move || {
        let mut entries = vec![];
        let glob = filenamegen::Glob::new(&pattern)?;
        for path in glob.walk(path.as_deref().unwrap_or(".")) {
            if let Some(utf8) = path.to_str() {
                entries.push(utf8.to_string());
            } else {
                return Err(anyhow!(
                    "path entry {} is not representable as utf8",
                    path.display()
                ));
            }
        }
        Ok(entries)
    })
    .await
    .map_err(mlua::Error::external)?;
    Ok(entries)
}

#[derive(Debug, Default, FromDynamic)]
struct FindFilesOptions {
    #[dynamic(default)]
    extensions: Vec<String>,
    #[dynamic(default)]
    exclude: Vec<String>,
    #[dynamic(default)]
    types: Vec<String>,
    #[dynamic(default)]
    max_depth: Option<usize>,
    #[dynamic(default)]
    hidden: bool,
}

async fn find_files<'lua>(
    _: &'lua Lua,
    (directory, options): (String, Option<Value<'_>>),
) -> mlua::Result<Vec<String>> {
    let opts: FindFilesOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => FindFilesOptions::default(),
    };

    let entries = smol::unblock(move || {
        let mut results = vec![];
        let extensions: HashSet<String> = opts
            .extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .collect();

        let exclude_set = build_glob_set(&opts.exclude)?;

        let file_types: HashSet<FileType> = opts
            .types
            .iter()
            .filter_map(|t| FileType::from_str(t))
            .collect();

        fn walk_dir(
            dir: &Path,
            extensions: &HashSet<String>,
            exclude_set: &GlobSet,
            file_types: &HashSet<FileType>,
            max_depth: Option<usize>,
            current_depth: usize,
            hidden: bool,
            results: &mut Vec<String>,
        ) -> anyhow::Result<()> {
            if let Some(max) = max_depth {
                if current_depth > max {
                    return Ok(());
                }
            }

            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => return Ok(()),
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name,
                    None => continue,
                };

                if !hidden && file_name.starts_with('.') {
                    continue;
                }

                if exclude_set.is_match(&path) {
                    continue;
                }

                let is_dir = path.is_dir();

                // Check if this entry matches the type filter
                let type_matches = if file_types.is_empty() {
                    // Default behavior: only match files
                    path.is_file()
                } else {
                    file_types.iter().any(|ft| ft.matches(&path))
                };

                // Always recurse into directories (unless excluded)
                if is_dir {
                    walk_dir(
                        &path,
                        extensions,
                        exclude_set,
                        file_types,
                        max_depth,
                        current_depth + 1,
                        hidden,
                        results,
                    )?;
                }

                // Add to results if type matches
                if type_matches {
                    // Apply extension filter only to files
                    if path.is_file() && !extensions.is_empty() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if !extensions.contains(&ext.to_lowercase()) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    if let Some(utf8) = path.to_str() {
                        results.push(utf8.to_string());
                    }
                }
            }
            Ok(())
        }

        walk_dir(
            Path::new(&directory),
            &extensions,
            &exclude_set,
            &file_types,
            opts.max_depth,
            1,
            opts.hidden,
            &mut results,
        )?;

        Ok::<_, anyhow::Error>(results)
    })
    .await
    .map_err(mlua::Error::external)?;

    Ok(entries)
}

fn build_glob_set(patterns: &[String]) -> anyhow::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)?;
        builder.add(glob);
    }
    Ok(builder.build()?)
}
