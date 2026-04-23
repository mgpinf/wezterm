---
tags:
  - copy_mode
  - shell_integration
---

# CopyMode `MoveBackwardCommandBlock`

{{since('nightly')}}

Moves the CopyMode cursor position to the start of a shell *command block*:
the row that begins with an OSC 133 `Prompt` zone. If the cursor is anywhere
past column 0 of the current block's prompt row (i.e. anywhere inside the
block except its very first cell), this snaps the cursor to that anchor;
otherwise it steps to the start of the previous block. If a selection is
active, it is extended in command-block-sized increments.

This mirrors the "snap to current, then step to previous" behavior of
[`MoveBackwardSemanticZone`](MoveBackwardSemanticZone.md), so repeated presses
walk back one block at a time regardless of where in the current block the
cursor started.

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
