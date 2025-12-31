---
tags:
  - float
---

# `ToggleFloatingPane`

{{since('nightly')}}

Toggles the visibility of the floating pane in the current tab. If the floating
pane is visible, it will be hidden. If it is hidden, it will be shown. If there
is no floating pane, this action does nothing.

When a floating pane is hidden, the underlying tiled panes (or zoomed pane)
become visible and interactive. The floating pane remains alive in the
background and can be toggled back at any time.

```lua
local wezterm = require 'wezterm'

config.keys = {
  {
    key = 'f',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.ToggleFloatingPane,
  },
}
```

