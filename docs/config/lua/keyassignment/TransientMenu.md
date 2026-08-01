---
tags:
  - transient
---

# `TransientMenu`

{{since('nightly')}}

This creates an overlay with keyboard driven menu similar to
Emacs transient menus. 

We can view and set switches, options, and cyclic switches, then trigger an
action with the current state of those entries passed to its callback.

Entry keys can contain multiple characters. While an incomplete key prefix is
active, entries that cannot be reached from that prefix are dimmed. For matching
entries, the consumed portion of the displayed key is dimmed while the remaining
portion retains the normal key color. The
`transient_entry_non_matching_fg` [color setting](../../appearance.md#defining-your-own-colors)
controls the dimmed foreground.

`TransientMenu` accepts the following fields:

* `description` - text to display at the top of the menu
* `title` - optional, the title that will be set for the overlay pane
* `context` - optional, accepts a [TransientContext](../TransientContext.md) object
* `cancel` - optional event callback registered via `wezterm.action_callback`. The
  callback's function signature is `(window, pane)` where `window` and
  `pane` are the [Window](../window/index.md) and [Pane](../pane/index.md).
  Called when the user cancels the overlay
* `dimensions` - optional [OverlayDimensions](../OverlayDimensions.md) controlling
  the size of the overlay. Defaults to the full tab. {{since('nightly', inline=True)}}

For specifying menu entries, use **one** of the following:

* `sections` - list of [TransientSection](#transientsection) objects (for multi-section menus)
* `entries` - list of [TransientEntry](#transiententry) objects (for single-section menus)
  * `header` - optional, section header text (only valid with `entries`)

Note: You cannot combine `sections` with `entries` or `header`.

#### Single-section menus

For menus with a single section, use `entries` directly:

```lua
act.TransientMenu {
  description = 'My Menu',
  header = 'Actions', -- optional, defaults to empty string
  entries = {
    {
      type = 'switch',
      key = '-v',
      description = 'Verbose',
      flag = '--verbose',
    },
    { type = 'action', key = 'r', description = 'Run', action = callback },
  },
}
```

#### Multi-section menus

For menus with multiple sections, use `sections`:

```lua
act.TransientMenu {
  description = 'My Menu',
  sections = {
    {
      header = 'Flags',
      entries = {
        {
          type = 'switch',
          key = '-v',
          description = 'Verbose',
          flag = '--verbose',
        },
      },
    },
    {
      header = 'Actions',
      entries = {
        {
          type = 'action',
          key = 'r',
          description = 'Run',
          action = callback,
        },
      },
    },
  },
}
```


### `TransientSection`

`TransientSection` struct is a lua object with the following fields:
* `header` - text to describe the section
* `entries` - list of [TransientEntry](#transiententry) objects


### `TransientEntry`

`TransientEntry` is a lua object with a `type` field to specify the entry type,
and all other fields at the same level:

```lua
entries = {
  {
    type = 'switch',
    key = '-f',
    description = 'Follow',
    flag = '--follow',
    default = true,
  },
  {
    type = 'option',
    key = '-t',
    description = 'Tail',
    flag = '--tail=',
    default = '0',
  },
  {
    type = 'cyclic',
    key = '-c',
    description = 'Choice',
    flag = '--choice=',
    choices = { 'a', 'b' },
  },
  { type = 'action', key = 'l', description = 'Logs', action = callback },
}
```

The `type` field accepts:
* `"switch"` - boolean toggle, see [TransientSwitch](../TransientSwitch.md)
* `"option"` - value input, see [TransientOption](../TransientOption.md)
* `"cyclic"` - cycle through choices, see [TransientCyclicSwitch](../TransientCyclicSwitch.md)
* `"action"` - trigger an action, see [TransientAction](../TransientAction.md)

## Combining TransientMenu and SelectorActions for viewing logs for Docker containers with an ability to move between KeyAssignments

{% raw %}
```lua
local wezterm = require 'wezterm'
local act = wezterm.action

local function description(text)
  return wezterm.format {
    { Attribute = { Intensity = 'Bold' } },
    { Foreground = { AnsiColor = 'Teal' } },
    { Text = text },
  }
end

local function header(text)
  return wezterm.format {
    { Attribute = { Intensity = 'Bold' } },
    { Foreground = { AnsiColor = 'Navy' } },
    { Text = text },
  }
end

local function entry_label(text)
  return wezterm.format {
    { Foreground = { AnsiColor = 'Olive' } },
    { Text = text },
  }
end

local function fuzzy_description(text)
  return wezterm.format {
    { Attribute = { Intensity = 'Bold' } },
    { Foreground = { AnsiColor = 'Teal' } },
    { Text = text },
    'ResetAttributes',
    { Text = ': ' },
  }
end

local docker_actions_transient
local containers_selector_actions
local containers_logs_transient

containers_logs_transient = function(state)
  return wezterm.action_callback(function(window, pane)
    local selected_containers = {}
    for _, container in ipairs(state.choices) do
      table.insert(selected_containers, container.label)
    end

    window:perform_action(
      act.TransientMenu {
        description = description 'Docker container logs',
        context = {
          header = header 'Context',
          entries = {
            {
              label = entry_label 'Entity',
              id = 'Containers',
            },
            {
              label = entry_label 'Operation',
              id = 'Logs',
            },
            {
              label = entry_label 'Selected containers',
              id = table.concat(selected_containers, ', '),
            },
          },
        },
        sections = {
          {
            header = header 'Flags',
            entries = {
              {
                type = 'switch',
                key = '-f',
                default = true,
                description = 'Follow',
                flag = '--follow',
              },
              {
                type = 'option',
                key = '-t',
                default = '0',
                description = 'Tail',
                flag = '--tail=',
                allow_nil = false,
              },
            },
          },
          {
            header = header 'Actions',
            entries = {
              {
                type = 'action',
                key = 'l',
                description = 'Logs',
                action = wezterm.action_callback(
                  function(inner_window, inner_pane, result)
                    local cmd = { 'docker', 'logs' }
                    for _, entry in ipairs(result.entries) do
                      if entry.value == true then
                        table.insert(cmd, entry.flag)
                      elseif entry.value then
                        table.insert(cmd, entry.flag .. entry.value)
                      end
                    end

                    local cmd_len = #cmd
                    for _, container in ipairs(state.choices) do
                      cmd[cmd_len + 1] = container.id
                      inner_window:perform_action(
                        act.SpawnCommandInNewTab { args = cmd },
                        inner_pane
                      )
                    end
                  end
                ),
              },
            },
          },
        },
        cancel = wezterm.action_callback(function(inner_window, inner_pane)
          state.choices = nil
          inner_window:perform_action(
            containers_selector_actions(state),
            inner_pane
          )
        end),
      },
      pane
    )
  end)
end

containers_selector_actions = function(state)
  return wezterm.action_callback(function(window, pane)
    local success, stdout, stderr = wezterm.run_child_process {
      'docker',
      'container',
      'ls',
      '--format',
      '{{.ID}}:{{.Names}}',
    }
    if success then
      local containers = {}
      for _, line in ipairs(wezterm.split_by_newlines(stdout)) do
        local id, name = line:match '(.-):(.+)'
        if id and name then
          table.insert(containers, { label = name, id = id })
        end
      end

      window:perform_action(
        act.SelectorActions {
          description = description 'Select containers',
          context = {
            header = header 'Context',
            entries = {
              {
                label = entry_label 'Entity',
                id = 'Containers',
              },
            },
          },
          choices = containers,
          section = {
            header = header 'Actions',
            actions = {
              {
                key = 'l',
                description = 'Logs',
                action = wezterm.action_callback(
                  function(inner_window, inner_pane, result)
                    state.choices = result.choices

                    inner_window:perform_action(
                      containers_logs_transient(state),
                      inner_pane
                    )
                  end
                ),
              },
            },
          },
          fuzzy_description = fuzzy_description 'Select containers',
          multiple = true,
          cancel = wezterm.action_callback(function(inner_window, inner_pane)
            inner_window:perform_action(
              docker_actions_transient(state),
              inner_pane
            )
          end),
        },
        pane
      )
    end
  end)
end

docker_actions_transient = function(state)
  return wezterm.action_callback(function(window, pane)
    window:perform_action(
      act.TransientMenu {
        description = description 'Docker action',
        sections = {
          {
            header = header 'Actions',
            entries = {
              {
                type = 'action',
                key = 'c',
                description = 'Containers',
                action = wezterm.action_callback(
                  function(inner_window, inner_pane, result)
                    inner_window:perform_action(
                      containers_selector_actions(state),
                      inner_pane
                    )
                  end
                ),
              },
            },
          },
        },
      },
      pane
    )
  end)
end

local config = wezterm.config_builder()

config.keys = {
  {
    key = 'k',
    mods = 'CTRL',
    action = wezterm.action_callback(function(window, pane)
      local state = {}
      window:perform_action(docker_actions_transient(state), pane)
    end),
  },
}

return config
```
{% endraw %}
