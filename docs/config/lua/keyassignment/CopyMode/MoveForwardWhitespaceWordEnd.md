# CopyMode `MoveForwardWhitespaceWordEnd`

{{since('nightly')}}

Moves the CopyMode cursor forward to the end of a whitespace-delimited word. A
whitespace-delimited word is a maximal run of non-whitespace characters.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'E',
        mods = 'NONE',
        action = act.CopyMode 'MoveForwardWhitespaceWordEnd',
      },
    },
  },
}
```
