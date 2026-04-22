---
tags:
  - copy_mode
  - shell_integration
---

# CopyMode `MoveBackwardCommandBlock`

{{since('nightly')}}

Moves the CopyMode cursor position to the start of the previous shell *command
block*: the row that begins with the previous OSC 133 `Prompt` zone. If a
selection is active, it is extended in command-block-sized increments.

A "command block" is the contiguous run of `Prompt`, `Input` and `Output`
semantic zones produced by a single shell command. See
[Shell Integration](../../../../shell-integration.md) for details on how shells
emit OSC 133 sequences, and the
[`SetSelectionMode = "CommandBlock"`](SetSelectionMode.md) action for
selecting an entire block.

If the pane has no semantic zones at all (no shell integration, or the cursor
is positioned before any prompt has been emitted) this action does nothing.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'PageUp',
        mods = 'NONE',
        action = act.CopyMode 'MoveBackwardCommandBlock',
      },
    },
  },
}
```
