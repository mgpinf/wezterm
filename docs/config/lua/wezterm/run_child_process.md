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

See also [background_child_process](background_child_process.md)
