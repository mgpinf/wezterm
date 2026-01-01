# `wezterm.docker.list_images([options])`

{{since('nightly')}}

Returns a list of Docker images, or `nil` if Docker is not available.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `all` | bool | `false` | Include intermediate images |
| `digests` | bool | `false` | Include digests |

## Return Value

Returns an array of tables, each with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Full image ID |
| `short_id` | string | Short image ID (12 characters) |
| `tags` | table | Array of image tags |
| `created` | number | Creation timestamp |
| `size` | number | Image size in bytes |
| `virtual_size` | number | Virtual size in bytes |

## Example

```lua
local wezterm = require 'wezterm'

local images = wezterm.docker.list_images()
if images then
  for _, image in ipairs(images) do
    local tag = image.tags[1] or '<none>'
    wezterm.log_info(tag .. ' - ' .. image.short_id)
  end
end
```

## Example: Status bar with image count

```lua
local wezterm = require 'wezterm'

wezterm.on('update-status', function(window, pane)
  if not wezterm.docker.is_available() then
    return
  end

  local images = wezterm.docker.list_images()
  local containers = wezterm.docker.list_containers { all = false }

  if images and containers then
    window:set_right_status(
      string.format('🐳 %d containers, %d images', #containers, #images)
    )
  end
end)
```
