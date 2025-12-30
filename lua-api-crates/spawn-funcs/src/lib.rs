use std::collections::HashMap;
use std::path::PathBuf;

use bstr::BString;
use config::lua::get_or_create_module;
use config::lua::mlua::{self, Lua, Value as LuaValue};
use luahelper::impl_lua_conversion_dynamic;
use wezterm_dynamic::{FromDynamic, ToDynamic};

/// Extended options for running a child process.
/// Used when passing a table with named fields instead of just an args array.
#[derive(Debug, FromDynamic, ToDynamic, Clone, Default)]
struct ChildProcessOptions {
    args: Vec<String>,
    #[dynamic(default)]
    cwd: Option<PathBuf>,
    #[dynamic(default)]
    set_environment_variables: Option<HashMap<String, String>>,
}

impl_lua_conversion_dynamic!(ChildProcessOptions);

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

/// Parse the input value which can be either:
/// - An array of strings (simple syntax): {"ls", "-l"}
/// - A table with options (extended syntax): {args = {"ls", "-l"}, cwd = "/tmp"}
fn parse_child_process_args<'lua>(
    lua: &'lua Lua,
    value: LuaValue<'lua>,
) -> mlua::Result<ChildProcessOptions> {
    match &value {
        LuaValue::Table(table) => {
            // Check if this is an array (has numeric key 1) or a table with named fields
            if table.contains_key(1)? {
                // It's an array of strings - simple syntax
                let args: Vec<String> = lua.unpack(value)?;
                Ok(ChildProcessOptions {
                    args,
                    cwd: None,
                    set_environment_variables: None,
                })
            } else {
                // It's a table with named fields - extended syntax
                let opts: ChildProcessOptions = lua.unpack(value)?;
                Ok(opts)
            }
        }
        _ => Err(mlua::Error::external(
            "run_child_process expects an array of strings or a table with 'args' field",
        )),
    }
}

/// Run a child process and wait for it to complete.
///
/// Accepts either:
/// - Simple syntax: `wezterm.run_child_process{"ls", "-l"}`
/// - Extended syntax: `wezterm.run_child_process{args = {"ls", "-l"}, cwd = "/tmp"}`
async fn run_child_process<'lua>(
    lua: &'lua Lua,
    value: LuaValue<'lua>,
) -> mlua::Result<(bool, BString, BString)> {
    let opts = parse_child_process_args(lua, value)?;

    if opts.args.is_empty() {
        return Err(mlua::Error::external("args cannot be empty"));
    }

    let mut cmd = smol::process::Command::new(&opts.args[0]);

    if opts.args.len() > 1 {
        cmd.args(&opts.args[1..]);
    }

    if let Some(cwd) = opts.cwd.as_ref() {
        cmd.current_dir(cwd);
    }

    if let Some(env_vars) = opts.set_environment_variables.as_ref() {
        cmd.envs(env_vars);
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

/// Spawn a child process in the background without waiting for it.
///
/// Accepts either:
/// - Simple syntax: `wezterm.background_child_process{"ls", "-l"}`
/// - Extended syntax: `wezterm.background_child_process{args = {"ls", "-l"}, cwd = "/tmp"}`
async fn background_child_process<'lua>(lua: &'lua Lua, value: LuaValue<'lua>) -> mlua::Result<()> {
    let opts = parse_child_process_args(lua, value)?;

    if opts.args.is_empty() {
        return Err(mlua::Error::external("args cannot be empty"));
    }

    let mut cmd = smol::process::Command::new(&opts.args[0]);

    if opts.args.len() > 1 {
        cmd.args(&opts.args[1..]);
    }

    if let Some(cwd) = opts.cwd.as_ref() {
        cmd.current_dir(cwd);
    }

    if let Some(env_vars) = opts.set_environment_variables.as_ref() {
        cmd.envs(env_vars);
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
