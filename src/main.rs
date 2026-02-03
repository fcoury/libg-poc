#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    cell::{Cell, RefCell},
    ffi::CString,
    os::raw::{c_char, c_void},
    ptr,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};

use block2::RcBlock;
use ghostty_sys::*;
use objc2::{
    declare_class, msg_send, msg_send_id, mutability, ClassType, DeclaredClass,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSEvent, NSEventModifierFlags, NSAutoresizingMaskOptions, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer,
};

struct RuntimeFlags {
    needs_tick: AtomicBool,
    close_requested: AtomicBool,
}

impl RuntimeFlags {
    fn new() -> Self {
        Self {
            needs_tick: AtomicBool::new(false),
            close_requested: AtomicBool::new(false),
        }
    }
}

struct AppState {
    ghostty_app: ghostty_app_t,
    ghostty_surface: ghostty_surface_t,
    window: Retained<NSWindow>,
    view: Retained<GhosttyView>,
    timer: Option<Retained<NSTimer>>,
    flags: RuntimeFlags,
}

impl AppState {
    fn new(window: Retained<NSWindow>, view: Retained<GhosttyView>) -> Result<Box<Self>, String> {
        let mut state = Box::new(Self {
            ghostty_app: ptr::null_mut(),
            ghostty_surface: ptr::null_mut(),
            window,
            view,
            timer: None,
            flags: RuntimeFlags::new(),
        });

        let state_ptr = &mut *state as *mut AppState;

        let init_res = unsafe { ghostty_init() };
        if init_res != GHOSTTY_SUCCESS as _ {
            return Err("ghostty_init failed".to_string());
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
            userdata: state_ptr as *mut c_void,
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
        state.ghostty_app = app;

        let mut surface_config = unsafe { ghostty_surface_config_new() };
        surface_config.platform_tag = ghostty_platform_e_GHOSTTY_PLATFORM_MACOS;
        surface_config.platform.macos.nsview =
            Retained::as_ptr(&state.view) as *const _ as *mut c_void;
        surface_config.userdata = state_ptr as *mut c_void;

        let scale = state.window.backingScaleFactor() as f64;
        surface_config.scale_factor = scale;
        surface_config.font_size = 0.0;

        let surface = unsafe { ghostty_surface_new(state.ghostty_app, &mut surface_config) };
        if surface.is_null() {
            return Err("ghostty_surface_new failed".to_string());
        }
        state.ghostty_surface = surface;

        state.update_surface_metrics();

        let state_ptr_for_timer = state_ptr as usize;
        let tick_block: RcBlock<dyn Fn(NonNull<NSTimer>)> = RcBlock::new(move |_timer| {
            let state = unsafe { &mut *(state_ptr_for_timer as *mut AppState) };
            state.tick();
        });

        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 60.0, true, &tick_block)
        };
        state.timer = Some(timer);

        Ok(state)
    }

    fn tick(&mut self) {
        if self.flags.close_requested.swap(false, Ordering::AcqRel) {
            self.window.close();
            return;
        }

        let _ = self.flags.needs_tick.swap(false, Ordering::AcqRel);

        unsafe {
            ghostty_app_tick(self.ghostty_app);
            ghostty_surface_draw(self.ghostty_surface);
        }
    }

    fn update_surface_metrics(&mut self) {
        let bounds = self.view.bounds();
        let scale = self.window.backingScaleFactor() as f64;
        let width_px = (bounds.size.width * scale) as u32;
        let height_px = (bounds.size.height * scale) as u32;

        unsafe {
            ghostty_surface_set_content_scale(self.ghostty_surface, scale, scale);
            ghostty_surface_set_size(self.ghostty_surface, width_px, height_px);
        }
    }

    fn handle_key(&mut self, event: &NSEvent, action: ghostty_input_action_e) {
        let mods = mods_from_event(event);
        let keycode = unsafe { event.keyCode() } as u32;

        let mut text_ptr: *const c_char = ptr::null();
        let mut _text_storage: Option<CString> = None;
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
        } else if let Some(chars) = unsafe { event.characters() } {
            let s = chars.UTF8String();
            if !s.is_null() {
                if let Ok(cstr) = unsafe { std::ffi::CStr::from_ptr(s) }.to_str() {
                    if let Ok(cstring) = CString::new(cstr) {
                        text_ptr = cstring.as_ptr();
                        _text_storage = Some(cstring);
                    }
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
            ghostty_surface_mouse_pos(self.ghostty_surface, x, y, mods);
            ghostty_surface_mouse_button(self.ghostty_surface, action, button, mods);
        }
    }

    fn handle_mouse_move(&mut self, event: &NSEvent) {
        let (x, y) = self.event_position_px(event);
        let mods = mods_from_event(event);
        unsafe {
            ghostty_surface_mouse_pos(self.ghostty_surface, x, y, mods);
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
        let scale = self.window.backingScaleFactor() as f64;
        let x = local.x * scale;
        let y = (bounds.size.height - local.y) * scale;
        (x, y)
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            unsafe { timer.invalidate() };
            drop(timer);
        }

        unsafe {
            if !self.ghostty_surface.is_null() {
                ghostty_surface_free(self.ghostty_surface);
            }
            if !self.ghostty_app.is_null() {
                ghostty_app_free(self.ghostty_app);
            }
        }
    }
}

#[derive(Debug)]
struct ViewIvars {
    state_ptr: Cell<*mut AppState>,
}

declare_class!(
    struct GhosttyView;

    unsafe impl ClassType for GhosttyView {
        type Super = NSView;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "GhosttyView";
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
            self.with_state(|state| unsafe {
                ghostty_surface_set_focus(state.ghostty_surface, true);
                ghostty_app_set_focus(state.ghostty_app, true);
            });
            true
        }

        #[method(resignFirstResponder)]
        fn resign_first_responder(&self) -> bool {
            self.with_state(|state| unsafe {
                ghostty_surface_set_focus(state.ghostty_surface, false);
                ghostty_app_set_focus(state.ghostty_app, false);
            });
            true
        }

        #[method(setFrameSize:)]
        fn set_frame_size(&self, new_size: NSSize) {
            unsafe {
                let _: () = msg_send![super(self), setFrameSize: new_size];
            }
            self.with_state(|state| state.update_surface_metrics());
        }

        #[method(viewDidMoveToWindow)]
        fn view_did_move_to_window(&self) {
            unsafe {
                let _: () = msg_send![super(self), viewDidMoveToWindow];
            }
            if let Some(window) = self.window() {
                let responder = self.as_super().as_super();
                window.makeFirstResponder(Some(responder));
            }
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
    }
);

impl GhosttyView {
    fn new(frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(ViewIvars {
            state_ptr: Cell::new(ptr::null_mut()),
        });
        unsafe { msg_send_id![super(this), initWithFrame: frame] }
    }

    fn set_state_ptr(&self, ptr: *mut AppState) {
        self.ivars().state_ptr.set(ptr);
    }

    fn with_state(&self, f: impl FnOnce(&mut AppState)) {
        let ptr = self.ivars().state_ptr.get();
        if ptr.is_null() {
            return;
        }
        unsafe { f(&mut *ptr) };
    }
}

struct AppDelegateIvars {
    state: RefCell<Option<Box<AppState>>>,
}

declare_class!(
    struct AppDelegate;

    unsafe impl ClassType for AppDelegate {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "GhosttyAppDelegate";
    }

    impl DeclaredClass for AppDelegate {
        type Ivars = AppDelegateIvars;
    }

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[method(applicationDidFinishLaunching:)]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::new().expect("main thread");
            let app = NSApplication::sharedApplication(mtm);

            let window = create_window(mtm);
            let view = GhosttyView::new(window.contentRectForFrameRect(window.frame()), mtm);
            unsafe {
                view.setAutoresizingMask(
                    NSAutoresizingMaskOptions::NSViewWidthSizable
                        | NSAutoresizingMaskOptions::NSViewHeightSizable,
                );
            }
            window.setContentView(Some(&view));
            window.setAcceptsMouseMovedEvents(true);
            window.makeKeyAndOrderFront(None);
            view.set_state_ptr(ptr::null_mut());

            let mut state = match AppState::new(window, view) {
                Ok(state) => state,
                Err(err) => {
                    eprintln!("Failed to init Ghostty: {err}");
                    unsafe {
                        app.terminate(None);
                    }
                    return;
                }
            };

            let state_ptr = &mut *state as *mut AppState;
            state.view.set_state_ptr(state_ptr);

            self.ivars().state.replace(Some(state));
        }

        #[method(applicationShouldTerminateAfterLastWindowClosed:)]
        fn should_terminate_after_last_window_closed(&self, _sender: &NSApplication) -> bool {
            true
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        let this = this.set_ivars(AppDelegateIvars {
            state: RefCell::new(None),
        });
        unsafe { msg_send_id![super(this), init] }
    }
}

fn create_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::Miniaturizable;
    let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(900., 600.));
    let allocated = mtm.alloc::<NSWindow>();
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            allocated,
            frame,
            style,
            NSBackingStoreType::NSBackingStoreBuffered,
            false,
        )
    };
    window.center();
    window.setTitle(&NSString::from_str("Ghostty (libghostty)"));
    window
}

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

unsafe extern "C" fn runtime_wakeup_cb(userdata: *mut c_void) {
    if userdata.is_null() {
        return;
    }
    let state = unsafe { &mut *(userdata as *mut AppState) };
    state.flags.needs_tick.store(true, Ordering::Release);
}

unsafe extern "C" fn runtime_action_cb(
    _app: ghostty_app_t,
    _target: ghostty_target_s,
    _action: ghostty_action_s,
) -> bool {
    false
}

unsafe extern "C" fn runtime_read_clipboard_cb(
    _userdata: *mut c_void,
    _clipboard: ghostty_clipboard_e,
    _request: *mut c_void,
) {
}

unsafe extern "C" fn runtime_confirm_read_clipboard_cb(
    _userdata: *mut c_void,
    _value: *const c_char,
    _request: *mut c_void,
    _request_type: ghostty_clipboard_request_e,
) {
}

unsafe extern "C" fn runtime_write_clipboard_cb(
    _userdata: *mut c_void,
    _value: *const c_char,
    _clipboard: ghostty_clipboard_e,
    _confirm: bool,
) {
}

unsafe extern "C" fn runtime_close_surface_cb(userdata: *mut c_void, _confirm: bool) {
    if userdata.is_null() {
        return;
    }
    let state = unsafe { &mut *(userdata as *mut AppState) };
    state.flags.close_requested.store(true, Ordering::Release);
}

fn main() {
    let mtm: MainThreadMarker = MainThreadMarker::new().expect("main thread");

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let delegate = AppDelegate::new(mtm);
    let object = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(object));

    unsafe { app.run() };
}
