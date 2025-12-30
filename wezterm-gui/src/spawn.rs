use anyhow::{anyhow, bail, Context};
use config::keyassignment::{SpawnCommand, SpawnTabDomain};
use config::TermConfig;
use mux::activity::Activity;
use mux::domain::SplitSource;
use mux::tab::SplitRequest;
use mux::window::WindowId as MuxWindowId;
use mux::Mux;
use portable_pty::CommandBuilder;
use std::sync::Arc;
use wezterm_term::TerminalSize;

#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum SpawnWhere {
    NewWindow,
    NewTab,
    SplitPane(SplitRequest),
    FloatingPane { replace_current: bool },
}

pub fn spawn_command_impl(
    spawn: &SpawnCommand,
    spawn_where: SpawnWhere,
    size: TerminalSize,
    src_window_id: Option<MuxWindowId>,
    term_config: Arc<TermConfig>,
) {
    let spawn = spawn.clone();

    promise::spawn::spawn(async move {
        if let Err(err) =
            spawn_command_internal(spawn, spawn_where, size, src_window_id, term_config).await
        {
            log::error!("Failed to spawn: {:#}", err);
        }
    })
    .detach();
}

pub async fn spawn_command_internal(
    spawn: SpawnCommand,
    spawn_where: SpawnWhere,
    size: TerminalSize,
    src_window_id: Option<MuxWindowId>,
    term_config: Arc<TermConfig>,
) -> anyhow::Result<()> {
    let mux = Mux::get();
    let activity = Activity::new();

    let current_pane_id = match src_window_id {
        Some(window_id) => {
            if let Some(tab) = mux.get_active_tab_for_window(window_id) {
                tab.get_active_pane().map(|p| p.pane_id())
            } else {
                None
            }
        }
        None => None,
    };

    let cwd = if let Some(cwd) = spawn.cwd.as_ref() {
        Some(cwd.to_str().map(|s| s.to_owned()).ok_or_else(|| {
            anyhow!(
                "Domain::spawn requires that the cwd be unicode in {:?}",
                cwd
            )
        })?)
    } else {
        None
    };

    let cmd_builder = match (
        spawn.args.as_ref(),
        spawn.cwd.as_ref(),
        spawn.set_environment_variables.is_empty(),
    ) {
        (None, None, true) => None,
        _ => {
            let mut builder = spawn
                .args
                .as_ref()
                .map(|args| CommandBuilder::from_argv(args.iter().map(Into::into).collect()))
                .unwrap_or_else(CommandBuilder::new_default_prog);
            for (k, v) in spawn.set_environment_variables.iter() {
                builder.env(k, v);
            }
            if let Some(cwd) = &spawn.cwd {
                builder.cwd(cwd);
            }
            Some(builder)
        }
    };

    let workspace = mux.active_workspace().clone();

    match spawn_where {
        SpawnWhere::SplitPane(direction) => {
            let src_window_id = match src_window_id {
                Some(id) => id,
                None => anyhow::bail!("no src window when splitting a pane?"),
            };
            if let Some(tab) = mux.get_active_tab_for_window(src_window_id) {
                let pane = tab
                    .get_active_pane()
                    .ok_or_else(|| anyhow!("tab to have a pane"))?;

                log::trace!("doing split_pane");
                let (pane, _size) = mux
                    .split_pane(
                        // tab.tab_id(),
                        pane.pane_id(),
                        direction,
                        SplitSource::Spawn {
                            command: cmd_builder,
                            command_dir: cwd,
                        },
                        spawn.domain,
                    )
                    .await
                    .context("split_pane")?;
                pane.set_config(term_config);
            } else {
                bail!("there is no active tab while splitting pane!?");
            }
        }
        SpawnWhere::FloatingPane { replace_current } => {
            // Floating panes are a specific layout preference handled by the Tab itself,
            // rather than a global window management concern like SplitPane or NewTab.
            // By implementing this here in the Controller (GUI), we orchestrate the
            // creation of the pane via the Domain and then hand it off to the Tab
            // to store in its special `floating` slot. This keeps the Mux API more
            // generic while allowing the GUI to handle the specific "floating" layout logic.
            let src_window_id = match src_window_id {
                Some(id) => id,
                None => anyhow::bail!("no src window when spawning floating pane?"),
            };
            if let Some(tab) = mux.get_active_tab_for_window(src_window_id) {
                // A tab can only have one floating pane at a time.
                if tab.has_floating_pane() {
                    if replace_current {
                        // Close the existing floating pane before spawning a new one
                        if let Some(old_pane) = tab.take_floating_pane() {
                            log::debug!(
                                "replacing floating pane {} in tab {}",
                                old_pane.pane_id(),
                                tab.tab_id()
                            );
                            mux.remove_pane(old_pane.pane_id());
                        }
                    } else {
                        log::debug!(
                            "tab {} already has a floating pane, not spawning another",
                            tab.tab_id()
                        );
                        return Ok(());
                    }
                }

                let domain = match &spawn.domain {
                    SpawnTabDomain::DefaultDomain => Some(mux.default_domain()),
                    SpawnTabDomain::CurrentPaneDomain => {
                        let dom_id = tab.get_active_pane().map(|p| p.domain_id()).unwrap_or(0);
                        Some(
                            mux.get_domain(dom_id)
                                .unwrap_or_else(|| mux.default_domain()),
                        )
                    }
                    SpawnTabDomain::DomainName(name) => mux.get_domain_by_name(name),
                    SpawnTabDomain::DomainId(id) => mux.get_domain(*id),
                }
                .ok_or_else(|| anyhow!("domain not found"))?;

                let pane = domain.spawn_pane(size, cmd_builder, cwd).await?;
                pane.set_config(term_config);
                tab.assign_floating_pane(&pane);
            }
        }
        _ => {
            let (_tab, pane, window_id) = mux
                .spawn_tab_or_window(
                    match spawn_where {
                        SpawnWhere::NewWindow => None,
                        _ => src_window_id,
                    },
                    spawn.domain,
                    cmd_builder,
                    cwd,
                    size,
                    current_pane_id,
                    workspace,
                    spawn.position,
                )
                .await
                .context("spawn_tab_or_window")?;

            // If it was created in this window, it copies our handlers.
            // Otherwise, we'll pick them up when we later respond to
            // the new window being created.
            if Some(window_id) == src_window_id {
                pane.set_config(term_config);
            }
        }
    };

    drop(activity);

    Ok(())
}
