# CopyMode `MoveBackwardLogicalLine`

{{since('nightly')}}

Moves the CopyMode cursor to the preceding logical line while preserving its
unwrapped logical column. A logical line joins physical terminal rows that are
connected by soft wrapping.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'UpArrow',
        mods = 'ALT',
        action = act.CopyMode 'MoveBackwardLogicalLine',
      },
    },
  },
}
```
