# `wezterm.git.get_status(path [, options])`

{{since('nightly')}}

Returns detailed status counts for the repository, or `nil` if not in a git repository.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `include_untracked` | bool | `true` | Include untracked files in counts |
| `recurse_untracked_dirs` | bool | `false` | Recurse into untracked directories |

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `staged` | number | Files staged for commit |
| `modified` | number | Modified files (unstaged) |
| `deleted` | number | Deleted files (unstaged) |
| `untracked` | number | Untracked files |
| `conflicted` | number | Files with merge conflicts |
| `total` | number | Total number of status entries |

## Example

```lua
local wezterm = require 'wezterm'

local status = wezterm.git.get_status '/path/to/repo'
if status then
  wezterm.log_info('Staged: ' .. status.staged)
  wezterm.log_info('Modified: ' .. status.modified)
  wezterm.log_info('Untracked: ' .. status.untracked)
end
```

## Example: Detailed status bar

```lua
local wezterm = require 'wezterm'

wezterm.on('update-status', function(window, pane)
  local cwd = pane:get_current_working_dir()
  if not cwd then
    return
  end

  local branch = wezterm.git.get_current_branch(cwd.file_path)
  if not branch then
    return
  end

  local status = wezterm.git.get_status(cwd.file_path)
  local parts = { branch }

  if status then
    if status.staged > 0 then
      table.insert(parts, '+' .. status.staged)
    end
    if status.modified > 0 then
      table.insert(parts, '!' .. status.modified)
    end
    if status.untracked > 0 then
      table.insert(parts, '?' .. status.untracked)
    end
  end

  window:set_right_status(table.concat(parts, ' '))
end)
```
