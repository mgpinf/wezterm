---
title: wezterm.http.request
tags:
 - utility
 - http
---

# `wezterm.http.request(options)`

Performs a custom HTTP request with full control over the method and options.
Returns an `HttpResponse` object.

## Parameters

The `options` parameter is a table that must contain:

* `url` - the URL to request (required)

And can optionally contain:

* `method` - HTTP method: `"GET"`, `"POST"`, `"PUT"`, `"DELETE"`, `"HEAD"`, `"OPTIONS"`, `"PATCH"` (default: `"GET"`)
* `headers` - a table of HTTP headers
* `params` - a table of query parameters (will be URL-encoded and appended to the URL)
* `body` - request body as a string
* `timeout` - request timeout in seconds (default: 30)

## HttpResponse

The returned `HttpResponse` object has the following fields:

* `status` - the HTTP status code
* `body` - the response body as a string
* `headers` - a table of response headers

## Examples

### PUT request

```lua
local wezterm = require 'wezterm'

local response = wezterm.http.request {
  url = 'https://api.example.com/items/123',
  method = 'PUT',
  headers = {
    ['Content-Type'] = 'application/json',
    ['Authorization'] = 'Bearer my-token',
  },
  body = wezterm.json_encode {
    name = 'updated-name',
  },
}
```

### DELETE request

```lua
local wezterm = require 'wezterm'

local response = wezterm.http.request {
  url = 'https://api.example.com/items/123',
  method = 'DELETE',
  headers = {
    ['Authorization'] = 'Bearer my-token',
  },
}

if response.status == 204 then
  wezterm.log_info 'Deleted successfully'
end
```

### HEAD request (check if resource exists)

```lua
local wezterm = require 'wezterm'

local response = wezterm.http.request {
  url = 'https://example.com/file.zip',
  method = 'HEAD',
}

if response.status == 200 then
  local size = response.headers['Content-Length']
  wezterm.log_info('File size: ' .. (size or 'unknown'))
end
```
