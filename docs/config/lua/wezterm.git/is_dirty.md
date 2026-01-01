# `wezterm.git.is_dirty(path)`

{{since('nightly')}}

Returns `true` if the repository has uncommitted changes (modified, staged, or
untracked files), `false` if clean, or `nil` if not in a git repository.

```lua
local wezterm = require 'wezterm'

local dirty = wezterm.git.is_dirty '/path/to/repo'
if dirty == nil then
  wezterm.log_info 'Not a git repository'
elseif dirty then
  wezterm.log_info 'Repository has uncommitted changes'
else
  wezterm.log_info 'Repository is clean'
end
```

## Example: Show dirty indicator in status bar

```lua
local wezterm = require 'wezterm'

wezterm.on('update-status', function(window, pane)
  local cwd = pane:get_current_working_dir()
  if cwd then
    local branch = wezterm.git.get_current_branch(cwd.file_path)
    local dirty = wezterm.git.is_dirty(cwd.file_path)
    if branch then
      local indicator = dirty and '*' or ''
      window:set_right_status(branch .. indicator)
    end
  end
end)
```
