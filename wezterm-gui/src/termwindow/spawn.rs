use crate::overlay::resolve_overlay_dimensions;
use crate::scripting::guiwin::GuiWin;
use crate::spawn::SpawnWhere;
use config::keyassignment::{FloatingPaneSpawn, KeyAssignment, SpawnCommand, SpawnTabDomain};
use config::TermConfig;
use mux::pane::PaneId;
use mux::{Mux, MuxNotification};
use mux_lua::MuxPane;
use std::rc::Rc;
use std::sync::Arc;

fn trigger_floating_pane_close_event(name: String, window: GuiWin, pane: MuxPane) {
    promise::spawn::spawn(async move {
        config::with_lua_config_on_main_thread(move |lua| {
            floating_pane_close_event(lua, name, window, pane)
        })
        .await
    })
    .detach();
}

async fn floating_pane_close_event(
    lua: Option<Rc<mlua::Lua>>,
    name: String,
    window: GuiWin,
    pane: MuxPane,
) -> anyhow::Result<()> {
    if let Some(lua) = lua {
        let args = lua.pack_multi((window, pane))?;

        if let Err(err) = config::lua::emit_event(lua.as_ref().clone(), (name.clone(), args)).await
        {
            log::error!("while processing {} event: {:#}", name, err);
        }
    }

    Ok(())
}

fn register_floating_pane_close_action(
    target_pane_id: PaneId,
    action: KeyAssignment,
    window: GuiWin,
    pane: MuxPane,
) {
    let name = match action {
        KeyAssignment::EmitEvent(id) => id,
        _ => {
            log::error!(
                "SpawnCommandInFloatingPane requires action to be defined by wezterm.action_callback"
            );
            return;
        }
    };

    if Mux::get().get_pane(target_pane_id).is_none() {
        trigger_floating_pane_close_event(name, window, pane);
        return;
    }

    let mux_window_id = window.mux_window_id;
    Mux::get().subscribe(move |notification| match notification {
        MuxNotification::PaneRemoved(pane_id) if pane_id == target_pane_id => {
            trigger_floating_pane_close_event(name.clone(), window.clone(), pane);
            false
        }
        MuxNotification::WindowRemoved(window_id) if window_id == mux_window_id => false,
        _ => true,
    });
}

impl super::TermWindow {
    pub fn spawn_command(&self, spawn: &SpawnCommand, spawn_where: SpawnWhere) {
        let size = if spawn_where == SpawnWhere::NewWindow {
            self.config.initial_size(
                self.dimensions.dpi as u32,
                crate::cell_pixel_dims(&self.config, self.dimensions.dpi as f64).ok(),
            )
        } else {
            self.terminal_size
        };
        let term_config = Arc::new(TermConfig::with_config(self.config.clone()));

        crate::spawn::spawn_command_impl(
            spawn,
            spawn_where,
            size,
            Some(self.mux_window_id),
            term_config,
        )
    }

    pub fn spawn_floating_pane(&self, spawn: &FloatingPaneSpawn, source_pane_id: PaneId) {
        let size = resolve_overlay_dimensions(self.terminal_size, spawn.dimensions).size;
        let term_config = Arc::new(TermConfig::with_config(self.config.clone()));
        let src_window_id = self.mux_window_id;
        let window = GuiWin::new(self);
        let source_pane = MuxPane(source_pane_id);
        let command = spawn.command.clone();
        let replace_current = spawn.replace_current;
        let action = spawn.action.clone();
        let dimensions = spawn.dimensions;
        let border = spawn.border;
        let border_color = spawn.border_color;

        promise::spawn::spawn(async move {
            match crate::spawn::spawn_command_internal(
                command,
                SpawnWhere::FloatingPane {
                    replace_current,
                    dimensions,
                    border,
                    border_color,
                },
                size,
                Some(src_window_id),
                term_config,
            )
            .await
            {
                Ok(Some(target_pane_id)) => {
                    if let Some(action) = action {
                        register_floating_pane_close_action(
                            target_pane_id,
                            *action,
                            window,
                            source_pane,
                        );
                    }
                }
                Ok(None) => {}
                Err(err) => log::error!("Failed to spawn: {:#}", err),
            }
        })
        .detach();
    }

    pub fn spawn_tab(&mut self, domain: &SpawnTabDomain) {
        self.spawn_command(
            &SpawnCommand {
                domain: domain.clone(),
                ..Default::default()
            },
            SpawnWhere::NewTab,
        );
    }
}
