---
tags:
  - transient
---

# `TransientResult`

{{since('nightly')}}

The `TransientResult` is a lua object with the following fields:
* `entries` - a table mapping flag names to their values. Each key is
  the `flag` string from one of the below entries:
  * [TransientSwitch](./TransientSwitch.md) — value is `true` or `false`
  * [TransientOption](./TransientOption.md) — value is the selected string, or `nil`
  * [TransientCyclicSwitch](./TransientCyclicSwitch.md) — value is the selected choice string, or `nil`


Example of `TransientResult` object:
```lua
local result = {
  entries = {
    ['--follow'] = true,
    ['--tail='] = '100',
  },
}
```

Example of using `TransientResult`:
```lua
wezterm.action_callback(function(window, pane, result)
  local follow = result.entries['--follow']
  local tail = result.entries['--tail=']
end)
```
