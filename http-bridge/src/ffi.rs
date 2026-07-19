//! Stable C ABI for the HTTP Responses bridge.

use crate::CODEX_COMMIT;
use crate::HTTP_ABI_VERSION;
use crate::HttpBridge;
use crate::HttpBridgeConfig;
use crate::HttpBridgeError;
use crate::HttpRequest;
use crate::HttpRequestHandle;
use crate::engine::NextEvent;
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::ptr;
use std::time::Duration;

pub const OK: i32 = 0;
pub const ERROR: i32 = 1;
pub const PANIC: i32 = 2;
pub const TIMEOUT: i32 = 3;
pub const CLOSED: i32 = 4;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").expect("empty CString"));
}

fn set_last_error(error: impl std::fmt::Display) {
    let message = error.to_string().replace('\0', "\\0");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(message).expect("NUL bytes replaced");
    });
}

fn ffi_call(function: impl FnOnce() -> Result<i32, HttpBridgeError>) -> i32 {
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => {
            set_last_error(error);
            ERROR
        }
        Err(_) => {
            set_last_error("panic crossed the Codex HTTP bridge boundary");
            PANIC
        }
    }
}

unsafe fn bytes_from_parts<'a>(
    data: *const u8,
    len: usize,
    name: &'static str,
) -> Result<&'a [u8], HttpBridgeError> {
    if data.is_null() {
        if len == 0 {
            return Ok(&[]);
        }
        return Err(HttpBridgeError::InvalidArgument(name));
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}

fn write_string(value: String, output: *mut *mut c_char) -> Result<(), HttpBridgeError> {
    if output.is_null() {
        return Err(HttpBridgeError::InvalidArgument("output string is null"));
    }
    let value =
        CString::new(value).map_err(|_| HttpBridgeError::InvalidArgument("output contains NUL"))?;
    unsafe { *output = value.into_raw() };
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn codex_http_bridge_abi_version() -> u32 {
    HTTP_ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn codex_http_bridge_codex_commit() -> *const c_char {
    let _ = CODEX_COMMIT;
    concat!(env!("CODEX_HTTP_BRIDGE_CODEX_COMMIT"), "\0")
        .as_ptr()
        .cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn codex_http_bridge_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_create(
    config_json: *const u8,
    config_len: usize,
    out_bridge: *mut *mut HttpBridge,
) -> i32 {
    ffi_call(|| {
        if out_bridge.is_null() {
            return Err(HttpBridgeError::InvalidArgument("out_bridge is null"));
        }
        unsafe { *out_bridge = ptr::null_mut() };
        let bytes = unsafe { bytes_from_parts(config_json, config_len, "invalid config buffer")? };
        let config = HttpBridgeConfig::from_json(bytes)?;
        let bridge = HttpBridge::new(config)?;
        unsafe { *out_bridge = Box::into_raw(Box::new(bridge)) };
        Ok(OK)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_request_start(
    bridge: *mut HttpBridge,
    request_json: *const u8,
    request_len: usize,
    out_request: *mut *mut HttpRequestHandle,
) -> i32 {
    ffi_call(|| {
        let bridge =
            unsafe { bridge.as_ref() }.ok_or(HttpBridgeError::InvalidArgument("bridge is null"))?;
        if out_request.is_null() {
            return Err(HttpBridgeError::InvalidArgument("out_request is null"));
        }
        unsafe { *out_request = ptr::null_mut() };
        let bytes =
            unsafe { bytes_from_parts(request_json, request_len, "invalid request buffer")? };
        let request = HttpRequest::from_json(bytes)?;
        let handle = bridge.start(request);
        unsafe { *out_request = Box::into_raw(Box::new(handle)) };
        Ok(OK)
    })
}

/// Return OK with an allocated JSON event, TIMEOUT, CLOSED, ERROR or PANIC.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_event_next(
    request: *mut HttpRequestHandle,
    timeout_ms: i64,
    out_event_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let request = unsafe { request.as_ref() }
            .ok_or(HttpBridgeError::InvalidArgument("request is null"))?;
        if out_event_json.is_null() {
            return Err(HttpBridgeError::InvalidArgument("out_event_json is null"));
        }
        unsafe { *out_event_json = ptr::null_mut() };
        let timeout = (timeout_ms >= 0).then(|| Duration::from_millis(timeout_ms as u64));
        match request.next_event(timeout)? {
            NextEvent::Event(event) => {
                write_string(event, out_event_json)?;
                Ok(OK)
            }
            NextEvent::Timeout => Ok(TIMEOUT),
            NextEvent::Closed => Ok(CLOSED),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_request_cancel(request: *mut HttpRequestHandle) -> i32 {
    ffi_call(|| {
        let request = unsafe { request.as_ref() }
            .ok_or(HttpBridgeError::InvalidArgument("request is null"))?;
        request.cancel();
        Ok(OK)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_request_free(request: *mut HttpRequestHandle) {
    if !request.is_null() {
        drop(unsafe { Box::from_raw(request) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_free(bridge: *mut HttpBridge) {
    if !bridge.is_null() {
        drop(unsafe { Box::from_raw(bridge) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_http_bridge_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}
