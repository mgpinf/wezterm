---
title: wezterm.http.get
tags:
 - utility
 - http
---

# `wezterm.http.get(url [, options])`

Performs an HTTP GET request to the specified URL and returns an `HttpResponse` object.

The optional `options` parameter is a table that can contain:

* `headers` - a table of HTTP headers to include in the request
* `params` - a table of query parameters (will be URL-encoded and appended to the URL)
* `timeout` - request timeout in seconds (default: 30)

## HttpResponse

The returned `HttpResponse` object has the following fields:

* `status` - the HTTP status code (e.g., 200, 404)
* `body` - the response body as a string
* `headers` - a table of response headers

## Examples

### Simple GET request

```lua
local wezterm = require 'wezterm'

wezterm.on('my-event', function()
  local response = wezterm.http.get 'https://api.github.com/zen'
  wezterm.log_info('Status: ' .. response.status)
  wezterm.log_info('Body: ' .. response.body)
end)
```

### GET with query parameters

```lua
local wezterm = require 'wezterm'

-- This will request: https://api.example.com/search?q=hello%20world&page=1
local response = wezterm.http.get('https://api.example.com/search', {
  params = {
    q = 'hello world',
    page = '1',
  },
})
```

### GET with custom headers

```lua
local wezterm = require 'wezterm'

local response = wezterm.http.get('https://api.example.com/data', {
  headers = {
    ['Authorization'] = 'Bearer my-token',
    ['Accept'] = 'application/json',
  },
  timeout = 10,
})

if response.status == 200 then
  local data = wezterm.json_parse(response.body)
  -- use data...
end
```
