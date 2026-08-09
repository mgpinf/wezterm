---
tags:
  - transient
---

# `TransientResult`

{{since('nightly')}}

The `TransientResult` is a lua table mapping flag names to their values. Each key
is the `flag` string from one of the following entries and the each value can
be a valid lua value allowed against the corresponding entry:
* [TransientSwitch](./TransientSwitch.md) - value is `true` or `false`
* [TransientOption](./TransientOption.md) - value is the selected string, or `nil`
* [TransientCyclicSwitch](./TransientCyclicSwitch.md) - value is the selected choice string, or nil

Example of `TransientResult` object:
```lua
local result = {
  ['--follow'] = true,
  ['--tail='] = '100',
}
```

Example of using `TransientResult`:
```lua
wezterm.action_callback(function(window, pane, result)
  local follow = result['--follow']
  local tail = result['--tail=']
end)
```
