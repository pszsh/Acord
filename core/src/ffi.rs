use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use crate::document::AcordDoc;
use crate::eval;
use crate::highlight;
use crate::persist;

fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() { return None; }
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

fn str_to_cstr(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_new() -> *mut AcordDoc {
    Box::into_raw(Box::new(AcordDoc::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_free(doc: *mut AcordDoc) {
    if doc.is_null() { return; }
    unsafe { drop(Box::from_raw(doc)); }
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_set_text(doc: *mut AcordDoc, text: *const c_char) {
    let doc = match unsafe { doc.as_mut() } {
        Some(d) => d,
        None => return,
    };
    let text = match cstr_to_str(text) {
        Some(s) => s,
        None => return,
    };
    doc.set_text(text);
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_get_text(doc: *const AcordDoc) -> *mut c_char {
    let doc = match unsafe { doc.as_ref() } {
        Some(d) => d,
        None => return std::ptr::null_mut(),
    };
    str_to_cstr(&doc.text)
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_evaluate(doc: *mut AcordDoc) -> *mut c_char {
    let doc = match unsafe { doc.as_mut() } {
        Some(d) => d,
        None => return str_to_cstr("[]"),
    };
    let result = doc.evaluate();
    let json = serde_json::to_string(&result).unwrap_or_else(|_| "[]".into());
    str_to_cstr(&json)
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_eval_line(text: *const c_char) -> *mut c_char {
    let text = match cstr_to_str(text) {
        Some(s) => s,
        None => return str_to_cstr(""),
    };
    match eval::evaluate_line(text) {
        Ok(result) => str_to_cstr(&result),
        Err(e) => str_to_cstr(&format!("error: {}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_save(doc: *const AcordDoc, path: *const c_char) -> bool {
    let doc = match unsafe { doc.as_ref() } {
        Some(d) => d,
        None => return false,
    };
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return false,
    };
    persist::save_to_file(&doc.text, Path::new(path)).is_ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_doc_load(path: *const c_char) -> *mut AcordDoc {
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match persist::load_from_file(Path::new(path)) {
        Ok(text) => {
            let mut doc = AcordDoc::new();
            doc.set_text(&text);
            Box::into_raw(Box::new(doc))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_cache_save(doc: *const AcordDoc) -> *mut c_char {
    let doc = match unsafe { doc.as_ref() } {
        Some(d) => d,
        None => return std::ptr::null_mut(),
    };
    let uuid = doc.uuid.clone();
    match persist::cache_save(&uuid, &doc.text) {
        Ok(_) => str_to_cstr(&uuid),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_cache_load(uuid: *const c_char) -> *mut AcordDoc {
    let uuid = match cstr_to_str(uuid) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match persist::cache_load(uuid) {
        Ok(text) => {
            let mut doc = AcordDoc::with_uuid(uuid.to_string());
            doc.set_text(&text);
            Box::into_raw(Box::new(doc))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_list_notes() -> *mut c_char {
    let notes = persist::list_notes();
    let json = serde_json::to_string(&notes).unwrap_or_else(|_| "[]".into());
    str_to_cstr(&json)
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_highlight(source: *const c_char, lang: *const c_char) -> *mut c_char {
    let source = match cstr_to_str(source) {
        Some(s) => s,
        None => return str_to_cstr("[]"),
    };
    let lang = match cstr_to_str(lang) {
        Some(s) => s,
        None => return str_to_cstr("[]"),
    };
    let spans = highlight::highlight_source(source, lang);
    let json = serde_json::to_string(&spans).unwrap_or_else(|_| "[]".into());
    str_to_cstr(&json)
}

#[unsafe(no_mangle)]
pub extern "C" fn acord_free_string(s: *mut c_char) {
    if s.is_null() { return; }
    unsafe { drop(CString::from_raw(s)); }
}
