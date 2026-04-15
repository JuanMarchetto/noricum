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

    // package.loaded — map of loaded modules. Pre-populate with
    // every standard library that linit::open_libs already
    // installed onto the globals table, so `require("string")`
    // returns the existing library instead of trying to find a
    // string.lua file.
    let loaded = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    let loaded_name = state.global.new_string(b"loaded", 0);
    state
        .global
        .table_set_shortstr(pkg, loaded_name, TValue::Table(loaded));
    for lib in &[
        "string", "math", "table", "os", "io", "debug",
        "coroutine", "utf8",
    ] {
        let key = state.global.new_string(lib.as_bytes(), 0);
        if let Some(v) = state.global.heap.table_get_shortstr(globals, key) {
            state.global.table_set_shortstr(loaded, key, v);
        }
    }
    // Also expose `_G` and `package` itself for require("_G") /
    // require("package").
    let g_key = state.global.new_string(b"_G", 0);
    state.global.table_set_shortstr(loaded, g_key, TValue::Table(globals));
    let pkg_key = state.global.new_string(b"package", 0);
    state.global.table_set_shortstr(loaded, pkg_key, TValue::Table(pkg));

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

    // package.config — directory separator, template separator,
    // template character, executable character, igmark. Five lines
    // matching the C Lua defaults on POSIX. attrib.lua expects this
    // exact 5-line string.
    let config_key = state.global.new_string(b"config", 0);
    let config_val = state.global.new_string(b"/\n;\n?\n!\n-\n", 0);
    state
        .global
        .table_set_shortstr(pkg, config_key, TValue::ShortString(config_val));

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
    let (loaded_h, _path_str, _cpath_str) = {
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
            Some(TValue::Nil) | None => String::new(),
            _ => {
                let err_h = state
                    .global
                    .new_string(b"'package.path' must be a string", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        };
        let cpath_name = state.global.new_string(b"cpath", 0);
        let cpath_val = state.global.heap.table_get_shortstr(pkg, cpath_name);
        let cpath_str = match cpath_val {
            Some(TValue::ShortString(h)) | Some(TValue::LongString(h)) => {
                String::from_utf8_lossy(&state.global.heap.string(h).bytes)
                    .to_string()
            }
            Some(TValue::Nil) | None => String::new(),
            _ => {
                let err_h = state
                    .global
                    .new_string(b"'package.cpath' must be a string", 0);
                state.raise_error_value(TValue::ShortString(err_h));
                return 0;
            }
        };
        (loaded, path_str, cpath_str)
    };

    // Check cache.
    if let Some(v) = state.global.heap.table_get_shortstr(loaded_h, modname_h) {
        state.current_thread_mut().push(v);
        return 1;
    }

    // Check package.preload[modname] — an explicit loader function
    // registered by the host (or by the module itself for submodules,
    // e.g., fennel.lua stashing sub-modules into preload).
    let preload_h = {
        let pkg_name = state.global.new_string(b"package", 0);
        let globals_opt = state.global.globals_table();
        let pkg = globals_opt
            .and_then(|g| state.global.heap.table_get_shortstr(g, pkg_name))
            .and_then(|v| if let TValue::Table(t) = v { Some(t) } else { None });
        pkg.and_then(|p| {
            let pkey = state.global.new_string(b"preload", 0);
            state.global.heap.table_get_shortstr(p, pkey).and_then(|v| {
                if let TValue::Table(t) = v { Some(t) } else { None }
            })
        })
    };
    if let Some(preload) = preload_h {
        if let Some(loader) = state.global.heap.table_get_shortstr(preload, modname_h) {
            if !matches!(loader, TValue::Nil) {
                // Call loader(modname) and cache the result.
                let base = state.frame_base_index().unwrap_or(0);
                let top_before = state.current_thread().top;
                state.current_thread_mut().push(loader);
                state.current_thread_mut().push(TValue::ShortString(modname_h));
                let _ = state.call_value(top_before, 1, 1);
                // Cache.
                let result = state
                    .current_thread()
                    .stack
                    .get((base + state.get_top() as u32 - 1) as usize)
                    .copied()
                    .unwrap_or(TValue::Nil);
                let cached = if matches!(result, TValue::Nil) {
                    TValue::True
                } else {
                    result
                };
                state.global.table_set_shortstr(loaded_h, modname_h, cached);
                return 1;
            }
        }
    }

    // Walk package.path, substituting `?` for the module name
    // with dots converted to path separators. First match wins.
    let modname_sub = modname_str.replace('.', "/");
    let search_paths: Vec<String> = if _path_str.is_empty() {
        vec![
            format!("./{}.lua", modname_sub),
            format!("./{}/init.lua", modname_sub),
        ]
    } else {
        _path_str
            .split(';')
            .filter(|p| !p.is_empty())
            .map(|p| p.replace('?', &modname_sub))
            .collect()
    };
    let mut content: Option<Vec<u8>> = None;
    let mut tried: Vec<String> = Vec::new();
    for p in &search_paths {
        match std::fs::read(p) {
            Ok(c) => {
                content = Some(c);
                break;
            }
            Err(_) => {
                tried.push(format!("\tno file '{}'", p));
            }
        }
    }
    if content.is_none() {
        // Append cpath attempts to the error trace so the message
        // shape matches C Lua's ll_require output.
        let cpath_candidates: Vec<String> = _cpath_str
            .split(';')
            .filter(|p| !p.is_empty())
            .map(|p| p.replace('?', &modname_sub))
            .collect();
        for p in &cpath_candidates {
            if !std::path::Path::new(p).exists() {
                tried.push(format!("\tno file '{}'", p));
            }
        }
    }
    let content = match content {
        Some(c) => c,
        None => {
            // Match C Lua's ll_require error shape: "module 'X' not
            // found:\n\tno field package.preload['X']\n\tno file ...".
            let mut err = format!("module '{}' not found:", modname_str);
            err.push('\n');
            err.push_str(&format!("\tno field package.preload['{}']", modname_str));
            for line in &tried {
                err.push('\n');
                err.push_str(line);
            }
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
    let source_name = format!("@{}.lua", modname_sub);
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
    // If the input starts with the Lua signature byte, treat it as
    // pre-compiled .luac and route through lundump instead of the
    // source-text parser.
    if src_bytes.first().copied() == Some(0x1B) {
        match crate::lundump::undump(&mut state.global, &src_bytes) {
            Ok(closure) => {
                // Bind _ENV (upvalue 0) to the calling chunk's
                // globals so the bytecode-loaded function can see
                // print, etc.
                if let Some(uv) = state.global.heap.lclosure(closure).upvalues.first().copied() {
                    state.global.heap.upval_mut(uv).state =
                        crate::contract::UpValState::Closed(TValue::Table(globals));
                }
                state.current_thread_mut().push(TValue::LuaClosure(closure));
                return 1;
            }
            Err(e) => {
                state.push_nil();
                state.push_string(&format!("load: bytecode error: {}", e));
                return 2;
            }
        }
    }
    // Wrap parse in catch_unwind since parser panics on syntax
    // errors (pending real error propagation). Silence the default
    // panic hook so the backtrace doesn't leak to stderr.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::lparser::parse_with_env(state, &src_bytes, &chunkname_bytes, globals)
    }));
    std::panic::set_hook(prev_hook);
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
