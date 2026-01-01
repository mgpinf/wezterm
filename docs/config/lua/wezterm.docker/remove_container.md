# `wezterm.docker.remove_container(container_id [, options])`

{{since('nightly')}}

Removes a container.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `container_id` | string | Container ID or name |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `force` | bool | `false` | Force removal of running container |
| `volumes` | bool | `false` | Remove associated volumes |

## Return Value

Returns `true` on success, or throws an error on failure.

## Example

```lua
local wezterm = require 'wezterm'

wezterm.docker.remove_container 'my-container'
wezterm.log_info 'Container removed'
```

## Example: Force remove with volumes

```lua
local wezterm = require 'wezterm'

wezterm.docker.remove_container('my-container', {
  force = true,
  volumes = true,
})
```
