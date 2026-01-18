use config::lua::get_or_create_sub_module;
use config::lua::mlua::{self, Lua};

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let password_mod = get_or_create_sub_module(lua, "password")?;

    password_mod.set("get", lua.create_function(get_password)?)?;
    password_mod.set("set", lua.create_function(set_password)?)?;
    password_mod.set("delete", lua.create_function(delete_password)?)?;

    Ok(())
}

fn get_password(_lua: &Lua, (service, account): (String, String)) -> mlua::Result<Option<String>> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| mlua::Error::external(format!("Failed to create keyring entry: {}", e)))?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(mlua::Error::external(format!(
            "Failed to get password: {}",
            e
        ))),
    }
}

fn set_password(
    _lua: &Lua,
    (service, account, password): (String, String, String),
) -> mlua::Result<bool> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| mlua::Error::external(format!("Failed to create keyring entry: {}", e)))?;

    entry
        .set_password(&password)
        .map_err(|e| mlua::Error::external(format!("Failed to set password: {}", e)))?;

    Ok(true)
}

fn delete_password(_lua: &Lua, (service, account): (String, String)) -> mlua::Result<bool> {
    let entry = keyring::Entry::new(&service, &account)
        .map_err(|e| mlua::Error::external(format!("Failed to create keyring entry: {}", e)))?;

    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(mlua::Error::external(format!(
            "Failed to delete password: {}",
            e
        ))),
    }
}
