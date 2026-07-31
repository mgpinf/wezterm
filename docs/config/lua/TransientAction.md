---
tags:
  - transient
---

# `TransientAction`

{{since('nightly')}}

The `TransientAction` struct specifies an action that can be triggered from a
transient-style overlay.

It is a lua object with the following fields:
* `key` - text to enter in order to trigger the action
* `description` - text to describe the action
* `action` - an event callback registered via `wezterm.action_callback`.  The
  callback's function signature is `(window, pane, result)` where `window` and
  `pane` are the [Window](./window/index.md) and [Pane](./pane/index.md)
  objects from the current pane and window. When used with `TransientMenu`,
  `result` is a [TransientResult](./TransientResult.md) object. When used with
  `SelectorActions`, it is a
  [SelectorActionsResult](./SelectorActionsResult.md) object
* `keep_overlay` - optional boolean (default: `false`). When set to `true`, the
  overlay will remain open after the action callback is triggered,
  allowing the user to continue interacting with the menu. When `false` (the
  default), the overlay closes after the action fires.


Example of a `TransientAction` object:

```lua
local action = {
  key = 'l',
  description = 'Logs',
  action = wezterm.action_callback(function(window, pane, result)
    wezterm.log_info(result)
  end),
}
```

Example with `keep_overlay`:

```lua
local refresh_action = {
  key = 'r',
  description = 'Refresh',
  keep_overlay = true,
  action = wezterm.action_callback(function(window, pane, result)
    wezterm.log_info 'refreshed without closing the overlay'
  end),
}
```
