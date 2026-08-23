# CopyMode `MoveForwardWhitespaceWord`

{{since('nightly')}}

Moves the CopyMode cursor forward to the beginning of the next
whitespace-delimited word. A whitespace-delimited word is a maximal run of
non-whitespace characters, so punctuation in values such as paths, URLs and
command-line arguments does not introduce additional word boundaries.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'W',
        mods = 'NONE',
        action = act.CopyMode 'MoveForwardWhitespaceWord',
      },
    },
  },
}
```
