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

The argument accepts all fields from [SpawnCommand](../SpawnCommand.md), plus an
additional `replace_current` option:

* `replace_current` - If `true`, and a floating pane already exists in the tab,
  the existing floating pane will be closed and replaced with the new one.
  If `false` (the default), spawning a floating pane when one already exists
  will do nothing.

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
  -- Start `htop` in a floating pane, replacing any existing floating pane
  {
    key = 'F',
    mods = 'LEADER',
    action = act.SpawnCommandInFloatingPane {
      args = { 'htop' },
      replace_current = true,
    },
  },
}
```
