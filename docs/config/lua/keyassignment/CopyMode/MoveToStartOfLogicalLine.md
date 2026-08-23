# CopyMode `MoveToStartOfLogicalLine`

{{since('nightly')}}

Moves the CopyMode cursor to the first cell of the current logical line. A
logical line joins physical terminal rows that are connected by soft wrapping.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'Home',
        mods = 'ALT',
        action = act.CopyMode 'MoveToStartOfLogicalLine',
      },
    },
  },
}
```
