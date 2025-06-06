# CopyMode `MoveBackwardLongWord`

{{since('nightly')}}

Moves the CopyMode cursor position one WORD to the left.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'B',
        mods = 'SHIFT',
        action = act.CopyMode 'MoveBackwardLongWord',
      },
    },
  },
}
```
