# CopyMode `MoveForwardLongWord`

{{since('nightly')}}

Moves the CopyMode cursor position one WORD to the right.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'W',
        mods = 'SHIFT',
        action = act.CopyMode 'MoveForwardLongWord',
      },
    },
  },
}
```

