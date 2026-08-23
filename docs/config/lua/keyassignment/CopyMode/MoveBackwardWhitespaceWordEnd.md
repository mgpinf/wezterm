# CopyMode `MoveBackwardWhitespaceWordEnd`

{{since('nightly')}}

Moves the CopyMode cursor backward to the end of the preceding
whitespace-delimited word. A whitespace-delimited word is a maximal run of
non-whitespace characters.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'E',
        mods = 'ALT',
        action = act.CopyMode 'MoveBackwardWhitespaceWordEnd',
      },
    },
  },
}
```
