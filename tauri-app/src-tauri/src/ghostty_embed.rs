use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::CString,
    os::raw::{c_char, c_void},
    ptr::{self, NonNull},
    sync::{atomic::{AtomicBool, Ordering}, OnceLock},
};

use serde::Deserialize;

#[cfg(target_os = "macos")]
use {
    block2::RcBlock,
    ghostty_sys::*,
    objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass},
    objc2::rc::Retained,
    objc2_app_kit::{NSEvent, NSEventModifierFlags, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindowOrderingMode},
    objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSTimer},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    tauri::Window,
};

#[derive(Debug, Deserialize, Clone, Copy, Default)]
pub struct GhosttyInsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GhosttyStyle {
    #[serde(default)]
    pub insets: GhosttyInsets,
    #[serde(default)]
    pub corner_radius: f64,
}

impl Default for GhosttyStyle {
    fn default() -> Self {
        Self {
            insets: GhosttyInsets::default(),
            corner_radius: 0.0,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct GhosttyRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(rename = "viewportWidth", default)]
    pub viewport_width: Option<f64>,
    #[serde(rename = "viewportHeight", default)]
    pub viewport_height: Option<f64>,
    #[serde(default)]
    pub style: GhosttyStyle,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GhosttyOptions {
    pub font_size: Option<f32>,
    pub working_directory: Option<String>,
    pub command: Option<String>,
}

impl Default for GhosttyOptions {
    fn default() -> Self {
        Self {
            font_size: None,
            working_directory: None,
            command: None,
        }
    }
}

#[derive(Default)]
pub struct GhosttyManager {
    instances: HashMap<String, Box<GhosttyInstance>>,
}

impl GhosttyManager {
    pub fn create(
        &mut self,
        window: &Window,
        id: String,
        rect: GhosttyRect,
        options: GhosttyOptions,
    ) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window, id, rect, options);
            return Err("Ghostty embedding is only supported on macOS".to_string());
        }

        #[cfg(target_os = "macos")]
        {
            if self.instances.contains_key(&id) {
                return Err(format!("Ghostty instance already exists: {id}"));
            }

            let instance = GhosttyInstance::new(window, rect, options)?;
            self.instances.insert(id, instance);
            Ok(())
        }
    }

    pub fn update_rect(
        &mut self,
        window: &Window,
        id: &str,
        rect: GhosttyRect,
    ) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window, id, rect);
            return Err("Ghostty embedding is only supported on macOS".to_string());
        }

        #[cfg(target_os = "macos")]
        {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| format!("Ghostty instance not found: {id}"))?;
            instance.update_rect(window, rect);
            Ok(())
        }
    }

    pub fn destroy(&mut self, id: &str) -> Result<(), String> {
        if self.instances.remove(id).is_none() {
            return Err(format!("Ghostty instance not found: {id}"));
        }
        Ok(())
    }

    pub fn set_visible(&mut self, id: &str, visible: bool) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (id, visible);
            return Err("Ghostty embedding is only supported on macOS".to_string());
        }

        #[cfg(target_os = "macos")]
        {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| format!("Ghostty instance not found: {id}"))?;
            instance.view.setHidden(!visible);
            Ok(())
        }
    }

    pub fn focus(&mut self, id: &str, focused: bool) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (id, focused);
            return Err("Ghostty embedding is only supported on macOS".to_string());
        }

        #[cfg(target_os = "macos")]
        {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| format!("Ghostty instance not found: {id}"))?;
            instance.set_focus(focused);
            Ok(())
        }
    }

    pub fn write_text(&mut self, id: &str, text: &str) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (id, text);
            return Err("Ghostty embedding is only supported on macOS".to_string());
        }

        #[cfg(target_os = "macos")]
        {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| format!("Ghostty instance not found: {id}"))?;

            // Split on \n and \r — send text segments via ghostty_surface_text
            // and newlines as Enter keypresses via ghostty_surface_key.
            let mut segment_start = 0;
            for (i, ch) in text.char_indices() {
                if ch == '\n' || ch == '\r' {
                    if i > segment_start {
                        let segment = &text[segment_start..i];
                        unsafe {
                            ghostty_surface_text(
                                instance.ghostty_surface,
                                segment.as_ptr() as *const _,
                                segment.len(),
                            );
                        }
                    }
                    // macOS virtual keycode for Return = 0x24
                    const VK_RETURN: u32 = 0x24;
                    let key_event = ghostty_input_key_s {
                        action: ghostty_input_action_e_GHOSTTY_ACTION_PRESS,
                        mods: ghostty_input_mods_e_GHOSTTY_MODS_NONE,
                        keycode: VK_RETURN,
                        text: ptr::null(),
                        composing: false,
                    };
                    unsafe {
                        ghostty_surface_key(instance.ghostty_surface, key_event);
                    }
                    segment_start = i + ch.len_utf8();
                }
            }
            // Send any remaining text after the last newline
            if segment_start < text.len() {
                let segment = &text[segment_start..];
                unsafe {
                    ghostty_surface_text(
                        instance.ghostty_surface,
                        segment.as_ptr() as *const _,
                        segment.len(),
                    );
                }
            }

            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
thread_local! {
    static GHOSTTY_MANAGER: RefCell<GhosttyManager> = RefCell::new(GhosttyManager::default());
}

#[cfg(target_os = "macos")]
pub fn with_manager<R>(f: impl FnOnce(&mut GhosttyManager) -> R) -> R {
    GHOSTTY_MANAGER.with(|cell| f(&mut cell.borrow_mut()))
}

#[cfg(not(target_os = "macos"))]
pub fn with_manager<R>(_f: impl FnOnce(&mut GhosttyManager) -> R) -> R {
    let mut manager = GhosttyManager::default();
    _f(&mut manager)
}

#[cfg(target_os = "macos")]
struct RuntimeFlags {
    needs_tick: AtomicBool,
    close_requested: AtomicBool,
}

#[cfg(target_os = "macos")]
impl RuntimeFlags {
    fn new() -> Self {
        Self {
            needs_tick: AtomicBool::new(false),
            close_requested: AtomicBool::new(false),
        }
    }
}

#[cfg(target_os = "macos")]
struct GhosttyInstance {
    ghostty_app: ghostty_app_t,
    ghostty_surface: ghostty_surface_t,
    view: Retained<GhosttyView>,
    timer: Option<Retained<NSTimer>>,
    flags: RuntimeFlags,
}

#[cfg(target_os = "macos")]
impl GhosttyInstance {
    fn new(window: &Window, rect: GhosttyRect, options: GhosttyOptions) -> Result<Box<Self>, String> {
        let (content_view, webview_view) = content_and_webview(window)?;
        let mtm = MainThreadMarker::new().ok_or("not on main thread")?;

        let frame = rect_to_frame(&content_view, &webview_view, rect);
        let view = GhosttyView::new(frame, mtm);

        unsafe {
            content_view.addSubview_positioned_relativeTo(
                &view,
                NSWindowOrderingMode::NSWindowAbove,
                Some(&webview_view),
            );
        }

        // Enable mouse-moved events on the window so mouseMoved: fires
        if let Some(ns_window) = webview_view.window() {
            ns_window.setAcceptsMouseMovedEvents(true);
        }

        let mut instance = Box::new(Self {
            ghostty_app: ptr::null_mut(),
            ghostty_surface: ptr::null_mut(),
            view,
            timer: None,
            flags: RuntimeFlags::new(),
        });

        let instance_ptr = &mut *instance as *mut GhosttyInstance;

        static GHOSTTY_INIT: OnceLock<Result<(), String>> = OnceLock::new();
        let init_result = GHOSTTY_INIT.get_or_init(|| {
            let res = unsafe { ghostty_init() };
            if res != GHOSTTY_SUCCESS as i32 {
                Err("ghostty_init failed".to_string())
            } else {
                Ok(())
            }
        });
        if let Err(e) = init_result {
            return Err(e.clone());
        }

        let config = unsafe { ghostty_config_new() };
        if config.is_null() {
            return Err("ghostty_config_new failed".to_string());
        }

        unsafe {
            ghostty_config_load_default_files(config);
            ghostty_config_load_cli_args(config);
            ghostty_config_load_recursive_files(config);
            ghostty_config_finalize(config);
        }

        let runtime_config = ghostty_runtime_config_s {
            userdata: instance_ptr as *mut c_void,
            supports_selection_clipboard: false,
            wakeup_cb: Some(runtime_wakeup_cb),
            action_cb: Some(runtime_action_cb),
            read_clipboard_cb: Some(runtime_read_clipboard_cb),
            confirm_read_clipboard_cb: Some(runtime_confirm_read_clipboard_cb),
            write_clipboard_cb: Some(runtime_write_clipboard_cb),
            close_surface_cb: Some(runtime_close_surface_cb),
        };

        let app = unsafe { ghostty_app_new(&runtime_config, config) };
        unsafe {
            ghostty_config_free(config);
        }
        if app.is_null() {
            return Err("ghostty_app_new failed".to_string());
        }
        instance.ghostty_app = app;

        let mut surface_config = unsafe { ghostty_surface_config_new() };
        surface_config.platform_tag = ghostty_platform_e_GHOSTTY_PLATFORM_MACOS;
        surface_config.platform.macos.nsview = Retained::as_ptr(&instance.view) as *const _ as *mut _;
        surface_config.userdata = instance_ptr as *mut _;
        surface_config.scale_factor = backing_scale_factor(&webview_view);
        surface_config.font_size = options.font_size.unwrap_or(0.0);

        let _wd = options
            .working_directory
            .as_ref()
            .and_then(|s| CString::new(s.as_str()).ok());
        let _cmd = options
            .command
            .as_ref()
            .and_then(|s| CString::new(s.as_str()).ok());

        if let Some(wd) = _wd.as_ref() {
            surface_config.working_directory = wd.as_ptr();
        }
        if let Some(cmd) = _cmd.as_ref() {
            surface_config.command = cmd.as_ptr();
        }

        let surface = unsafe { ghostty_surface_new(instance.ghostty_app, &mut surface_config) };
        if surface.is_null() {
            return Err("ghostty_surface_new failed".to_string());
        }
        instance.ghostty_surface = surface;

        instance.view.set_state_ptr(instance_ptr);
        instance.update_rect(window, rect);

        let instance_ptr_for_timer = instance_ptr as usize;
        let tick_block: RcBlock<dyn Fn(NonNull<NSTimer>)> = RcBlock::new(move |_timer| {
            let instance = unsafe { &mut *(instance_ptr_for_timer as *mut GhosttyInstance) };
            instance.tick();
        });

        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 60.0, true, &tick_block)
        };
        instance.timer = Some(timer);

        Ok(instance)
    }

    fn tick(&mut self) {
        if self.flags.close_requested.swap(false, Ordering::AcqRel) {
            if let Some(window) = self.view.window() {
                window.close();
            }
            return;
        }

        let _ = self.flags.needs_tick.swap(false, Ordering::AcqRel);
        unsafe {
            ghostty_app_tick(self.ghostty_app);
            ghostty_surface_draw(self.ghostty_surface);
        }
    }

    fn update_rect(&mut self, window: &Window, rect: GhosttyRect) {
        let (content_view, webview_view) = match content_and_webview(window) {
            Ok(tuple) => tuple,
            Err(_) => return,
        };

        let frame = rect_to_frame(&content_view, &webview_view, rect);
        unsafe {
            self.view.setFrame(frame);
        }

        self.apply_style(rect.style);

        // Use the window's backing scale factor directly (matches the working
        // standalone implementation). Avoids potential issues with
        // convertRectToBacking on layer-backed views where contentsScale
        // may not yet reflect the correct backing factor.
        let bounds = self.view.bounds();
        let scale = backing_scale_factor(self.view.as_super());
        let width_px = (bounds.size.width * scale).max(1.0).round() as u32;
        let height_px = (bounds.size.height * scale).max(1.0).round() as u32;
        unsafe {
            ghostty_surface_set_content_scale(self.ghostty_surface, scale, scale);
            ghostty_surface_set_size(self.ghostty_surface, width_px, height_px);
        }
    }

    fn apply_style(&self, style: GhosttyStyle) {
        let corner = style.corner_radius.max(0.0);
        self.view.setWantsLayer(true);
        let layer = unsafe { self.view.layer() };
        if let Some(layer) = layer.as_ref() {
            layer.setCornerRadius(corner);
            layer.setMasksToBounds(corner > 0.0);
        }
    }

    fn set_focus(&mut self, focused: bool) {
        unsafe {
            ghostty_surface_set_focus(self.ghostty_surface, focused);
            ghostty_app_set_focus(self.ghostty_app, focused);
        }
        if focused {
            if let Some(window) = self.view.window() {
                let responder = self.view.as_super().as_super();
                window.makeFirstResponder(Some(responder));
            }
        }
    }

    fn handle_key(&mut self, event: &NSEvent, action: ghostty_input_action_e) {
        let mods = mods_from_event(event);
        let keycode = unsafe { event.keyCode() } as u32;

        let mut text_ptr: *const c_char = ptr::null();
        let flags = unsafe { event.modifierFlags() };
        let allow_text = !flags.contains(NSEventModifierFlags::NSEventModifierFlagCommand)
            && !flags.contains(NSEventModifierFlags::NSEventModifierFlagControl);

        if allow_text {
            if let Some(chars) = unsafe { event.characters() } {
                let utf8 = chars.UTF8String();
                if !utf8.is_null() {
                    text_ptr = utf8;
                }
            }
        }

        let key_event = ghostty_input_key_s {
            action,
            mods,
            keycode,
            text: text_ptr,
            composing: false,
        };

        unsafe {
            ghostty_surface_key(self.ghostty_surface, key_event);
        }
    }

    fn handle_mouse_button(
        &mut self,
        event: &NSEvent,
        action: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
    ) {
        let (x, y) = self.event_position_px(event);
        let mods = mods_from_event(event);
        unsafe {
            ghostty_surface_mouse_pos(self.ghostty_surface, x, y, mods as u32);
            ghostty_surface_mouse_button(self.ghostty_surface, action, button, mods as u32);
        }
    }

    fn handle_mouse_move(&mut self, event: &NSEvent) {
        let (x, y) = self.event_position_px(event);
        let mods = mods_from_event(event);
        unsafe {
            ghostty_surface_mouse_pos(self.ghostty_surface, x, y, mods as u32);
        }
    }

    fn handle_scroll(&mut self, event: &NSEvent) {
        let mods = mods_from_event(event);
        let dx = unsafe { event.scrollingDeltaX() } as f64;
        let dy = unsafe { event.scrollingDeltaY() } as f64;
        unsafe {
            ghostty_surface_mouse_scroll(self.ghostty_surface, dx, dy, mods as i32);
        }
    }

    fn event_position_px(&self, event: &NSEvent) -> (f64, f64) {
        let location = unsafe { event.locationInWindow() };
        let local = self.view.convertPoint_fromView(location, None);
        let bounds = self.view.bounds();

        // Ghostty embedded input expects view-space coordinates (points), not
        // backing pixels. It applies content scale internally.
        let x = local.x;
        let y = bounds.size.height - local.y;

        use std::sync::atomic::{AtomicU32, Ordering as AtOrd};
        static LOG_COUNT: AtomicU32 = AtomicU32::new(0);
        let count = LOG_COUNT.fetch_add(1, AtOrd::Relaxed);
        if count < 10 {
            eprintln!("[ghostty-diag] event_position_px (#{count}):");
            eprintln!("  locationInWindow: ({}, {})", location.x, location.y);
            eprintln!("  local: ({}, {})", local.x, local.y);
            eprintln!("  bounds: ({}, {})", bounds.size.width, bounds.size.height);
            eprintln!("  result: x={x}, y={y}");
        }

        (x, y)
    }
}

#[cfg(target_os = "macos")]
impl Drop for GhosttyInstance {
    fn drop(&mut self) {
        unsafe {
            if let Some(timer) = self.timer.take() {
                timer.invalidate();
            }
            self.view.removeFromSuperview();
            ghostty_surface_free(self.ghostty_surface);
            ghostty_app_free(self.ghostty_app);
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct ViewIvars {
    state_ptr: Cell<*mut GhosttyInstance>,
}

#[cfg(target_os = "macos")]
declare_class!(
    struct GhosttyView;

    unsafe impl ClassType for GhosttyView {
        type Super = NSView;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "GhosttyEmbeddedView";
    }

    impl DeclaredClass for GhosttyView {
        type Ivars = ViewIvars;
    }

    unsafe impl GhosttyView {
        #[method(acceptsFirstResponder)]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[method(becomeFirstResponder)]
        fn become_first_responder(&self) -> bool {
            self.with_state(|state| state.set_focus(true));
            true
        }

        #[method(resignFirstResponder)]
        fn resign_first_responder(&self) -> bool {
            self.with_state(|state| state.set_focus(false));
            true
        }

        #[method(keyDown:)]
        fn key_down(&self, event: &NSEvent) {
            self.with_state(|state| {
                let action = if unsafe { event.isARepeat() } {
                    ghostty_input_action_e_GHOSTTY_ACTION_REPEAT
                } else {
                    ghostty_input_action_e_GHOSTTY_ACTION_PRESS
                };
                state.handle_key(event, action);
            });
        }

        #[method(keyUp:)]
        fn key_up(&self, event: &NSEvent) {
            self.with_state(|state| {
                state.handle_key(event, ghostty_input_action_e_GHOSTTY_ACTION_RELEASE);
            });
        }

        #[method(mouseDown:)]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(window) = self.window() {
                let responder = self.as_super().as_super();
                window.makeFirstResponder(Some(responder));
            }
            self.with_state(|state| {
                state.handle_mouse_button(
                    event,
                    ghostty_input_mouse_state_e_GHOSTTY_MOUSE_PRESS,
                    ghostty_input_mouse_button_e_GHOSTTY_MOUSE_LEFT,
                );
            });
        }

        #[method(mouseUp:)]
        fn mouse_up(&self, event: &NSEvent) {
            self.with_state(|state| {
                state.handle_mouse_button(
                    event,
                    ghostty_input_mouse_state_e_GHOSTTY_MOUSE_RELEASE,
                    ghostty_input_mouse_button_e_GHOSTTY_MOUSE_LEFT,
                );
            });
        }

        #[method(rightMouseDown:)]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.with_state(|state| {
                state.handle_mouse_button(
                    event,
                    ghostty_input_mouse_state_e_GHOSTTY_MOUSE_PRESS,
                    ghostty_input_mouse_button_e_GHOSTTY_MOUSE_RIGHT,
                );
            });
        }

        #[method(rightMouseUp:)]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.with_state(|state| {
                state.handle_mouse_button(
                    event,
                    ghostty_input_mouse_state_e_GHOSTTY_MOUSE_RELEASE,
                    ghostty_input_mouse_button_e_GHOSTTY_MOUSE_RIGHT,
                );
            });
        }

        #[method(mouseMoved:)]
        fn mouse_moved(&self, event: &NSEvent) {
            self.with_state(|state| state.handle_mouse_move(event));
        }

        #[method(mouseDragged:)]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.with_state(|state| state.handle_mouse_move(event));
        }

        #[method(scrollWheel:)]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.with_state(|state| state.handle_scroll(event));
        }

        #[method(updateTrackingAreas)]
        fn update_tracking_areas(&self) {
            unsafe {
                // Remove all existing tracking areas
                let areas = self.trackingAreas();
                let count = areas.count();
                for i in 0..count {
                    let area = areas.objectAtIndex(i);
                    self.removeTrackingArea(&area);
                }

                let options = NSTrackingAreaOptions::NSTrackingMouseMoved
                    | NSTrackingAreaOptions::NSTrackingMouseEnteredAndExited
                    | NSTrackingAreaOptions::NSTrackingActiveInKeyWindow
                    | NSTrackingAreaOptions::NSTrackingInVisibleRect;

                let tracking_area = NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    self.bounds(),
                    options,
                    Some(self),
                    None,
                );

                self.addTrackingArea(&tracking_area);
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl GhosttyView {
    fn new(frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(ViewIvars {
            state_ptr: Cell::new(ptr::null_mut()),
        });
        unsafe { msg_send_id![super(this), initWithFrame: frame] }
    }

    fn set_state_ptr(&self, ptr: *mut GhosttyInstance) {
        self.ivars().state_ptr.set(ptr);
    }

    fn with_state(&self, f: impl FnOnce(&mut GhosttyInstance)) {
        let ptr = self.ivars().state_ptr.get();
        if ptr.is_null() {
            return;
        }
        unsafe { f(&mut *ptr) };
    }
}

#[cfg(target_os = "macos")]
fn content_and_webview(window: &Window) -> Result<(Retained<NSView>, Retained<NSView>), String> {
    let handle = window
        .window_handle()
        .map_err(|_| "failed to get window handle")?;
    let raw = handle.as_raw();
    let ns_view_ptr = match raw {
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
        _ => return Err("not an AppKit window".to_string()),
    };

    let webview = unsafe { &*(ns_view_ptr as *mut NSView) };
    let webview_retained = unsafe { Retained::retain(webview as *const _ as *mut NSView) }
        .ok_or_else(|| "failed to retain webview".to_string())?;

    let content_view = if let Some(ns_window) = webview.window() {
        ns_window
            .contentView()
            .unwrap_or_else(|| webview_retained.clone())
    } else {
        webview_retained.clone()
    };

    Ok((content_view, webview_retained))
}

#[cfg(target_os = "macos")]
fn rect_to_frame(content_view: &NSView, webview_view: &NSView, rect: GhosttyRect) -> NSRect {
    let insets = rect.style.insets;
    let mut width = rect.width - (insets.left + insets.right);
    let mut height = rect.height - (insets.top + insets.bottom);
    width = width.max(1.0);
    height = height.max(1.0);

    let x = rect.x + insets.left;
    let y = rect.y + insets.top;

    let webview_frame = webview_view.frame();
    let webview_bounds = webview_view.bounds();
    let webview_flipped: bool = unsafe { objc2::msg_send![webview_view, isFlipped] };
    let viewport_height = rect
        .viewport_height
        .filter(|v| *v > 0.0)
        .unwrap_or(webview_bounds.size.height);
    let viewport_width = rect
        .viewport_width
        .filter(|v| *v > 0.0)
        .unwrap_or(webview_bounds.size.width);

    // Convert from web (CSS) coordinates to webview view coordinates.
    // JS coordinates are relative to the visible web viewport (window.inner*),
    // while AppKit bounds can include titlebar/full-size insets.
    let y_in_webview = if webview_flipped {
        webview_bounds.origin.y + y
    } else {
        webview_bounds.origin.y + viewport_height - y - height
    };

    let rect_in_webview = NSRect::new(
        NSPoint::new(webview_bounds.origin.x + x, y_in_webview),
        NSSize::new(width, height),
    );
    let rect_in_window: NSRect =
        unsafe { objc2::msg_send![webview_view, convertRect: rect_in_webview, toView: ptr::null::<NSView>()] };
    let frame: NSRect =
        unsafe { objc2::msg_send![content_view, convertRect: rect_in_window, fromView: ptr::null::<NSView>()] };

    eprintln!("[ghostty-diag] rect_to_frame:");
    eprintln!("  JS rect: x={}, y={}, w={}, h={}", rect.x, rect.y, rect.width, rect.height);
    eprintln!("  insets: top={}, right={}, bottom={}, left={}", insets.top, insets.right, insets.bottom, insets.left);
    eprintln!("  after insets: x={x}, y={y}, w={width}, h={height}");
    eprintln!("  webview_frame: origin=({}, {}), size=({}, {})",
        webview_frame.origin.x, webview_frame.origin.y,
        webview_frame.size.width, webview_frame.size.height);
    eprintln!("  webview_bounds: origin=({}, {}), size=({}, {})",
        webview_bounds.origin.x, webview_bounds.origin.y,
        webview_bounds.size.width, webview_bounds.size.height);
    eprintln!("  viewport: width={viewport_width}, height={viewport_height}");
    eprintln!(
        "  viewport inset: dx={}, dy={}",
        webview_bounds.size.width - viewport_width,
        webview_bounds.size.height - viewport_height
    );
    let content_flipped: bool = unsafe { objc2::msg_send![content_view, isFlipped] };
    eprintln!("  content_view.isFlipped={content_flipped}");
    eprintln!("  webview.isFlipped={webview_flipped}");
    eprintln!("  result frame: origin=({}, {}), size=({}, {})",
        frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);

    frame
}

#[cfg(target_os = "macos")]
fn backing_scale_factor(view: &NSView) -> f64 {
    if let Some(window) = view.window() {
        window.backingScaleFactor() as f64
    } else {
        1.0
    }
}

#[cfg(target_os = "macos")]
fn mods_from_event(event: &NSEvent) -> ghostty_input_mods_e {
    let flags = unsafe { event.modifierFlags() };
    let mut mods: ghostty_input_mods_e = ghostty_input_mods_e_GHOSTTY_MODS_NONE;

    if flags.contains(NSEventModifierFlags::NSEventModifierFlagShift) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_SHIFT;
    }
    if flags.contains(NSEventModifierFlags::NSEventModifierFlagControl) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_CTRL;
    }
    if flags.contains(NSEventModifierFlags::NSEventModifierFlagOption) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_ALT;
    }
    if flags.contains(NSEventModifierFlags::NSEventModifierFlagCommand) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_SUPER;
    }
    if flags.contains(NSEventModifierFlags::NSEventModifierFlagCapsLock) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_CAPS;
    }
    if flags.contains(NSEventModifierFlags::NSEventModifierFlagNumericPad) {
        mods |= ghostty_input_mods_e_GHOSTTY_MODS_NUM;
    }

    mods
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_wakeup_cb(userdata: *mut c_void) {
    if userdata.is_null() {
        return;
    }
    let instance = unsafe { &mut *(userdata as *mut GhosttyInstance) };
    instance.flags.needs_tick.store(true, Ordering::Release);
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_action_cb(
    _app: ghostty_app_t,
    _target: ghostty_target_s,
    _action: ghostty_action_s,
) -> bool {
    false
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_read_clipboard_cb(
    _userdata: *mut c_void,
    _clipboard: ghostty_clipboard_e,
    _request: *mut c_void,
) {
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_confirm_read_clipboard_cb(
    _userdata: *mut c_void,
    _value: *const c_char,
    _request: *mut c_void,
    _request_type: ghostty_clipboard_request_e,
) {
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_write_clipboard_cb(
    _userdata: *mut c_void,
    _value: *const c_char,
    _clipboard: ghostty_clipboard_e,
    _confirm: bool,
) {
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn runtime_close_surface_cb(userdata: *mut c_void, _confirm: bool) {
    if userdata.is_null() {
        return;
    }
    let instance = unsafe { &mut *(userdata as *mut GhosttyInstance) };
    instance.flags.close_requested.store(true, Ordering::Release);
}
