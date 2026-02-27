---
tags:
  - overlay
---

# `ToggleTabOverlay`

{{since('nightly')}}

Toggles the visibility of the current tab-level overlay. If the overlay is
visible, it will be hidden. If it is hidden, it will be shown. If there is no
tab-level overlay for the current tab, this action does nothing.

When a tab-level overlay is hidden, it remains alive in the background and can
be restored without losing its state.

```lua
local wezterm = require 'wezterm'

config.keys = {
  {
    key = 'o',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.ToggleTabOverlay,
  },
}
```
