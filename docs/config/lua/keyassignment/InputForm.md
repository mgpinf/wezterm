---
tags:
  - overlay
---

# `InputForm`

{{since('nightly')}}

The `InputForm` key assignment allows you to present a multi-field form to the user. This is useful for gathering multiple pieces of information at once, such as credentials for a database connection or parameters for a script.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  {
    key = 'f',
    mods = 'LEADER',
    action = act.InputForm {
      title = 'Connect to Database',
      submit_label = 'Connect',
      fields = {
        {
          label = 'Host',
          id = 'host',
          placeholder = 'localhost',
        },
        {
          label = 'Port',
          id = 'port',
          placeholder = '5432',
        },
        {
          label = 'User',
          id = 'user',
        },
        {
          label = 'Password',
          id = 'password',
          is_password = true,
        },
      },
      action = wezterm.action_callback(function(window, pane, form_result)
        if form_result then
          -- form_result is a table with a 'fields' list
          -- each field has 'id' and 'value'
          local host = form_result.fields[1].value
          if host == '' then
            host = 'localhost'
          end

          window:toast_notification(
            'WezTerm',
            'Connecting to ' .. host .. ' as ' .. form_result.fields[3].value,
            nil,
            4000
          )
        end
      end),
    },
  },
}
```

## InputForm Fields

The `InputForm` struct takes the following fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `title` | string | yes | The title displayed at the top of the form |
| `fields` | list | yes | A list of `FormField` objects defining the inputs |
| `action` | KeyAssignment | yes | Action to execute when the form is submitted (typically `wezterm.action_callback`) |
| `submit_label` | string | no | The text to display for the submit hint (default: "Submit") |

## FormField Properties

Each `FormField` in the `fields` list has the following properties:

| Property | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `label` | string | yes | - | The text label displayed next to the input |
| `id` | string | yes | - | A unique identifier for the field, returned in the result |
| `placeholder` | string | no | `nil` | Text to display when the field is empty |
| `is_password` | bool | no | `false` | If true, input characters are masked with `*` |
| `initial_value` | string | no | `nil` | The starting value for the field |
| `required` | bool | no | `false` | If true, the form cannot be submitted unless this field has a value |
| `choices` | list | no | `[]` | If non-empty, this field becomes a dropdown selector with fuzzy search |

## Selector Fields

When a field has a non-empty `choices` list, it becomes a dropdown selector instead of a text input. Users can:
- Press `Enter` to open the dropdown
- Type to fuzzy-filter the choices
- Use arrow keys to navigate
- Press `Enter` again to select the highlighted choice

Each entry in the `choices` list is a `FormFieldChoice` with:

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `label` | string | yes | Display text shown in the dropdown |
| `id` | string | no | Value to submit (defaults to `label` if not specified) |

### Selector Example

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  {
    key = 's',
    mods = 'LEADER',
    action = act.InputForm {
      title = 'SSH Connection',
      fields = {
        {
          label = 'Server',
          id = 'server',
          choices = {
            { label = 'Production', id = 'prod.example.com' },
            { label = 'Staging', id = 'staging.example.com' },
            { label = 'Development', id = 'dev.example.com' },
          },
        },
        {
          label = 'User',
          id = 'user',
          initial_value = 'admin',
        },
        {
          label = 'Environment',
          id = 'env',
          choices = {
            { label = 'bash' },
            { label = 'zsh' },
            { label = 'fish' },
          },
        },
      },
      action = wezterm.action_callback(function(window, pane, form_result)
        if form_result then
          local server = form_result.fields[1].value
          local user = form_result.fields[2].value
          local shell = form_result.fields[3].value

          pane:send_text('ssh ' .. user .. '@' .. server .. '\n')
        end
      end),
    },
  },
}
```

## Form Result

When the user submits the form, the `action` is triggered. If it is an `action_callback`, the callback function receives `(window, pane, form_result)`.

`form_result` is `nil` if the user cancelled the form, otherwise it is a table containing:

| Field | Type | Description |
|-------|------|-------------|
| `fields` | list | A list of tables with the submitted values |

Each entry in `fields` contains:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | The `id` of the field from the configuration |
| `value` | string | The string value entered by the user (or selected choice's `id`) |

## Key Bindings

The following keys are handled while the input form is active:

### General Navigation

| Key | Action |
|-----|--------|
| `Esc` | Cancel the form |
| `Ctrl-Enter` | Submit the form (validates required fields) |
| `Tab` | Focus next field (wraps around) |
| `Shift-Tab` | Focus previous field (wraps around) |
| `UpArrow` / `Ctrl-P` | Focus previous field |
| `DownArrow` / `Ctrl-N` | Focus next field |

### Text Field Editing

| Key | Action |
|-----|--------|
| `LeftArrow` / `Ctrl-B` | Move cursor left |
| `RightArrow` / `Ctrl-F` | Move cursor right |
| `Home` / `Ctrl-A` | Move cursor to start of field |
| `End` / `Ctrl-E` | Move cursor to end of field |
| `Backspace` | Delete character before cursor |
| `Delete` / `Ctrl-D` | Delete character at cursor |
| `Ctrl-U` | Clear field |
| `Ctrl-K` | Kill to end of field |
| `Ctrl-W` | Delete word before cursor |
| `Paste` | Insert clipboard text at cursor |

### Selector Field Keys

| Key | Action |
|-----|--------|
| `Enter` | Open dropdown (when closed) or select highlighted choice (when open) |
| `UpArrow` / `Ctrl-P` | Move to previous choice (when dropdown is open) |
| `DownArrow` / `Ctrl-N` | Move to next choice (when dropdown is open) |
| `Backspace` | Delete filter character, or close dropdown if filter is empty |
| `Ctrl-U` | Clear the filter |
| Type any character | Filter choices with fuzzy search |
