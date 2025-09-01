use std::collections::HashMap;
use std::path::PathBuf;

use bstr::BString;
use config::lua::get_or_create_module;
use config::lua::mlua::{self, Lua};
use luahelper::impl_lua_conversion_dynamic;
use wezterm_dynamic::{FromDynamic, ToDynamic};

#[derive(Debug, FromDynamic, ToDynamic, Clone)]
struct ExtendedChildProcess {
    args: Vec<String>,
    cwd: Option<PathBuf>,
    set_environment_variables: Option<HashMap<String, String>>,
}

impl_lua_conversion_dynamic!(ExtendedChildProcess);

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let wezterm_mod = get_or_create_module(lua, "wezterm")?;
    wezterm_mod.set("open_with", lua.create_function(open_with)?)?;
    wezterm_mod.set(
        "run_child_process",
        lua.create_async_function(run_child_process)?,
    )?;
    wezterm_mod.set(
        "background_child_process",
        lua.create_async_function(background_child_process)?,
    )?;
    wezterm_mod.set(
        "run_child_process_extended",
        lua.create_async_function(run_child_process_extended)?,
    )?;
    Ok(())
}

fn open_with<'lua>(_: &'lua Lua, (url, app): (String, Option<String>)) -> mlua::Result<()> {
    if let Some(app) = app {
        wezterm_open_url::open_with(&url, &app);
    } else {
        wezterm_open_url::open_url(&url);
    }
    Ok(())
}

async fn run_child_process<'lua>(
    _: &'lua Lua,
    args: Vec<String>,
) -> mlua::Result<(bool, BString, BString)> {
    let mut cmd = smol::process::Command::new(&args[0]);

    if args.len() > 1 {
        cmd.args(&args[1..]);
    }

    #[cfg(windows)]
    {
        use smol::process::windows::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output().await.map_err(mlua::Error::external)?;

    Ok((
        output.status.success(),
        output.stdout.into(),
        output.stderr.into(),
    ))
}

async fn background_child_process<'lua>(_: &'lua Lua, args: Vec<String>) -> mlua::Result<()> {
    let mut cmd = smol::process::Command::new(&args[0]);

    if args.len() > 1 {
        cmd.args(&args[1..]);
    }

    #[cfg(windows)]
    {
        use smol::process::windows::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.stdin(smol::process::Stdio::null())
        .spawn()
        .map_err(mlua::Error::external)?;

    Ok(())
}

async fn run_child_process_extended<'lua>(
    _: &'lua Lua,
    extended_child_process: ExtendedChildProcess,
) -> mlua::Result<(bool, BString, BString)> {
    let args = &extended_child_process.args;
    let mut cmd = smol::process::Command::new(&args[0]);

    if args.len() > 1 {
        cmd.args(&args[1..]);
    }

    if let Some(env_vars) = extended_child_process.set_environment_variables.as_ref() {
        cmd.envs(env_vars);
    }

    if let Some(cwd) = extended_child_process.cwd.as_ref() {
        cmd.current_dir(cwd);
    }

    #[cfg(windows)]
    {
        use smol::process::windows::CommandExt;
        cmd.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);
    }

    let output = cmd.output().await.map_err(mlua::Error::external)?;

    Ok((
        output.status.success(),
        output.stdout.into(),
        output.stderr.into(),
    ))
}
