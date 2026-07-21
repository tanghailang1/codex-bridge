//! Stable C ABI for the ChatGPT login bridge.

use crate::ABI_VERSION;
use crate::BridgeError;
use crate::LoginConfig;
use crate::LoginSession;
use crate::export_credentials;
use crate::list_models;
use crate::query_account_status;
use crate::refresh_credentials;
use crate::responses_request_schema;
use crate::start_login;
use std::cell::RefCell;
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::ptr;

const OK: i32 = 0;
const ERROR: i32 = 1;
const PANIC: i32 = 2;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").expect("empty CString"));
}

fn set_last_error(error: impl std::fmt::Display) {
    let message = error.to_string().replace('\0', "\\0");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(message).expect("NUL bytes replaced");
    });
}

fn ffi_call(function: impl FnOnce() -> Result<(), BridgeError>) -> i32 {
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(Ok(())) => OK,
        Ok(Err(error)) => {
            set_last_error(error);
            ERROR
        }
        Err(_) => {
            set_last_error("panic crossed the Codex bridge boundary");
            PANIC
        }
    }
}

unsafe fn config_from_parts(data: *const u8, len: usize) -> Result<LoginConfig, BridgeError> {
    if data.is_null() {
        if len == 0 {
            return Ok(LoginConfig::default());
        }
        return Err(BridgeError::InvalidArgument(
            "config is null but len is non-zero",
        ));
    }
    // SAFETY: validated non-null above; caller promises the region is readable for len bytes.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    LoginConfig::from_json(bytes).map_err(BridgeError::InvalidConfig)
}

unsafe fn bytes_from_parts<'a>(
    data: *const u8,
    len: usize,
    name: &'static str,
) -> Result<&'a [u8], BridgeError> {
    if data.is_null() {
        if len == 0 {
            return Ok(&[]);
        }
        return Err(BridgeError::InvalidArgument(name));
    }
    // SAFETY: the caller promises the region is readable for len bytes.
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}

fn write_json<T: serde::Serialize>(value: &T, output: *mut *mut c_char) -> Result<(), BridgeError> {
    if output.is_null() {
        return Err(BridgeError::InvalidArgument("output JSON pointer is null"));
    }
    let json = serde_json::to_string(value).map_err(BridgeError::InvalidConfig)?;
    let string = CString::new(json)
        .map_err(|_| BridgeError::InvalidArgument("serialized JSON contains NUL"))?;
    // SAFETY: output was checked above and points to caller-owned writable storage.
    unsafe { *output = string.into_raw() };
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn codex_bridge_login_abi_version() -> u32 {
    ABI_VERSION
}

/// Return the exact NUL-terminated Codex submodule commit compiled into the library.
#[unsafe(no_mangle)]
pub extern "C" fn codex_bridge_codex_commit() -> *const c_char {
    concat!(env!("CODEX_BRIDGE_CODEX_COMMIT"), "\0")
        .as_ptr()
        .cast()
}

/// Returns a thread-local error string valid until the next bridge call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn codex_bridge_login_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Start a browser login and return both an opaque session and JSON start information.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_login_start(
    config_json: *const u8,
    config_len: usize,
    out_session: *mut *mut LoginSession,
    out_start_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        if out_session.is_null() {
            return Err(BridgeError::InvalidArgument("out_session is null"));
        }
        // SAFETY: validated writable pointers are supplied by the caller.
        unsafe {
            *out_session = ptr::null_mut();
            if !out_start_json.is_null() {
                *out_start_json = ptr::null_mut();
            }
        }

        // SAFETY: pointer/length validation is centralized in config_from_parts.
        let config = unsafe { config_from_parts(config_json, config_len)? };
        let session = start_login(config)?;
        write_json(session.start_info(), out_start_json)?;
        // SAFETY: out_session was validated above; ownership transfers to the caller.
        unsafe { *out_session = Box::into_raw(Box::new(session)) };
        Ok(())
    })
}

/// Block until login completes and return the full persisted credential payload as JSON.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_login_wait(
    session: *mut LoginSession,
    out_credentials_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let session =
            unsafe { session.as_ref() }.ok_or(BridgeError::InvalidArgument("session is null"))?;
        let credentials = session.wait()?;
        write_json(&credentials, out_credentials_json)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_login_cancel(session: *mut LoginSession) -> i32 {
    ffi_call(|| {
        let session =
            unsafe { session.as_ref() }.ok_or(BridgeError::InvalidArgument("session is null"))?;
        session.cancel();
        Ok(())
    })
}

/// Load already persisted credentials without starting a new login.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_login_credentials(
    config_json: *const u8,
    config_len: usize,
    out_credentials_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let config = unsafe { config_from_parts(config_json, config_len)? };
        let credentials = export_credentials(&config)?;
        write_json(&credentials, out_credentials_json)
    })
}

/// Query current account usage and rate-limit details using a serialized `auth.json` payload.
/// The returned JSON is owned by the caller and must be released with string_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_status_query(
    auth_json: *const u8,
    auth_len: usize,
    base_url: *const u8,
    base_url_len: usize,
    out_status_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let auth = unsafe { bytes_from_parts(auth_json, auth_len, "auth JSON is null")? };
        let base_url = unsafe { bytes_from_parts(base_url, base_url_len, "base URL is null")? };
        let base_url = if base_url.is_empty() {
            None
        } else {
            Some(
                std::str::from_utf8(base_url)
                    .map_err(|_| BridgeError::InvalidArgument("base URL is not UTF-8"))?,
            )
        };
        let status = query_account_status(auth, base_url).map_err(BridgeError::Status)?;
        write_json(&status, out_status_json)
    })
}

/// Return the JSON Schema for the Codex Responses request body, with `default` values for the
/// baseline fields Codex fills in. Static metadata: takes no credentials, makes no network call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_responses_schema(out_schema_json: *mut *mut c_char) -> i32 {
    ffi_call(|| write_json(&responses_request_schema(), out_schema_json))
}

/// List the models available to one stored credential, mirroring the CLI `/model` picker.
///
/// The returned JSON is `{"models": [...]}`; each entry carries its own supported reasoning
/// efforts and service tiers (the CLI's 推理强度 / 速度 menus).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_models_list(
    auth_json: *const u8,
    auth_len: usize,
    out_models_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let auth = unsafe { bytes_from_parts(auth_json, auth_len, "auth JSON is null")? };
        let models = list_models(auth)?;
        write_json(&models, out_models_json)
    })
}

/// Refresh one stored `auth.json` payload and return the outcome as JSON.
///
/// The call returns OK for both successful and failed refreshes; inspect the returned JSON
/// (`refreshed` / `permanent` / `message` / `auth`) to decide whether a new login is required.
/// Only malformed arguments produce a non-zero return code.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_token_refresh(
    auth_json: *const u8,
    auth_len: usize,
    out_result_json: *mut *mut c_char,
) -> i32 {
    ffi_call(|| {
        let auth = unsafe { bytes_from_parts(auth_json, auth_len, "auth JSON is null")? };
        let result = refresh_credentials(auth)?;
        write_json(&result, out_result_json)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_login_session_free(session: *mut LoginSession) {
    if !session.is_null() {
        // SAFETY: pointer must have been returned by codex_bridge_login_start exactly once.
        drop(unsafe { Box::from_raw(session) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn codex_bridge_string_free(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: pointer must have been returned by this library exactly once.
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Helper used by ABI smoke tests and language bindings.
pub unsafe fn owned_string_from_raw(value: *mut c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: test/helper caller follows the same ownership contract as the public C API.
    let result = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    unsafe { codex_bridge_string_free(value) };
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_is_one() {
        assert_eq!(codex_bridge_login_abi_version(), 1);
    }

    #[test]
    fn invalid_json_sets_error() {
        let mut session = ptr::null_mut();
        let mut output = ptr::null_mut();
        let config = b"{";
        let code = unsafe {
            codex_bridge_login_start(config.as_ptr(), config.len(), &mut session, &mut output)
        };
        assert_eq!(code, ERROR);
        let error = unsafe { CStr::from_ptr(codex_bridge_login_last_error()) }.to_string_lossy();
        assert!(error.contains("invalid login configuration"));
    }
}
