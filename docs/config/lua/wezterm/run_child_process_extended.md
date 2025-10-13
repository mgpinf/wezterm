---
title: wezterm.run_child_process_extended
tags:
 - utility
 - open
 - spawn
---
# `wezterm.run_child_process_extended(args)`

{{since('nightly')}}

This function accepts the following fields.
* `args` - the argument array specifying the command and its arguments.
* `cwd` - the current working directory to set for the command. This
  is an optional argument
* `set_environment_variables` - sets additional environment variables
  in the environment for the child process to be run

```lua
local wezterm = require 'wezterm'

local success, stdout, stderr = wezterm.run_child_process_extended {
  args = { 'ls', '-l' },
  cwd = wezterm.config_dir,
  set_environment_variables = {
    SOMETHING = 'a value',
  },
}
```
