use config::lua::get_or_create_sub_module;
use config::lua::mlua::{self, Lua, Value};
use git2::{
    build::CheckoutBuilder, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository,
    ResetType, StatusOptions,
};
use std::path::Path;
use wezterm_dynamic::FromDynamic;

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let git_mod = get_or_create_sub_module(lua, "git")?;

    git_mod.set("is_repository", lua.create_async_function(is_repository)?)?;
    git_mod.set(
        "get_repository_root",
        lua.create_async_function(get_repository_root)?,
    )?;
    git_mod.set(
        "get_current_branch",
        lua.create_async_function(get_current_branch)?,
    )?;
    git_mod.set(
        "get_head_commit_hash",
        lua.create_async_function(get_head_commit_hash)?,
    )?;
    git_mod.set("is_dirty", lua.create_async_function(is_dirty)?)?;
    git_mod.set("get_status", lua.create_async_function(get_status)?)?;
    git_mod.set("get_remote_url", lua.create_async_function(get_remote_url)?)?;
    git_mod.set(
        "get_ahead_behind",
        lua.create_async_function(get_ahead_behind)?,
    )?;

    git_mod.set("checkout", lua.create_async_function(checkout)?)?;
    git_mod.set("add", lua.create_async_function(add)?)?;
    git_mod.set("commit", lua.create_async_function(commit)?)?;
    git_mod.set("fetch", lua.create_async_function(fetch)?)?;
    git_mod.set("push", lua.create_async_function(push)?)?;
    git_mod.set("reset", lua.create_async_function(reset)?)?;
    git_mod.set("rebase", lua.create_async_function(rebase)?)?;

    Ok(())
}

async fn is_repository<'lua>(_: &'lua Lua, path: String) -> mlua::Result<bool> {
    Ok(smol::unblock(move || Repository::discover(&path).is_ok()).await)
}

async fn get_repository_root<'lua>(_: &'lua Lua, path: String) -> mlua::Result<Option<String>> {
    Ok(smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;
        Some(repo.workdir()?.to_str()?.to_string())
    })
    .await)
}

async fn get_current_branch<'lua>(_: &'lua Lua, path: String) -> mlua::Result<Option<String>> {
    Ok(smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;
        let head = repo.head().ok()?;

        if head.is_branch() {
            head.shorthand().map(|s| s.to_string())
        } else {
            head.target()
                .map(|oid| format!("HEAD@{}", &oid.to_string()[..7]))
        }
    })
    .await)
}

#[derive(Debug, Default, FromDynamic)]
struct HeadCommitOptions {
    #[dynamic(default)]
    short: bool,
}

async fn get_head_commit_hash<'lua>(
    _: &'lua Lua,
    (path, options): (String, Option<Value<'_>>),
) -> mlua::Result<Option<String>> {
    let opts: HeadCommitOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => HeadCommitOptions::default(),
    };

    Ok(smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;
        let head = repo.head().ok()?;
        let oid = head.target()?;
        let hash = oid.to_string();

        Some(if opts.short {
            hash[..7].to_string()
        } else {
            hash
        })
    })
    .await)
}

async fn is_dirty<'lua>(_: &'lua Lua, path: String) -> mlua::Result<Option<bool>> {
    Ok(smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;

        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(false)
            .exclude_submodules(true);

        let statuses = repo.statuses(Some(&mut opts)).ok()?;
        Some(!statuses.is_empty())
    })
    .await)
}

#[derive(Debug, Default, FromDynamic)]
struct StatusOptionsLua {
    #[dynamic(default = "default_true")]
    include_untracked: bool,
    #[dynamic(default)]
    recurse_untracked_dirs: bool,
}

fn default_true() -> bool {
    true
}

struct StatusResult {
    staged: u32,
    modified: u32,
    deleted: u32,
    untracked: u32,
    conflicted: u32,
    total: u32,
}

async fn get_status<'lua>(
    lua: &'lua Lua,
    (path, options): (String, Option<Value<'_>>),
) -> mlua::Result<Value<'lua>> {
    let opts: StatusOptionsLua = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => StatusOptionsLua {
            include_untracked: true,
            recurse_untracked_dirs: false,
        },
    };

    let result: Option<StatusResult> = smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;

        let mut status_opts = StatusOptions::new();
        status_opts
            .include_untracked(opts.include_untracked)
            .recurse_untracked_dirs(opts.recurse_untracked_dirs)
            .exclude_submodules(true);

        let statuses = repo.statuses(Some(&mut status_opts)).ok()?;

        let mut staged = 0u32;
        let mut modified = 0u32;
        let mut deleted = 0u32;
        let mut untracked = 0u32;
        let mut conflicted = 0u32;

        for entry in statuses.iter() {
            let status = entry.status();

            if status.is_index_new()
                || status.is_index_modified()
                || status.is_index_deleted()
                || status.is_index_renamed()
                || status.is_index_typechange()
            {
                staged += 1;
            }
            if status.is_wt_modified() || status.is_wt_typechange() {
                modified += 1;
            }
            if status.is_wt_deleted() {
                deleted += 1;
            }
            if status.is_wt_new() {
                untracked += 1;
            }
            if status.is_conflicted() {
                conflicted += 1;
            }
        }

        Some(StatusResult {
            staged,
            modified,
            deleted,
            untracked,
            conflicted,
            total: statuses.len() as u32,
        })
    })
    .await;

    match result {
        Some(status) => {
            let table = lua.create_table()?;
            table.set("staged", status.staged)?;
            table.set("modified", status.modified)?;
            table.set("deleted", status.deleted)?;
            table.set("untracked", status.untracked)?;
            table.set("conflicted", status.conflicted)?;
            table.set("total", status.total)?;
            Ok(Value::Table(table))
        }
        None => Ok(Value::Nil),
    }
}

async fn get_remote_url<'lua>(
    _: &'lua Lua,
    (path, remote_name): (String, Option<String>),
) -> mlua::Result<Option<String>> {
    let remote = remote_name.unwrap_or_else(|| "origin".to_string());

    Ok(smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;
        let remote_ref = repo.find_remote(&remote).ok()?;
        remote_ref.url().map(|s| s.to_string())
    })
    .await)
}

async fn get_ahead_behind<'lua>(lua: &'lua Lua, path: String) -> mlua::Result<Value<'lua>> {
    let result: Option<(usize, usize)> = smol::unblock(move || {
        let repo = Repository::discover(&path).ok()?;
        let head = repo.head().ok()?;

        if !head.is_branch() {
            return None;
        }

        let local_oid = head.target()?;
        let branch_name = head.shorthand()?;

        let branch = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .ok()?;
        let upstream = branch.upstream().ok()?;
        let upstream_oid = upstream.get().target()?;

        repo.graph_ahead_behind(local_oid, upstream_oid).ok()
    })
    .await;

    match result {
        Some((ahead, behind)) => {
            let table = lua.create_table()?;
            table.set("ahead", ahead)?;
            table.set("behind", behind)?;
            Ok(Value::Table(table))
        }
        None => Ok(Value::Nil),
    }
}

#[derive(Debug, Default, FromDynamic)]
struct CheckoutOptions {
    #[dynamic(default)]
    create: bool,
    #[dynamic(default)]
    start_point: Option<String>,
}

async fn checkout<'lua>(
    _: &'lua Lua,
    (path, ref_name, options): (String, String, Option<Value<'_>>),
) -> mlua::Result<bool> {
    let opts: CheckoutOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => CheckoutOptions::default(),
    };

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;

        if opts.create {
            let start = opts.start_point.as_deref().unwrap_or("HEAD");
            let commit = repo
                .revparse_single(start)
                .map_err(mlua::Error::external)?
                .peel_to_commit()
                .map_err(mlua::Error::external)?;

            repo.branch(&ref_name, &commit, false)
                .map_err(mlua::Error::external)?;

            repo.set_head(&format!("refs/heads/{}", ref_name))
                .map_err(mlua::Error::external)?;

            repo.checkout_head(Some(CheckoutBuilder::new().safe()))
                .map_err(mlua::Error::external)?;
        } else {
            let (object, reference) = repo
                .revparse_ext(&ref_name)
                .map_err(mlua::Error::external)?;

            repo.checkout_tree(&object, Some(CheckoutBuilder::new().safe()))
                .map_err(mlua::Error::external)?;

            match reference {
                Some(r) => repo.set_head(r.name().unwrap_or(&format!("refs/heads/{}", ref_name))),
                None => repo.set_head_detached(object.id()),
            }
            .map_err(mlua::Error::external)?;
        }

        Ok(true)
    })
    .await
}

async fn add<'lua>(_: &'lua Lua, (path, files): (String, Value<'_>)) -> mlua::Result<bool> {
    let file_list: Vec<String> = match files {
        Value::String(s) => vec![s.to_str()?.to_string()],
        Value::Table(t) => {
            let mut list = Vec::new();
            for pair in t.pairs::<i64, String>() {
                let (_, v) = pair?;
                list.push(v);
            }
            list
        }
        _ => {
            return Err(mlua::Error::external(
                "files must be a string or array of strings",
            ))
        }
    };

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;
        let mut index = repo.index().map_err(mlua::Error::external)?;

        let workdir = repo
            .workdir()
            .ok_or_else(|| mlua::Error::external("repository has no working directory"))?;

        for file in &file_list {
            if file == "." || file == "*" {
                index
                    .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                    .map_err(mlua::Error::external)?;
            } else {
                let file_path = Path::new(file);
                let relative_path = if file_path.is_absolute() {
                    file_path.strip_prefix(workdir).unwrap_or(file_path)
                } else {
                    file_path
                };
                index
                    .add_path(relative_path)
                    .map_err(mlua::Error::external)?;
            }
        }

        index.write().map_err(mlua::Error::external)?;
        Ok(true)
    })
    .await
}

#[derive(Debug, FromDynamic)]
struct CommitOptions {
    message: String,
    #[dynamic(default)]
    author_name: Option<String>,
    #[dynamic(default)]
    author_email: Option<String>,
}

async fn commit<'lua>(_: &'lua Lua, (path, options): (String, Value<'_>)) -> mlua::Result<String> {
    let opts: CommitOptions = luahelper::from_lua(options)?;

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;
        let mut index = repo.index().map_err(mlua::Error::external)?;
        let tree_id = index.write_tree().map_err(mlua::Error::external)?;
        let tree = repo.find_tree(tree_id).map_err(mlua::Error::external)?;

        let signature = match (&opts.author_name, &opts.author_email) {
            (Some(name), Some(email)) => {
                git2::Signature::now(name, email).map_err(mlua::Error::external)?
            }
            _ => repo.signature().map_err(mlua::Error::external)?,
        };

        let parent_commit = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        let commit_id = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                &opts.message,
                &tree,
                &parents,
            )
            .map_err(mlua::Error::external)?;

        Ok(commit_id.to_string())
    })
    .await
}

#[derive(Debug, Default, FromDynamic)]
struct FetchOptions_ {
    #[dynamic(default = "default_origin")]
    remote: String,
}

fn default_origin() -> String {
    "origin".to_string()
}

async fn fetch<'lua>(
    _: &'lua Lua,
    (path, options): (String, Option<Value<'_>>),
) -> mlua::Result<bool> {
    let opts: FetchOptions_ = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => FetchOptions_::default(),
    };

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;
        let mut remote = repo
            .find_remote(&opts.remote)
            .map_err(mlua::Error::external)?;

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                Cred::ssh_key_from_agent(username)
            } else if allowed_types.contains(git2::CredentialType::DEFAULT) {
                Cred::default()
            } else {
                Err(git2::Error::from_str("no valid credential type"))
            }
        });

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote
            .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
            .map_err(mlua::Error::external)?;

        Ok(true)
    })
    .await
}

#[derive(Debug, Default, FromDynamic)]
struct PushOptions_ {
    #[dynamic(default = "default_origin")]
    remote: String,
    #[dynamic(default)]
    branch: Option<String>,
}

async fn push<'lua>(
    _: &'lua Lua,
    (path, options): (String, Option<Value<'_>>),
) -> mlua::Result<bool> {
    let opts: PushOptions_ = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => PushOptions_::default(),
    };

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;
        let mut remote = repo
            .find_remote(&opts.remote)
            .map_err(mlua::Error::external)?;

        let branch = match opts.branch {
            Some(b) => b,
            None => {
                let head = repo.head().map_err(mlua::Error::external)?;
                head.shorthand()
                    .ok_or_else(|| mlua::Error::external("could not determine current branch"))?
                    .to_string()
            }
        };

        let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, allowed_types| {
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                let username = username_from_url.unwrap_or("git");
                Cred::ssh_key_from_agent(username)
            } else if allowed_types.contains(git2::CredentialType::DEFAULT) {
                Cred::default()
            } else {
                Err(git2::Error::from_str("no valid credential type"))
            }
        });

        let mut push_opts = PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote
            .push(&[&refspec], Some(&mut push_opts))
            .map_err(mlua::Error::external)?;

        Ok(true)
    })
    .await
}

#[derive(Debug, Default, FromDynamic)]
struct ResetOptions {
    #[dynamic(default = "default_mixed")]
    mode: String,
    #[dynamic(default = "default_head")]
    target: String,
}

fn default_mixed() -> String {
    "mixed".to_string()
}

fn default_head() -> String {
    "HEAD".to_string()
}

async fn reset<'lua>(
    _: &'lua Lua,
    (path, options): (String, Option<Value<'_>>),
) -> mlua::Result<bool> {
    let opts: ResetOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => ResetOptions::default(),
    };

    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;

        let reset_type = match opts.mode.to_lowercase().as_str() {
            "soft" => ResetType::Soft,
            "mixed" => ResetType::Mixed,
            "hard" => ResetType::Hard,
            _ => {
                return Err(mlua::Error::external(
                    "mode must be 'soft', 'mixed', or 'hard'",
                ))
            }
        };

        let obj = repo
            .revparse_single(&opts.target)
            .map_err(mlua::Error::external)?;

        repo.reset(&obj, reset_type, None)
            .map_err(mlua::Error::external)?;

        Ok(true)
    })
    .await
}

async fn rebase<'lua>(_: &'lua Lua, (path, upstream): (String, String)) -> mlua::Result<bool> {
    smol::unblock(move || {
        let repo = Repository::discover(&path).map_err(mlua::Error::external)?;

        let upstream_ref = repo
            .revparse_single(&upstream)
            .map_err(mlua::Error::external)?;
        let upstream_commit = upstream_ref
            .peel_to_commit()
            .map_err(mlua::Error::external)?;
        let upstream_annotated = repo
            .find_annotated_commit(upstream_commit.id())
            .map_err(mlua::Error::external)?;

        let mut rebase = repo
            .rebase(None, Some(&upstream_annotated), None, None)
            .map_err(mlua::Error::external)?;

        let signature = repo.signature().map_err(mlua::Error::external)?;

        while let Some(op) = rebase.next() {
            op.map_err(mlua::Error::external)?;
            rebase
                .commit(None, &signature, None)
                .map_err(mlua::Error::external)?;
        }

        rebase.finish(None).map_err(mlua::Error::external)?;

        Ok(true)
    })
    .await
}
