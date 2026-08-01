use crate::termwindow::TermWindow;
use config::keyassignment::{OverlayDimensions, SplitSize};
use mux::pane::{Pane, PaneId};
use mux::tab::{Tab, TabId};
use mux::termwiztermtab::{allocate, TermWizTerminal};
use std::pin::Pin;
use std::sync::Arc;
use wezterm_term::{TerminalConfiguration, TerminalSize};

pub mod command_runner;
pub mod common;
pub mod confirm;
pub mod confirm_close_pane;
pub mod copy;
pub mod debug;
pub mod display;
pub mod launcher;
pub mod prompt;
pub mod quickselect;
pub mod selector;
pub mod selector_actions;
pub mod transient;

pub use confirm_close_pane::{
    confirm_close_pane, confirm_close_tab, confirm_close_window, confirm_quit_program,
};
pub use copy::{ActivateMatchPosition, CopyModeParams, CopyOverlay};
pub use debug::show_debug_overlay;
pub use launcher::{launcher, LauncherArgs, LauncherFlags};
pub use quickselect::QuickSelectOverlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOverlayDimensions {
    pub size: TerminalSize,
    pub left: usize,
    pub top: usize,
}

fn resolve_overlay_axis(value: SplitSize, available: usize) -> usize {
    if available == 0 {
        return 0;
    }

    let requested = match value {
        SplitSize::Cells(cells) => cells,
        SplitSize::Percent(percent) => available.saturating_mul(percent as usize) / 100,
    };

    requested.clamp(1, available)
}

pub fn resolve_overlay_dimensions(
    available: TerminalSize,
    dimensions: OverlayDimensions,
) -> ResolvedOverlayDimensions {
    let cols = resolve_overlay_axis(dimensions.width, available.cols);
    let rows = resolve_overlay_axis(dimensions.height, available.rows);
    let pixel_width = if available.cols == 0 {
        0
    } else {
        available.pixel_width.saturating_mul(cols) / available.cols
    };
    let pixel_height = if available.rows == 0 {
        0
    } else {
        available.pixel_height.saturating_mul(rows) / available.rows
    };

    ResolvedOverlayDimensions {
        size: TerminalSize {
            rows,
            cols,
            pixel_width,
            pixel_height,
            dpi: available.dpi,
        },
        left: available.cols.saturating_sub(cols) / 2,
        top: available.rows.saturating_sub(rows) / 2,
    }
}

pub fn start_overlay<T, F>(
    term_window: &TermWindow,
    tab: &Arc<Tab>,
    func: F,
) -> (
    Arc<dyn Pane>,
    Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>>>>,
)
where
    T: Send + 'static,
    F: Send + 'static + FnOnce(TabId, TermWizTerminal) -> anyhow::Result<T>,
{
    start_overlay_with_dimensions(term_window, tab, OverlayDimensions::default(), func)
}

pub fn start_overlay_with_dimensions<T, F>(
    term_window: &TermWindow,
    tab: &Arc<Tab>,
    dimensions: OverlayDimensions,
    func: F,
) -> (
    Arc<dyn Pane>,
    Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>>>>,
)
where
    T: Send + 'static,
    F: Send + 'static + FnOnce(TabId, TermWizTerminal) -> anyhow::Result<T>,
{
    let tab_id = tab.tab_id();
    let overlay_size = resolve_overlay_dimensions(tab.get_size(), dimensions).size;
    let term_config: Arc<dyn TerminalConfiguration + Send + Sync> =
        Arc::new(config::TermConfig::with_config(term_window.config.clone()));
    let (tw_term, tw_tab) = allocate(overlay_size, term_config);

    let window = term_window.window.clone().unwrap();

    let overlay_pane_id = tw_tab.pane_id();

    let future = promise::spawn::spawn_into_new_thread(move || {
        let res = func(tab_id, tw_term);
        TermWindow::schedule_cancel_overlay(window, tab_id, Some(overlay_pane_id));
        res
    });

    (tw_tab, Box::pin(future))
}

pub fn start_overlay_pane<T, F>(
    term_window: &TermWindow,
    pane: &Arc<dyn Pane>,
    func: F,
) -> (
    Arc<dyn Pane>,
    Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>>>>,
)
where
    T: Send + 'static,
    F: Send + 'static + FnOnce(PaneId, TermWizTerminal) -> anyhow::Result<T>,
{
    let pane_id = pane.pane_id();
    let dims = pane.get_dimensions();
    let size = TerminalSize {
        cols: dims.cols,
        rows: dims.viewport_rows,
        pixel_width: term_window.render_metrics.cell_size.width as usize * dims.cols,
        pixel_height: term_window.render_metrics.cell_size.height as usize * dims.viewport_rows,
        dpi: dims.dpi,
    };
    let term_config: Arc<dyn TerminalConfiguration + Send + Sync> =
        Arc::new(config::TermConfig::with_config(term_window.config.clone()));
    let (tw_term, tw_tab) = allocate(size, term_config);

    let window = term_window.window.clone().unwrap();

    let future = promise::spawn::spawn_into_new_thread(move || {
        let res = func(pane_id, tw_term);
        TermWindow::schedule_cancel_overlay_for_pane(window, pane_id);
        res
    });

    (tw_tab, Box::pin(future))
}

#[cfg(test)]
mod test {
    use super::*;

    fn available_size() -> TerminalSize {
        TerminalSize {
            rows: 40,
            cols: 100,
            pixel_width: 1000,
            pixel_height: 800,
            dpi: 96,
        }
    }

    #[test]
    fn default_overlay_dimensions_fill_the_tab() {
        let resolved = resolve_overlay_dimensions(available_size(), OverlayDimensions::default());
        assert_eq!(resolved.size, available_size());
        assert_eq!(resolved.left, 0);
        assert_eq!(resolved.top, 0);
    }

    #[test]
    fn percentage_overlay_dimensions_are_centered() {
        let resolved = resolve_overlay_dimensions(
            available_size(),
            OverlayDimensions {
                width: SplitSize::Percent(60),
                height: SplitSize::Percent(50),
            },
        );
        assert_eq!(resolved.size.cols, 60);
        assert_eq!(resolved.size.rows, 20);
        assert_eq!(resolved.size.pixel_width, 600);
        assert_eq!(resolved.size.pixel_height, 400);
        assert_eq!(resolved.left, 20);
        assert_eq!(resolved.top, 10);
    }

    #[test]
    fn cell_overlay_dimensions_are_clamped_to_the_tab() {
        let resolved = resolve_overlay_dimensions(
            available_size(),
            OverlayDimensions {
                width: SplitSize::Cells(120),
                height: SplitSize::Cells(0),
            },
        );
        assert_eq!(resolved.size.cols, 100);
        assert_eq!(resolved.size.rows, 1);
        assert_eq!(resolved.left, 0);
        assert_eq!(resolved.top, 19);
    }
}
