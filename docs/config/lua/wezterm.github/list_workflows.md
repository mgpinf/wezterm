# `wezterm.github.list_workflows(options)`

{{since('nightly')}}

Returns a list of GitHub Actions workflows for a repository.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Required settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `owner` | string | required | Repository owner |
| `repo` | string | required | Repository name |
| `token` | string | nil | GitHub personal access token |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of tables with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Workflow ID |
| `name` | string | Workflow name |
| `path` | string | Workflow file path |
| `state` | string | Workflow state (active, disabled, etc.) |
| `html_url` | string | Workflow URL |

## Example

```lua
local wezterm = require 'wezterm'

local workflows = wezterm.github.list_workflows {
  owner = 'wez',
  repo = 'wezterm',
}

if workflows.error then
  wezterm.log_error('Failed: ' .. workflows.error)
else
  for _, wf in ipairs(workflows) do
    wezterm.log_info(wf.name .. ' (' .. wf.state .. ')')
  end
end
```
