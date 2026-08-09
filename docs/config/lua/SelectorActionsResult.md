---
tags:
  - transient
  - selector
---

# `SelectorActionsResult`

{{since('nightly')}}

The `SelectorActionsResult` is a Lua array containing the selected choices.
Each choice is a table with a `label` field, an optional `id` field, and an
optional `metadata` field. The metadata is a table of string key-value pairs
that were attached to the original choice.


Example of a `SelectorActionsResult` array:

```lua
local result = {
  {
    label = 'choice1',
    id = 'random_id',
    metadata = { status = 'running' },
  },
  { label = 'choice2' },
}
```

The result is always an array, including when `multiple` is `false`:

```lua
wezterm.action_callback(function(window, pane, result)
  for _, choice in ipairs(result) do
    wezterm.log_info(choice.label)
  end
end)
```
