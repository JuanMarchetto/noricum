//! Integration tests: compile and run real Lua programs end to end.
//!
//! Each test parses a Lua source string (or file in tests/lua_programs),
//! runs it under the full stdlib, and captures the print buffer to
//! assert on expected output.

use spike::contract::{LuaState, TValue};
use spike::lbaselib::{reset_print_buffer, take_print_buffer};
use spike::linit::open_libs;
use spike::lparser::parse_with_env;
use std::sync::Mutex;

// The print buffer is a process-global static. Serialise parallel
// tests so they don't stomp each other.
static LOCK: Mutex<()> = Mutex::new(());

fn run(source: &str) -> Vec<String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = LuaState::new(0);
    let globals = open_libs(&mut state);
    reset_print_buffer();
    let closure = parse_with_env(
        &mut state,
        source.as_bytes(),
        b"=(test)",
        globals,
    );
    state
        .current_thread_mut()
        .push(TValue::LuaClosure(closure));
    state.call_value(0, 0, 0).unwrap();
    take_print_buffer()
}

#[test]
fn hello_world() {
    let out = run("print('hello, world')");
    assert_eq!(out, vec!["hello, world".to_string()]);
}

#[test]
fn arithmetic_and_concat() {
    let out = run("print(1 + 2 .. ' is ' .. (3 * 4))");
    assert_eq!(out, vec!["3 is 12".to_string()]);
}

#[test]
fn recursive_factorial() {
    let out = run(r#"
        local function fact(n)
          if n <= 1 then return 1 else return n * fact(n - 1) end
        end
        print(fact(5), fact(10))
    "#);
    assert_eq!(out, vec!["120\t3628800".to_string()]);
}

#[test]
fn closures_capture_upvalues() {
    let out = run(r#"
        local function counter()
          local n = 0
          return function() n = n + 1; return n end
        end
        local c = counter()
        print(c(), c(), c())
    "#);
    assert_eq!(out, vec!["1\t2\t3".to_string()]);
}

#[test]
fn ipairs_iteration() {
    let out = run(r#"
        for k, v in ipairs({"a","b","c"}) do print(k, v) end
    "#);
    assert_eq!(out, vec![
        "1\ta".to_string(),
        "2\tb".to_string(),
        "3\tc".to_string(),
    ]);
}

#[test]
fn while_with_break() {
    let out = run(r#"
        local i = 0
        while true do
          i = i + 1
          if i > 3 then break end
          print(i)
        end
        print("done", i)
    "#);
    assert_eq!(out, vec![
        "1".to_string(),
        "2".to_string(),
        "3".to_string(),
        "done\t4".to_string(),
    ]);
}

#[test]
fn numeric_for_sum() {
    let out = run(r#"
        local s = 0
        for i = 1, 100 do s = s + i end
        print(s)
    "#);
    assert_eq!(out, vec!["5050".to_string()]);
}

#[test]
fn pcall_catches_error() {
    let out = run(r#"
        local ok, err = pcall(function() error("boom") end)
        print(ok, err)
    "#);
    assert_eq!(out, vec!["false\tboom".to_string()]);
}

#[test]
fn oo_via_metatables() {
    let out = run(r#"
        local Shape = {}
        Shape.__index = Shape
        function Shape.new(name) return setmetatable({name=name}, Shape) end
        function Shape:greet() return "hi, " .. self.name end
        local s = Shape.new("world")
        print(s:greet())
    "#);
    assert_eq!(out, vec!["hi, world".to_string()]);
}

#[test]
fn string_methods_via_colon() {
    let out = run(r#"
        print(("hello"):upper(), ("hello"):len(), ("hello"):sub(2,4))
    "#);
    assert_eq!(out, vec!["HELLO\t5\tell".to_string()]);
}

#[test]
fn math_functions() {
    let out = run(r#"
        print(math.floor(3.7), math.ceil(3.2), math.sqrt(9), math.max(1,5,3))
    "#);
    // math.sqrt always returns a float, so print formats it as "3.0"
    assert_eq!(out, vec!["3\t4\t3.0\t5".to_string()]);
}

#[test]
fn table_sort_and_concat() {
    let out = run(r#"
        local t = {3, 1, 4, 1, 5, 9, 2, 6}
        table.sort(t)
        print(table.concat(t, ","))
    "#);
    assert_eq!(out, vec!["1,1,2,3,4,5,6,9".to_string()]);
}

#[test]
fn multiple_returns() {
    let out = run(r#"
        local function two() return "a", "b" end
        local x, y = two()
        print(x, y)
    "#);
    assert_eq!(out, vec!["a\tb".to_string()]);
}

#[test]
fn table_constructors() {
    let out = run(r#"
        local t = {x=1, y=2, [3]="three", "pos1", "pos2"}
        print(t.x, t.y, t[3], t[1], t[2])
    "#);
    assert_eq!(out, vec!["1\t2\tthree\tpos1\tpos2".to_string()]);
}

#[test]
fn pairs_over_table() {
    let mut out = run(r#"
        local t = {a=1, b=2}
        local seen = 0
        for k, v in pairs(t) do seen = seen + v end
        print(seen)
    "#);
    assert_eq!(out.pop(), Some("3".to_string()));
}
