use anyhow::anyhow;
use config::lua::get_or_create_sub_module;
use config::lua::mlua::{self, Lua, Value};
use http_req::request::{Method, Request};
use http_req::uri::Uri;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::collections::HashMap;
use std::time::Duration;
use wezterm_dynamic::FromDynamic;

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let http_mod = get_or_create_sub_module(lua, "http")?;

    http_mod.set("get", lua.create_async_function(http_get)?)?;
    http_mod.set("post", lua.create_async_function(http_post)?)?;
    http_mod.set("request", lua.create_async_function(http_request)?)?;

    Ok(())
}

#[derive(Debug, Default, FromDynamic)]
struct HttpRequestOptions {
    #[dynamic(default)]
    headers: HashMap<String, String>,
    #[dynamic(default)]
    params: HashMap<String, String>,
    #[dynamic(default)]
    timeout: Option<u64>,
    #[dynamic(default)]
    body: Option<String>,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl mlua::UserData for HttpResponse {
    fn add_fields<'lua, F: mlua::UserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("status", |_, this| Ok(this.status));
        fields.add_field_method_get("body", |_, this| Ok(this.body.clone()));
        fields.add_field_method_get("headers", |lua, this| {
            let table = lua.create_table()?;
            for (k, v) in &this.headers {
                table.set(k.clone(), v.clone())?;
            }
            Ok(table)
        });
    }

    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, _: ()| {
            Ok(format!(
                "HttpResponse {{ status = {}, body_len = {} }}",
                this.status,
                this.body.len()
            ))
        });
    }
}

async fn http_get<'lua>(
    _: &'lua Lua,
    (url, options): (String, Option<Value<'_>>),
) -> mlua::Result<HttpResponse> {
    let opts: HttpRequestOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => HttpRequestOptions::default(),
    };

    do_request(Method::GET, url, opts)
        .await
        .map_err(mlua::Error::external)
}

async fn http_post<'lua>(
    _: &'lua Lua,
    (url, body, options): (String, Option<String>, Option<Value<'_>>),
) -> mlua::Result<HttpResponse> {
    let mut opts: HttpRequestOptions = match options {
        Some(value) => luahelper::from_lua(value)?,
        None => HttpRequestOptions::default(),
    };

    if opts.body.is_none() {
        opts.body = body;
    }

    do_request(Method::POST, url, opts)
        .await
        .map_err(mlua::Error::external)
}

#[derive(Debug, FromDynamic)]
struct FullHttpRequestOptions {
    url: String,
    #[dynamic(default = "default_method")]
    method: String,
    #[dynamic(default)]
    headers: HashMap<String, String>,
    #[dynamic(default)]
    params: HashMap<String, String>,
    #[dynamic(default)]
    timeout: Option<u64>,
    #[dynamic(default)]
    body: Option<String>,
}

fn default_method() -> String {
    "GET".to_string()
}

async fn http_request<'lua>(_: &'lua Lua, options: Value<'_>) -> mlua::Result<HttpResponse> {
    let opts: FullHttpRequestOptions = luahelper::from_lua(options)?;

    let method = match opts.method.to_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
        "PATCH" => Method::PATCH,
        other => {
            return Err(mlua::Error::external(anyhow!(
                "Unsupported HTTP method: {}",
                other
            )))
        }
    };

    let request_opts = HttpRequestOptions {
        headers: opts.headers,
        params: opts.params,
        timeout: opts.timeout,
        body: opts.body,
    };

    do_request(method, opts.url, request_opts)
        .await
        .map_err(mlua::Error::external)
}

async fn do_request(
    method: Method,
    url: String,
    opts: HttpRequestOptions,
) -> anyhow::Result<HttpResponse> {
    smol::unblock(move || {
        // Build URL with query parameters
        let full_url = if opts.params.is_empty() {
            url.clone()
        } else {
            let query_string: String = opts
                .params
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        utf8_percent_encode(k, NON_ALPHANUMERIC),
                        utf8_percent_encode(v, NON_ALPHANUMERIC)
                    )
                })
                .collect::<Vec<_>>()
                .join("&");

            if url.contains('?') {
                format!("{}&{}", url, query_string)
            } else {
                format!("{}?{}", url, query_string)
            }
        };

        let uri = Uri::try_from(full_url.as_str())
            .map_err(|e| anyhow!("Invalid URL '{}': {}", full_url, e))?;

        let mut body_data = Vec::new();

        let mut request = Request::new(&uri);
        request.method(method);

        // Set default User-Agent if not provided
        if !opts.headers.contains_key("User-Agent") && !opts.headers.contains_key("user-agent") {
            request.header("User-Agent", "wezterm");
        }

        // Set timeout (default 30 seconds)
        let timeout = Duration::from_secs(opts.timeout.unwrap_or(30));
        request.timeout(timeout);
        request.connect_timeout(Some(timeout));

        // Set headers
        for (key, value) in &opts.headers {
            request.header(key, value);
        }

        // Set body if present
        let body_bytes = opts.body.as_ref().map(|b| b.as_bytes());
        if let Some(bytes) = body_bytes {
            request.body(bytes);
        }

        let response = request
            .send(&mut body_data)
            .map_err(|e| anyhow!("HTTP request to '{}' failed: {:?}", full_url, e))?;

        let status = response.status_code().into();

        let mut headers = HashMap::new();
        for (key, value) in response.headers().iter() {
            headers.insert(key.to_string(), value.to_string());
        }

        let body = String::from_utf8_lossy(&body_data).to_string();

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    })
    .await
}
