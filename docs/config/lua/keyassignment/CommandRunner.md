---
tags:
  - overlay
---

# `CommandRunner`

{{since('nightly')}}

Runs a list of non-interactive commands in a dedicated overlay with a list
view and a per-command output view. This is intended for batch-style commands
and viewing streaming logs, rather than interactive programs. Each command is
started once when the overlay opens, and you can rerun or kill individual
commands from the UI.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  keys = {
    {
      key = 'r',
      mods = 'CTRL|SHIFT',
      action = act.CommandRunner {
        commands = {
          {
            title = 'Build',
            args = { 'cargo', 'build' },
            cwd = '/path/to/repo',
          },
          {
            title = 'Tests',
            args = { 'cargo', 'test', '--all' },
          },
        },
        auto_close_on_success = false,
      },
    },
  },
}
```

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `commands` | list | n/a | List of `CommandRunnerCommand` entries |
| `auto_close_on_success` | boolean | `false` | Close the overlay when all commands finish successfully |

## CommandRunnerCommand

Each entry in `commands` can have:

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `title` | string | no | args[1] | Display name in the list view |
| `args` | list | yes | - | Command and arguments, with the program as the first element |
| `cwd` | string | no | `nil` | Working directory |
| `set_environment_variables` | map | no | `{}` | Environment variables to set for the command |

## Keys

The overlay is modal with a list view, output view, and filter input.

List view:
* <kbd>j</kbd>/<kbd>k</kbd> or arrows: move selection
* <kbd>Enter</kbd>: open output view
* <kbd>r</kbd>: rerun selected command
* <kbd>Ctrl</kbd>+<kbd>C</kbd>: kill selected command
* <kbd>q</kbd>: quit (prompts if commands are running)

Output view:
* <kbd>j</kbd>/<kbd>k</kbd> or arrows: move by logical lines
* <kbd>n</kbd>/<kbd>N</kbd>: next/previous match
* <kbd>y</kbd>: copy the current line (accepts a numeric prefix)
* <kbd>/</kbd>: open filter input
* <kbd>q</kbd>: return to list view

Filter input:
* <kbd>Enter</kbd>: apply filter and return to output view
* <kbd>Escape</kbd>: cancel input (restores the prior filter)
* <kbd>Ctrl</kbd>+<kbd>U</kbd>: clear pattern
* <kbd>Ctrl</kbd>+<kbd>R</kbd>: cycle search mode
* <kbd>Tab</kbd>/<kbd>Shift</kbd>+<kbd>Tab</kbd>: increase/decrease context
