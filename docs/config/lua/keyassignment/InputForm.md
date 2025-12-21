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
          if host == '' then host = 'localhost' end
          
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

The `InputForm` struct takes the following fields:

*   `title` - The title displayed at the top of the form.
*   `submit_label` - (Optional) The text to display for the submit hint (default: "Submit").
*   `fields` - A list of `FormField` objects defining the inputs.
*   `action` - A `KeyAssignment` to execute when the form is submitted. This is typically a `wezterm.action_callback`.

Each `FormField` in the `fields` list has:

*   `label` - The text label displayed next to the input.
*   `id` - A unique identifier for the field, returned in the result.
*   `placeholder` - (Optional) Text to display when the field is empty.
*   `is_password` - (Optional) If true, input characters are masked with `*`.
*   `initial_value` - (Optional) The starting value for the field.
*   `required` - (Optional) If true, the form cannot be submitted unless this field has a value.

When the user submits the form, the `action` is triggered. If it is an `action_callback`, the callback function receives `(window, pane, form_result)`.

`form_result` is a table containing:
*   `fields`: A list of tables, where each table has:
    *   `id`: The `id` of the field from the configuration.
    *   `value`: The string value entered by the user.

## Key Assignments

The following keys are handled while the input form is active:

| Key | Action |
| --- | --- |
| `Esc` | Cancel the form |
| `Enter` | Submit the form (validates required fields) |
| `Tab` | Focus next field (wraps around) |
| `Shift-Tab` | Focus previous field (wraps around) |
| `UpArrow` / `Ctrl-P` | Focus previous field |
| `DownArrow` / `Ctrl-N` | Focus next field |
| `Backspace` | Delete character |