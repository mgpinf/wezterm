---
title: wezterm.run_child_process
tags:
 - utility
 - open
 - spawn
---
# `wezterm.run_child_process(args)`

{{since('20200503-171512-b13ef15f')}}

This function accepts an argument list; it will attempt to spawn that command
and will return a tuple consisting of the boolean success of the invocation,
the stdout data and the stderr data.

```lua
local wezterm = require 'wezterm'

local success, stdout, stderr = wezterm.run_child_process { 'ls', '-l' }
```

{{since('nightly')}}

You can now pass a table with named fields for more control:

* `args` - the argument array specifying the command and its arguments (required)
* `cwd` - the current working directory to set for the command (optional)
* `set_environment_variables` - a table of environment variables to set for the
  child process (optional)
* `trim_newline` - if `true`, trims trailing newlines (`\n` and `\r`) from stdout
  and stderr (optional, defaults to `false`)
* `stdin` - a string to pipe to the child process's standard input (optional).
  The data is written to the process's stdin pipe and then the pipe is closed
  (sending EOF) before collecting the output.

```lua
local wezterm = require 'wezterm'

local success, stdout, stderr = wezterm.run_child_process {
  args = { 'ls', '-l' },
  cwd = wezterm.config_dir,
  set_environment_variables = {
    SOMETHING = 'a value',
  },
}
```

The `stdin` option allows you to pipe data to a child process without
needing a shell wrapper:

```lua
local wezterm = require 'wezterm'

-- Pipe a string to cat's stdin
local success, stdout, stderr = wezterm.run_child_process {
  args = { 'cat' },
  stdin = 'hello from stdin',
  trim_newline = true,
}

wezterm.log_info('Output: ' .. stdout) -- Output: hello from stdin
```

The `trim_newline` option is useful when you want to use the output directly
without trailing newlines:

```lua
local wezterm = require 'wezterm'

-- Without trim_newline, stdout would be "Hello\n"
-- With trim_newline, stdout is "Hello"
local success, stdout, stderr = wezterm.run_child_process {
  args = { 'echo', 'Hello' },
  trim_newline = true,
}

wezterm.log_info('Output: [' .. stdout .. ']') -- Output: [Hello]
```

See also [background_child_process](background_child_process.md)
