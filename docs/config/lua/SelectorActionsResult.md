---
tags:
  - transient
  - selector
---

# `SelectorActionsResult`

{{since('nightly')}}

The `SelectorActionsResult` is a Lua array containing the IDs of the selected
choices.


Example of a `SelectorActionsResult` array:

```lua
local result = {
  'choice1',
  'choice2',
}
```

The result is always an array, including when `multiple` is `false`:

```lua
wezterm.action_callback(function(window, pane, result)
  for _, id in ipairs(result) do
    wezterm.log_info(id)
  end
end)
```
