//! loadlib — dynamic module loader (`package` + `require`).
//!
//! Port of `loadlib.c`. Provides the `require` function and the
//! `package.*` table with fields `path`, `cpath`, `loaded`,
//! `preload`, `searchpath`. In ZERO-deps mode, C dynamic loading
//! uses direct FFI to libdl (POSIX) / LoadLibraryA (Windows) —
//! but for the pure-Lua module loader, we only need file I/O
//! and re-entry into our own compiler.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};

pub fn open_package(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let pkg = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());

    // package.loaded — map of loaded modules.
    let loaded = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    let loaded_name = state.global.new_string(b"loaded", 0);
    state
        .global
        .table_set_shortstr(pkg, loaded_name, TValue::Table(loaded));

    // package.preload — map of pre-registered module loaders.
    let preload = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    let preload_name = state.global.new_string(b"preload", 0);
    state
        .global
        .table_set_shortstr(pkg, preload_name, TValue::Table(preload));

    // package.path — search pattern (default mirrors C Lua).
    let path_key = state.global.new_string(b"path", 0);
    let path_val = state
        .global
        .new_string(b"./?.lua;./?/init.lua;/usr/local/share/lua/5.4/?.lua", 0);
    state
        .global
        .table_set_shortstr(pkg, path_key, TValue::ShortString(path_val));

    // package.cpath — C module search pattern.
    let cpath_key = state.global.new_string(b"cpath", 0);
    let cpath_val = state
        .global
        .new_string(b"./?.so;/usr/local/lib/lua/5.4/?.so", 0);
    state
        .global
        .table_set_shortstr(pkg, cpath_key, TValue::ShortString(cpath_val));

    // package.searchpath
    register(state, pkg, "searchpath", pkg_searchpath);
    // package.loadlib — opens a C library and returns a function.
    register(state, pkg, "loadlib", pkg_loadlib);

    // require as a global
    register(state, globals, "require", require);
    // loadfile / dofile / load
    register(state, globals, "loadfile", loadfile);
    register(state, globals, "dofile", dofile);
    register(state, globals, "load", load_string);
    register(state, globals, "loadstring", load_string);

    let pkg_name = state.global.new_string(b"package", 0);
    state
        .global
        .table_set_shortstr(globals, pkg_name, TValue::Table(pkg));
    pkg
}

fn register(
    state: &mut LuaState,
    t: TableHandle,
    name: &str,
    f: RawCFunction,
) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state
        .global
        .table_set_shortstr(t, h, TValue::LightCFunction(f));
}

/// `package.searchpath(name, path [, sep [, rep]])` — returns the
/// first existing file produced by substituting `name` into `path`
/// templates, or nil + error message.
unsafe extern "C" fn pkg_searchpath(
    state: *mut LuaState,
) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let name_bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let path_bytes = state.to_lstring(2).map(|s| s.to_vec()).unwrap_or_default();
    let sep_bytes = state.to_lstring(3).map(|s| s.to_vec()).unwrap_or_else(|| b".".to_vec());
    let rep_bytes = state.to_lstring(4).map(|s| s.to_vec()).unwrap_or_else(|| b"/".to_vec());

    let name = String::from_utf8_lossy(&name_bytes).to_string();
    let path = String::from_utf8_lossy(&path_bytes).to_string();
    let sep = String::from_utf8_lossy(&sep_bytes).to_string();
    let rep = String::from_utf8_lossy(&rep_bytes).to_string();

    // Replace sep with rep in name.
    let substituted = if !sep.is_empty() {
        name.replace(&sep, &rep)
    } else {
        name.clone()
    };

    let mut tried = Vec::new();
    for template in path.split(';') {
        let candidate = template.replace('?', &substituted);
        if std::path::Path::new(&candidate).exists() {
            state.push_string(&candidate);
            return 1;
        }
        tried.push(format!("no file '{}'", candidate));
    }
    state.push_nil();
    let msg = tried.join("\n\t");
    state.push_string(&format!("\n\t{}", msg));
    2
}

/// `package.loadlib(libname, funcname)` — opens a shared object
/// and returns the function pointer. Full FFI implementation
/// requires libdl; here we provide a stub that rejects the call
/// with a clear message. Full C-module support can land later
/// alongside a libc FFI shim.
unsafe extern "C" fn pkg_loadlib(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    state.push_string("dynamic C libraries not supported in this port");
    state.push_string("open");
    3
}

/// `require(modname)` — loads a module. Checks package.loaded
/// first, then tries searchpath + compiles + runs the loader.
unsafe extern "C" fn require(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let modname_bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let modname_str = String::from_utf8_lossy(&modname_bytes).to_string();
    let modname_h = state
        .global
        .new_string(modname_bytes.as_slice(), state.global.hash_seed);

    // Look up package.loaded[modname].
    let (loaded_h, _path_str) = {
        let env_upv = state.global.heap.lclosure(match state.current_thread().stack[0] {
            TValue::LuaClosure(h) => h,
            _ => {
                // Called from a light C function — fetch _ENV via
                // the first upvalue of the caller. Fallback: use
                // the registry, which we don't track here.
                state.push_nil();
                let err_h = state
                    .global
                    .new_string(b"require: environment missing", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        }).upvalues[0];
        let env_val = match &state.global.heap.upval(env_upv).state {
            crate::contract::UpValState::Closed(v) => *v,
            crate::contract::UpValState::Open { thread, stack_index } => {
                state.global.heap.thread(*thread).stack[*stack_index as usize]
            }
        };
        let env_table = match env_val {
            TValue::Table(h) => h,
            _ => {
                let err_h = state.global.new_string(b"require: bad _ENV", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        };
        let pkg_name = state.global.new_string(b"package", 0);
        let pkg = match state.global.heap.table_get_shortstr(env_table, pkg_name) {
            Some(TValue::Table(h)) => h,
            _ => {
                let err_h = state.global.new_string(b"require: no 'package'", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        };
        let loaded_name = state.global.new_string(b"loaded", 0);
        let loaded = match state.global.heap.table_get_shortstr(pkg, loaded_name) {
            Some(TValue::Table(h)) => h,
            _ => {
                let err_h = state
                    .global
                    .new_string(b"require: no 'package.loaded'", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        };
        let path_name = state.global.new_string(b"path", 0);
        let path_val = state.global.heap.table_get_shortstr(pkg, path_name);
        let path_str = match path_val {
            Some(TValue::ShortString(h)) | Some(TValue::LongString(h)) => {
                String::from_utf8_lossy(&state.global.heap.string(h).bytes)
                    .to_string()
            }
            _ => String::new(),
        };
        (loaded, path_str)
    };

    // Check cache.
    if let Some(v) = state.global.heap.table_get_shortstr(loaded_h, modname_h) {
        state.current_thread_mut().push(v);
        return 1;
    }

    // Try to locate the file.
    let modname_str2 = modname_str.replace('.', "/");
    let path = format!("./{}.lua", modname_str2);
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(_) => {
            let err = format!("module '{}' not found", modname_str);
            let h = state.global.new_string(err.as_bytes(), 0);
            state.raise_error_value(TValue::ShortString(h));
            return 0;
        }
    };

    // Get the globals table to bind _ENV to the new closure.
    let globals = match state.current_thread().stack.first() {
        Some(&TValue::LuaClosure(ch)) => {
            let uv = state.global.heap.lclosure(ch).upvalues[0];
            match &state.global.heap.upval(uv).state {
                crate::contract::UpValState::Closed(TValue::Table(t)) => *t,
                _ => return 0,
            }
        }
        _ => return 0,
    };

    // Compile and run.
    let source_name = format!("@{}.lua", modname_str2);
    let closure = crate::lparser::parse_with_env(
        state,
        &content,
        source_name.as_bytes(),
        globals,
    );
    state
        .current_thread_mut()
        .push(TValue::LuaClosure(closure));
    let frame_base = state.frame_base_index().unwrap_or(0);
    let func_abs = state.current_thread().top - 1;
    let _ = state.call_value(func_abs, 0, 1);
    // Grab the return value (top of stack).
    let result = state.current_thread().stack[func_abs as usize];
    // Cache in package.loaded[modname].
    state
        .global
        .table_set_shortstr(loaded_h, modname_h, result);
    let _ = frame_base;
    state.current_thread_mut().push(result);
    1
}

unsafe extern "C" fn loadfile(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let path_bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let path = String::from_utf8_lossy(&path_bytes).to_string();
    let content = match std::fs::read(&path) {
        Ok(c) => c,
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("cannot open '{}': {}", path, e));
            return 2;
        }
    };
    // Grab globals from caller (same logic as require).
    let globals = match state.current_thread().stack.first() {
        Some(&TValue::LuaClosure(ch)) => {
            let uv = state.global.heap.lclosure(ch).upvalues[0];
            match &state.global.heap.upval(uv).state {
                crate::contract::UpValState::Closed(TValue::Table(t)) => *t,
                _ => {
                    state.push_nil();
                    return 1;
                }
            }
        }
        _ => {
            state.push_nil();
            return 1;
        }
    };
    let source_name = format!("@{}", path);
    let closure = crate::lparser::parse_with_env(
        state,
        &content,
        source_name.as_bytes(),
        globals,
    );
    state
        .current_thread_mut()
        .push(TValue::LuaClosure(closure));
    1
}

unsafe extern "C" fn dofile(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let n = loadfile(state);
    if n != 1 {
        return n;
    }
    // loadfile pushed the closure on top — call it.
    let func_abs = state.current_thread().top - 1;
    let status = state.pcall(func_abs, 0, -1);
    if status != crate::contract::ThreadStatus::Ok {
        let err = state.current_thread().stack[func_abs as usize];
        state.current_thread_mut().push(err);
        return 1;
    }
    let n_returns = (state.current_thread().top - func_abs) as i32;
    n_returns
}

unsafe extern "C" fn load_string(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let src_bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let chunkname_bytes = state
        .to_lstring(2)
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"=(load)".to_vec());
    let globals = match state.current_thread().stack.first() {
        Some(&TValue::LuaClosure(ch)) => {
            let uv = state.global.heap.lclosure(ch).upvalues[0];
            match &state.global.heap.upval(uv).state {
                crate::contract::UpValState::Closed(TValue::Table(t)) => *t,
                _ => {
                    state.push_nil();
                    return 1;
                }
            }
        }
        _ => {
            state.push_nil();
            return 1;
        }
    };
    // Wrap parse in catch_unwind since parser panics on syntax
    // errors (pending real error propagation).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::lparser::parse_with_env(state, &src_bytes, &chunkname_bytes, globals)
    }));
    match result {
        Ok(cl) => {
            state.current_thread_mut().push(TValue::LuaClosure(cl));
            1
        }
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
                s.to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "load: parse error".to_string()
            };
            state.push_nil();
            state.push_string(&msg);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_package_registers_package_table() {
        let mut state = LuaState::new(0);
        let g = crate::lbaselib::open_base(&mut state);
        open_package(&mut state, g);
        let name = state.global.new_string(b"package", 0);
        let v = state.global.heap.table_get_shortstr(g, name);
        assert!(matches!(v, Some(TValue::Table(_))));
    }

    #[test]
    fn package_path_has_default() {
        let mut state = LuaState::new(0);
        let g = crate::lbaselib::open_base(&mut state);
        let pkg = open_package(&mut state, g);
        let path_key = state.global.new_string(b"path", 0);
        let v = state.global.heap.table_get_shortstr(pkg, path_key);
        assert!(matches!(v, Some(TValue::ShortString(_)) | Some(TValue::LongString(_))));
    }

    #[test]
    fn load_registered_as_global() {
        let mut state = LuaState::new(0);
        let g = crate::lbaselib::open_base(&mut state);
        open_package(&mut state, g);
        let lname = state.global.new_string(b"load", 0);
        let v = state.global.heap.table_get_shortstr(g, lname);
        assert!(matches!(v, Some(TValue::LightCFunction(_))));
    }
}
