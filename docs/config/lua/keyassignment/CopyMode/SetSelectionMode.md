---
tags:
  - copy_mode
  - shell_integration
---

# CopyMode `{ SetSelectionMode = MODE }`

{{since('20220624-141144-bd1b7c5d')}}

Sets the CopyMode selection mode.

MODE can be one of:

* `"Cell"` - selection expands a single cell at a time
* `"Word"` - selection expands by a word at a time
* `"Line"` - selection expands by a line at a time
* `"Block"` - selection expands to define a rectangular block using the starting point and current cursor position as the corners
* `"SemanticZone"` - selection expands to the current semantic zone. See [Shell Integration](../../../../shell-integration.md). {{since('20220903-194523-3bb1ed61', inline=True)}}.
* `"CommandBlock"` - selection expands to the entire shell *command block*: the contiguous run of `Prompt`, `Input` and `Output` semantic zones produced by a single shell command. Requires OSC 133 shell integration; see [Shell Integration](../../../../shell-integration.md). Both ends of the selection independently snap to whole command blocks, so moving the cursor across prompts grows or shrinks the selection by entire commands. This mode is only meaningful inside Copy Mode; mouse selection bindings using this mode are treated as a no-op. {{since('nightly', inline=True)}}.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'v',
        mods = 'NONE',
        action = act.CopyMode { SetSelectionMode = 'Cell' },
      },
    },
  },
}
```

See also: [ClearSelectionMode](ClearSelectionMode.md).
