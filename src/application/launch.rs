// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The launch path: the event loop, the window, and the first frame.

use crate::application::launch_options::LaunchOptions;
use crate::application::platform::screenshot;
use crate::application::view::render::root_logic;
use crate::application::view::{UI_FONT, default_property_set};
use crate::application::workspace::AppState;
use masonry::core::{ErasedAction, WidgetId};
use masonry_winit::app::{AppDriver, DriverCtx, WgpuContext, WindowId};
use std::cell::Cell;
use std::path::Path as FsPath;
use std::rc::Rc;
use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use winit::window::Theme;
use xilem::view::sized_box;
use xilem::{EventLoopBuilder, Xilem};

pub(crate) fn run(
    event_loop: EventLoopBuilder,
    path: Option<&FsPath>,
) -> Result<(), EventLoopError> {
    crate::application::platform::config::load().apply();
    let options = LaunchOptions::current();
    let mut app = AppState::open(path);
    // RUNEBENDER_GLYPH=<name> starts in the editor on that glyph, so
    // a headless screenshot reaches edit mode without clicks.
    if let Some(workspace) = app.workspace.as_mut() {
        options.apply(workspace);
    }
    // Headless: render one frame and exit. No window, no event loop.
    if let Some(capture) = &options.capture {
        if let Some(workspace) = app.workspace.as_mut() {
            options.apply_capture(workspace);
        }

        // The harness needs a root widget with a concrete type, so wrap
        // the app's root view in a sized box.
        // The capture keeps the logical layout fixed at its size and applies the device-pixel
        // scale to it, so a scale of 2 is a true 2x proof and not a larger, reflowed window.
        let background = app.background();
        screenshot::render_to(
            app,
            background,
            |app: &mut AppState| sized_box(root_logic(app)),
            capture.size,
            capture.scale,
            &capture.path,
        );
        return Ok(());
    }
    // Xilem builds our first view (and installs the muda menu) before winit
    // starts its AppKit launch callback. Winit's default application-only menu
    // would then replace ours. Runebender owns the complete native menu bar.
    #[cfg(target_os = "macos")]
    let event_loop = {
        use winit::platform::macos::EventLoopBuilderExtMacOS as _;
        let mut event_loop = event_loop;
        event_loop.with_default_menu(false);
        event_loop
    };
    let initial_background = app.background();
    let initial_theme = app.palette.window_theme();
    let window_theme = Rc::new(Cell::new(initial_theme));
    let theme_for_view = window_theme.clone();
    #[cfg(target_os = "macos")]
    let initial_transparency = app.palette.blur_background;
    let window_id = WindowId::next();
    let application = Xilem::new(app, move |app| {
        theme_for_view.set(app.palette.window_theme());
        #[cfg(target_os = "macos")]
        if let Some(workspace) = app.workspace.as_mut() {
            workspace.fullscreen = crate::application::platform::window::is_fullscreen();
        }
        #[cfg(target_os = "macos")]
        crate::application::platform::window::set_backdrop(app.palette.backdrop_style());
        let background = app.background();
        #[cfg(target_os = "macos")]
        let background = if crate::application::platform::window::backdrop_active() {
            xilem::Color::TRANSPARENT
        } else {
            background
        };
        let content = root_logic(app);
        // RUNEBENDER_FRAME_STATS=1 renders every frame and reports the cadence on stderr.
        let content = if options.frame_stats {
            xilem::core::one_of::OneOf2::A(crate::application::widgets::frame_stats::frame_stats(
                content,
            ))
        } else {
            xilem::core::one_of::OneOf2::B(content)
        };
        #[cfg(target_os = "macos")]
        let content = xilem::view::resize_observer(
            |app: &mut AppState, _| {
                if let Some(workspace) = app.workspace.as_mut() {
                    workspace.fullscreen = crate::application::platform::window::is_fullscreen();
                }
                crate::application::platform::window::set_backdrop(app.palette.backdrop_style());
            },
            content,
        );
        #[cfg(target_os = "macos")]
        let content = crate::application::platform::window::with_backdrop(
            content,
            app.palette.blur_background,
        );
        // An open document cannot shrink the window past its panels' minimum widths.
        // The option is reactive, so collapsing a dock lowers the limit.
        let min_width = app.workspace.as_ref().map(|workspace| {
            crate::application::view::render::workspace_min_width(
                workspace.left_collapsed,
                workspace.right_collapsed,
            )
        });
        let view = xilem::window(window_id, "Runebender", content)
            // Stock widgets follow the active theme; the window swaps them on a theme change.
            .with_default_properties(app.palette.widget_properties())
            .with_options(|options| {
                let options = options
                    .with_initial_inner_size(LogicalSize::new(
                        crate::application::design::DEFAULT_WINDOW_WIDTH,
                        crate::application::design::DEFAULT_WINDOW_HEIGHT,
                    ))
                    .on_close(AppState::request_quit)
                    .on_file_drop(AppState::file_dropped);
                let options = match min_width {
                    Some(width) => options.with_min_inner_size(LogicalSize::new(width, 0.0)),
                    None => options,
                };
                // On macOS the header row is the title bar: content runs under
                // the transparent system bar and pads for traffic lights.
                #[cfg(target_os = "macos")]
                let options = {
                    use xilem::WindowOptionsExtMacOS as _;
                    options
                        // Preserve the opaque native window while blur is disabled.
                        // The backdrop setup makes the window clear itself when the
                        // View menu turns blur on later.
                        .with_transparent(initial_transparency)
                        .with_titlebar_transparent(true)
                        .with_fullsize_content_view(true)
                        .with_title_hidden(true)
                };
                options
            })
            .with_base_color(background);
        std::iter::once(view)
    })
    .with_font(xilem::Blob::new(Arc::new(UI_FONT)))
    .with_default_base_color(initial_background);
    // The pinned Xilem WindowOptions lacks a native appearance setter.
    // Its public Masonry driver path supports both initial and live winit themes.
    let event_loop = {
        let mut event_loop = event_loop;
        event_loop.build()?
    };
    let proxy = event_loop.create_proxy();
    let (driver, mut windows) = application
        .into_driver_and_windows(move |event| proxy.send_event(event).map_err(|error| error.0));
    for window in &mut windows {
        window.attributes = window.attributes.clone().with_theme(Some(initial_theme));
    }
    let driver = WindowAppearanceDriver {
        inner: driver,
        theme: window_theme,
        applied: initial_theme,
    };
    masonry_winit::app::run_with(event_loop, windows, driver, default_property_set())
}

/// Forward Xilem events while keeping the application's single native window themed.
struct WindowAppearanceDriver<D> {
    inner: D,
    theme: Rc<Cell<Theme>>,
    applied: Theme,
}

impl<D> WindowAppearanceDriver<D> {
    fn sync_theme(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_>) {
        let theme = self.theme.get();
        if theme != self.applied {
            ctx.window(window_id).handle().set_theme(Some(theme));
            self.applied = theme;
        }
    }
}

impl<D: AppDriver> AppDriver for WindowAppearanceDriver<D> {
    fn on_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_>,
        widget_id: WidgetId,
        action: ErasedAction,
    ) {
        self.inner.on_action(window_id, ctx, widget_id, action);
        self.sync_theme(window_id, ctx);
    }

    fn on_async_action(
        &mut self,
        window_id: WindowId,
        ctx: &mut DriverCtx<'_>,
        action: ErasedAction,
    ) {
        self.inner.on_async_action(window_id, ctx, action);
        self.sync_theme(window_id, ctx);
    }

    fn on_start(&mut self, state: &mut masonry_winit::app::MasonryState) {
        self.inner.on_start(state);
    }

    fn on_close_requested(&mut self, window_id: WindowId, ctx: &mut DriverCtx<'_>) {
        // Closing cannot change the palette and may remove the window.
        self.inner.on_close_requested(window_id, ctx);
    }

    fn on_wgpu_ready(&mut self, wgpu: &WgpuContext<'_>) {
        self.inner.on_wgpu_ready(wgpu);
    }

    fn on_file_dropped(
        &mut self,
        window_id: WindowId,
        path: std::path::PathBuf,
        ctx: &mut DriverCtx<'_>,
    ) {
        self.inner.on_file_dropped(window_id, path, ctx);
        self.sync_theme(window_id, ctx);
    }
}
