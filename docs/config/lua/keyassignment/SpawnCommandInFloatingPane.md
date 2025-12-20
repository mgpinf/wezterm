# `SpawnCommandInFloatingPane`

{{since('nightly')}}

Spawn a new floating pane into the current tab. A tab can have one floating pane.
If a floating pane is active, it occupies the entire area of the tab, obscuring
any tiled panes or zoomed panes.

The rendering priority for a tab is:
1. Tab Overlay (e.g. Launcher, InputSelector)
2. Floating Pane
3. Zoomed Pane
4. Tiled Panes

The argument is a `SpawnCommand` struct that is discussed in more
detail in the [SpawnCommand](../SpawnCommand.md) docs.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  -- Start `top` in a floating pane
  {
    key = 'f',
    mods = 'LEADER',
    action = act.SpawnCommandInFloatingPane {
      args = { 'top' },
    },
  },
}
```
