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
          -- Simple syntax: just an array of strings
          { 'git', 'status' },
          -- Extended syntax: object with options
          {
            title = 'Build',
            args = { 'cargo', 'build' },
            cwd = '/path/to/repo',
          },
          -- Can mix both syntaxes
          { 'cargo', 'test', '--all' },
          {
            title = 'Lint',
            args = { 'cargo', 'clippy' },
            set_environment_variables = { RUST_BACKTRACE = '1' },
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
| `alphabet` | string | `"1234567890abcdefhilmnopstuvwxyz"` | Characters used to build quick-select labels in the list view |

## CommandRunnerCommand

Each entry in `commands` can be specified in two ways:

### Simple Syntax

An array of strings where the first element is the command:

```lua
local cmd = { 'cargo', 'build', '--release' }
```

### Extended Syntax

An object with named fields for additional options:

```lua
local cmd = {
  title = 'Build Release',
  args = { 'cargo', 'build', '--release' },
  cwd = '/path/to/project',
  set_environment_variables = { RUST_LOG = 'debug' },
}
```

### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `title` | string | no | args[1] | Display name in the list view |
| `args` | list | yes | - | Command and arguments (must not be empty) |
| `cwd` | string | no | `nil` | Working directory |
| `set_environment_variables` | map | no | `{}` | Environment variables to set for the command |

## Keys

The overlay is modal with a list view, output view, and filter input.

List view:
* <kbd>j</kbd>/<kbd>k</kbd> or arrows: move selection
* Keys in `alphabet`: open the matching entry (single- or double-key)
* <kbd>Enter</kbd>: open output view
* <kbd>r</kbd>: rerun selected command
* <kbd>Ctrl</kbd>+<kbd>C</kbd>: kill selected command
* <kbd>Ctrl</kbd>+<kbd>L</kbd>: clear output for selected command
* <kbd>q</kbd>: quit (prompts if commands are running)

Note: keys present in `alphabet` are reserved for quick selection; remove any
navigation keys you want to keep from the alphabet string.

Output view:
* <kbd>j</kbd>/<kbd>k</kbd> or arrows: move by logical lines
* <kbd>n</kbd>/<kbd>N</kbd>: next/previous match
* <kbd>y</kbd>: copy the current line (accepts a numeric prefix)
* <kbd>/</kbd>: open filter input
* <kbd>Ctrl</kbd>+<kbd>L</kbd>: clear output for current command
* <kbd>q</kbd>: return to list view

Filter input:
* <kbd>Enter</kbd>: apply filter and return to output view
* <kbd>Escape</kbd>: cancel input (restores the prior filter)
* <kbd>Ctrl</kbd>+<kbd>U</kbd>: clear pattern
* <kbd>Ctrl</kbd>+<kbd>L</kbd>: clear output for current command
* <kbd>Ctrl</kbd>+<kbd>R</kbd>: cycle search mode
* <kbd>Tab</kbd>/<kbd>Shift</kbd>+<kbd>Tab</kbd>: increase/decrease context
