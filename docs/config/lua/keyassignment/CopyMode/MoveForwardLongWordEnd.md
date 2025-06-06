# CopyMode `MoveForwardLongWordEnd`

{{since('nightly')}}

Moves the CopyMode cursor position forward to the end of WORD.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'E',
        mods = 'SHIFT',
        action = act.CopyMode 'MoveForwardLongWordEnd',
      },
    },
  },
}
```

