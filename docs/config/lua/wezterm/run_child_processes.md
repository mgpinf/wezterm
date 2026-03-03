---
title: wezterm.run_child_processes
tags:
 - utility
 - open
 - spawn
---
# `wezterm.run_child_processes(commands)`

{{since('nightly')}}

This function accepts an array of command specifications and runs them all
**in parallel**. It returns when all processes have completed, providing an
array of result tables in the same order as the input commands.

This is useful when you need to run multiple independent commands and want
them to execute concurrently rather than sequentially, reducing total
execution time.

## Basic Usage

```lua
local wezterm = require 'wezterm'

local results = wezterm.run_child_processes {
  { 'git', 'branch', '--show-current' },
  { 'node', '--version' },
  { 'date', '+%H:%M' },
}

-- All three commands run in parallel
-- results[1] corresponds to 'git branch --show-current'
-- results[2] corresponds to 'node --version'
-- results[3] corresponds to 'date +%H:%M'

for i, result in ipairs(results) do
  if result.success then
    wezterm.log_info('Command ' .. i .. ' output: ' .. result.stdout)
  else
    wezterm.log_error('Command ' .. i .. ' failed: ' .. result.stderr)
  end
end
```

## Return Value

The function returns an array of tables. Each table contains:

| Field | Type | Description |
|-------|------|-------------|
| `success` | boolean | `true` if the process exited with code 0 |
| `stdout` | string | The captured standard output |
| `stderr` | string | The captured standard error |

## Extended Syntax

Each command in the array can use the extended syntax with named fields:

```lua
local wezterm = require 'wezterm'

local results = wezterm.run_child_processes {
  { 'git', 'status', '--short' },
  {
    args = { 'npm', 'list', '--depth=0' },
    cwd = '/path/to/project',
  },
  {
    args = { 'env' },
    set_environment_variables = {
      MY_VAR = 'my_value',
    },
  },
}
```

The extended syntax supports:

* `args` - the argument array specifying the command and its arguments (required)
* `cwd` - the current working directory to set for the command (optional)
* `set_environment_variables` - a table of environment variables to set for
  the child process (optional)
* `trim_newline` - if `true`, trims trailing newlines (`\n` and `\r`) from stdout
  and stderr (optional, defaults to `false`)
* `stdin` - a string to pipe to the child process's standard input (optional)

## Example: Status Bar with Multiple Data Sources

```lua
local wezterm = require 'wezterm'

wezterm.on('update-status', function(window, pane)
  local results = wezterm.run_child_processes {
    { args = { 'git', 'branch', '--show-current' }, trim_newline = true },
    {
      args = { 'kubectl', 'config', 'current-context' },
      trim_newline = true,
    },
  }

  local git_branch = results[1].success and results[1].stdout or '?'
  local k8s_context = results[2].success and results[2].stdout or '?'

  window:set_right_status(git_branch .. ' | ' .. k8s_context)
end)
```

## Example: Checking Multiple Hosts

```lua
local wezterm = require 'wezterm'

local hosts = { 'server1', 'server2', 'server3' }
local commands = {}

for _, host in ipairs(hosts) do
  table.insert(commands, {
    'ssh',
    '-o',
    'ConnectTimeout=2',
    '-o',
    'BatchMode=yes',
    host,
    'true',
  })
end

local results = wezterm.run_child_processes(commands)

for i, result in ipairs(results) do
  local status = result.success and '🟢 online' or '🔴 offline'
  wezterm.log_info(hosts[i] .. ': ' .. status)
end
```

## Comparison with run_child_process

| Aspect | `run_child_process` | `run_child_processes` |
|--------|---------------------|----------------------|
| Input | Single command | Array of commands |
| Execution | Sequential | Parallel |
| Return | `success, stdout, stderr` | Array of result tables |
| Use case | Single command | Multiple independent commands |

When running multiple commands with `run_child_process`, each must complete
before the next starts:

```lua
-- Sequential: total time = sum of all command times
local r1 = wezterm.run_child_process { 'cmd1' } -- 1 second
local r2 = wezterm.run_child_process { 'cmd2' } -- 1 second
local r3 = wezterm.run_child_process { 'cmd3' } -- 1 second
-- Total: ~3 seconds
```

With `run_child_processes`, all commands run concurrently:

```lua
-- Parallel: total time = max of all command times
local results = wezterm.run_child_processes {
  { 'cmd1' }, -- 1 second
  { 'cmd2' }, -- 1 second
  { 'cmd3' }, -- 1 second
}
-- Total: ~1 second
```

## Comparison with action.Multiple and action_callback

You might consider using `wezterm.action.Multiple` with multiple
`wezterm.action_callback` calls to run child processes. However, there are
important differences:

| Aspect | `run_child_processes` | `action.Multiple` with callbacks |
|--------|----------------------|----------------------------------|
| Execution | Parallel, awaited | Parallel, fire-and-forget |
| Waits for completion | Yes, returns when all done | No, callbacks are detached |
| Returns results | Yes, array of results | No, cannot collect results |
| Use case | Gathering data from multiple commands | Triggering independent side-effects |

When `action.Multiple` executes `action_callback` actions, each callback is
spawned asynchronously and detached—the loop does not wait for callbacks to
complete:

```lua
-- Both callbacks fire immediately and run in background
-- You CANNOT collect their results
wezterm.action.Multiple {
  wezterm.action_callback(function(window, pane)
    wezterm.run_child_process { 'cmd1' }
  end),
  wezterm.action_callback(function(window, pane)
    wezterm.run_child_process { 'cmd2' }
  end),
}
```

**Use `run_child_processes`** when you need to:

- Run multiple commands and collect their output
- Make decisions based on command results
- Build status bar content from multiple data sources

**Use `action.Multiple` with callbacks** when you need to:

- Trigger multiple independent side-effects from a key binding
- Fire-and-forget operations where results don't matter
