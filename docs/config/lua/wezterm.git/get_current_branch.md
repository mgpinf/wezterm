# `wezterm.git.get_current_branch(path)`

{{since('nightly')}}

Returns the current branch name, or `nil` if not in a git repository.

If HEAD is detached (not on a branch), returns `HEAD@<short-hash>` format.

```lua
local wezterm = require 'wezterm'

local branch = wezterm.git.get_current_branch '/path/to/repo'
if branch then
  wezterm.log_info('Current branch: ' .. branch)
  -- e.g., "main" or "HEAD@abc1234"
end
```

## Example: Show branch in tab title

```lua
local wezterm = require 'wezterm'

wezterm.on('format-tab-title', function(tab)
  local pane = tab.active_pane
  local cwd = pane.current_working_dir
  if cwd then
    local branch = wezterm.git.get_current_branch(cwd.file_path)
    if branch then
      return ' ' .. branch .. ' '
    end
  end
  return tab.active_pane.title
end)
```
