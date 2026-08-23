# CopyMode `MoveToEndOfLogicalLineContent`

{{since('nightly')}}

Moves the CopyMode cursor to the last non-space cell of the current logical
line. A logical line joins physical terminal rows that are connected by soft
wrapping. The cursor moves to the start of the logical line when the line has no
non-space content.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'End',
        mods = 'ALT',
        action = act.CopyMode 'MoveToEndOfLogicalLineContent',
      },
    },
  },
}
```
