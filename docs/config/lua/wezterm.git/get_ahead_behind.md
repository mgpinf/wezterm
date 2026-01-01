# `wezterm.git.get_ahead_behind(path)`

{{since('nightly')}}

Returns how many commits the local branch is ahead of and behind its upstream
tracking branch, or `nil` if not in a repository, no upstream is configured,
or HEAD is detached.

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `ahead` | number | Commits ahead of upstream |
| `behind` | number | Commits behind upstream |

## Example

```lua
local wezterm = require 'wezterm'

local info = wezterm.git.get_ahead_behind '/path/to/repo'
if info then
  if info.ahead > 0 then
    wezterm.log_info('You have ' .. info.ahead .. ' commits to push')
  end
  if info.behind > 0 then
    wezterm.log_info('You are ' .. info.behind .. ' commits behind upstream')
  end
end
```

## Example: Show in status bar

```lua
local wezterm = require 'wezterm'

wezterm.on('update-status', function(window, pane)
  local cwd = pane:get_current_working_dir()
  if not cwd then return end

  local branch = wezterm.git.get_current_branch(cwd.file_path)
  if not branch then return end

  local parts = { branch }
  local ab = wezterm.git.get_ahead_behind(cwd.file_path)
  if ab then
    if ab.ahead > 0 then
      table.insert(parts, '↑' .. ab.ahead)
    end
    if ab.behind > 0 then
      table.insert(parts, '↓' .. ab.behind)
    end
  end

  window:set_right_status(table.concat(parts, ' '))
end)
```
