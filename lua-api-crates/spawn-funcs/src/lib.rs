use std::collections::HashMap;
use std::path::PathBuf;

use bstr::BString;
use config::lua::get_or_create_module;
use config::lua::mlua::{self, Lua, Value as LuaValue};
use futures::future::join_all;
use luahelper::impl_lua_conversion_dynamic;
use smol::io::AsyncWriteExt;
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
    #[dynamic(default)]
    trim_newline: bool,
    #[dynamic(default)]
    stdin: Option<String>,
}

impl_lua_conversion_dynamic!(ChildProcessOptions);

#[derive(Debug, FromDynamic, ToDynamic, Clone)]
struct ProcessResult {
    success: bool,
    stdout: String,
    stderr: String,
}

impl_lua_conversion_dynamic!(ProcessResult);

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let wezterm_mod = get_or_create_module(lua, "wezterm")?;
    wezterm_mod.set("open_with", lua.create_function(open_with)?)?;
    wezterm_mod.set(
        "run_child_process",
        lua.create_async_function(run_child_process)?,
    )?;
    wezterm_mod.set(
        "run_child_processes",
        lua.create_async_function(run_child_processes)?,
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
                    trim_newline: false,
                    stdin: None,
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
        cmd.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);
    }

    let output = if let Some(stdin_data) = &opts.stdin {
        cmd.stdin(smol::process::Stdio::piped());
        cmd.stdout(smol::process::Stdio::piped());
        cmd.stderr(smol::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(mlua::Error::external)?;
        let mut child_stdin = child.stdin.take().unwrap();
        child_stdin
            .write_all(stdin_data.as_bytes())
            .await
            .map_err(mlua::Error::external)?;
        drop(child_stdin);
        child.output().await.map_err(mlua::Error::external)?
    } else {
        cmd.output().await.map_err(mlua::Error::external)?
    };

    let (stdout, stderr) = if opts.trim_newline {
        (
            trim_trailing_newlines(&output.stdout),
            trim_trailing_newlines(&output.stderr),
        )
    } else {
        (output.stdout.clone(), output.stderr.clone())
    };

    Ok((output.status.success(), stdout.into(), stderr.into()))
}

fn trim_trailing_newlines(data: &[u8]) -> Vec<u8> {
    let mut result = data.to_vec();
    while matches!(result.last(), Some(b'\n' | b'\r')) {
        result.pop();
    }
    result
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
        cmd.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);
    }

    cmd.stdin(smol::process::Stdio::null())
        .spawn()
        .map_err(mlua::Error::external)?;

    Ok(())
}

async fn execute_command(opts: &ChildProcessOptions) -> ProcessResult {
    if opts.args.is_empty() {
        return ProcessResult {
            success: false,
            stdout: String::new(),
            stderr: "args cannot be empty".to_string(),
        };
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
        cmd.creation_flags(winapi::um::winbase::CREATE_NO_WINDOW);
    }

    let output = if let Some(stdin_data) = &opts.stdin {
        cmd.stdin(smol::process::Stdio::piped());
        cmd.stdout(smol::process::Stdio::piped());
        cmd.stderr(smol::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return ProcessResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Failed to spawn command: {}", e),
                }
            }
        };

        let mut child_stdin = child.stdin.take().unwrap();
        if let Err(e) = child_stdin.write_all(stdin_data.as_bytes()).await {
            return ProcessResult {
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to write data to stdin: {}", e),
            };
        }
        drop(child_stdin);

        match child.output().await {
            Ok(output) => output,
            Err(e) => {
                return ProcessResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Failed to execute command: {}", e),
                }
            }
        }
    } else {
        match cmd.output().await {
            Ok(output) => output,
            Err(e) => {
                return ProcessResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Failed to execute command: {}", e),
                }
            }
        }
    };

    let (stdout, stderr) = if opts.trim_newline {
        (
            String::from_utf8_lossy(&trim_trailing_newlines(&output.stdout)).into_owned(),
            String::from_utf8_lossy(&trim_trailing_newlines(&output.stderr)).into_owned(),
        )
    } else {
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    ProcessResult {
        success: output.status.success(),
        stdout,
        stderr,
    }
}

async fn run_child_processes<'lua>(
    lua: &'lua Lua,
    value: LuaValue<'lua>,
) -> mlua::Result<Vec<ProcessResult>> {
    let commands = match &value {
        LuaValue::Table(table) => {
            let mut cmds = Vec::new();
            for pair in table.clone().pairs::<i64, LuaValue>() {
                let (_, cmd_value) = pair?;
                let opts = parse_child_process_args(lua, cmd_value)?;
                cmds.push(opts);
            }
            cmds
        }
        _ => {
            return Err(mlua::Error::external(
                "run_child_processes expects an array of command specifications",
            ));
        }
    };

    if commands.is_empty() {
        return Err(mlua::Error::external(
            "run_child_processes requires at least one command",
        ));
    }

    let futures: Vec<_> = commands.iter().map(|opts| execute_command(opts)).collect();
    let results = join_all(futures).await;

    Ok(results)
}
