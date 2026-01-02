# `wezterm.github.list_workflow_runs(options)`

{{since('nightly')}}

Returns a list of GitHub Actions workflow runs.

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
| `workflow_id` | number | nil | Filter by workflow ID (lists all runs if not specified) |
| `per_page` | number | nil | Results per page (max 100) |
| `page` | number | nil | Page number |

## Return Value

Returns an array of tables with the following fields, or a table with an `error` field on failure:

| Field | Type | Description |
|-------|------|-------------|
| `id` | number | Run ID |
| `name` | string | Run name |
| `status` | string | Run status (queued, in_progress, completed) |
| `conclusion` | string | Run conclusion (success, failure, cancelled, etc.) |
| `html_url` | string | Run URL |
| `run_number` | number | Run number |
| `head_branch` | string | Branch that triggered the run |
| `head_sha` | string | Commit SHA that triggered the run |

## Example

```lua
local wezterm = require 'wezterm'

local runs = wezterm.github.list_workflow_runs {
  owner = 'wez',
  repo = 'wezterm',
  per_page = 5,
}

if runs.error then
  wezterm.log_error('Failed: ' .. runs.error)
else
  for _, run in ipairs(runs) do
    local status = run.conclusion or run.status
    wezterm.log_info(
      '#' .. run.run_number .. ' ' .. run.name .. ' - ' .. status
    )
  end
end
```
