//! Differential test for `lobject::raw_arith`.
//!
//! Invokes the real `luaO_rawarith` through an ephemeral `lua_State`
//! under `lua_pcall` (so the div-by-zero `luaG_runerror` longjmp is
//! caught) and compares its result with the Rust port across every
//! op × several representative operand pairs.

use spike::contract::{LuaError, TValue};
use spike::lobject::{raw_arith, ArithOp};
use spike::wr_lobject_rawarith;

/// Call the C oracle with a pair of operands and decode its result.
fn oracle(op: ArithOp, p1: TValue, p2: TValue) -> OracleResult {
    let (t1, i1, f1) = encode(p1);
    let (t2, i2, f2) = encode(p2);
    let mut out_tag: i32 = -1;
    let mut out_int: i64 = 0;
    let mut out_float: f64 = 0.0;
    let rc = unsafe {
        wr_lobject_rawarith(
            op as i32,
            t1,
            i1,
            f1,
            t2,
            i2,
            f2,
            &mut out_tag,
            &mut out_int,
            &mut out_float,
        )
    };
    match rc {
        1 => match out_tag {
            0 => OracleResult::IntOk(out_int),
            1 => OracleResult::FloatOk(out_float),
            _ => panic!("bad oracle out_tag = {out_tag}"),
        },
        0 => OracleResult::NonNumeric,
        -1 => OracleResult::RuntimeError,
        -2 => panic!("oracle state init failed"),
        other => panic!("unexpected oracle rc = {other}"),
    }
}

fn encode(v: TValue) -> (i32, i64, f64) {
    match v {
        TValue::Integer(i) => (0, i, 0.0),
        TValue::Number(f) => (1, 0, f),
        _ => panic!("arith diff test only supports numeric operands"),
    }
}

#[derive(Debug, PartialEq)]
enum OracleResult {
    IntOk(i64),
    FloatOk(f64),
    NonNumeric,
    RuntimeError,
}

fn rust_result(op: ArithOp, p1: TValue, p2: TValue) -> OracleResult {
    match raw_arith(op, &p1, &p2) {
        Ok(Some(TValue::Integer(i))) => OracleResult::IntOk(i),
        Ok(Some(TValue::Number(f))) => OracleResult::FloatOk(f),
        Ok(Some(_)) => panic!("arith produced a non-numeric TValue"),
        Ok(None) => OracleResult::NonNumeric,
        Err(LuaError::Runtime(_)) => OracleResult::RuntimeError,
        Err(e) => panic!("arith produced unexpected error: {e:?}"),
    }
}

fn assert_agree(op: ArithOp, p1: TValue, p2: TValue) {
    let r = rust_result(op, p1, p2);
    let c = oracle(op, p1, p2);
    match (&r, &c) {
        (OracleResult::FloatOk(a), OracleResult::FloatOk(b)) => {
            if a.is_nan() && b.is_nan() {
                return;
            }
            assert_eq!(a.to_bits(), b.to_bits(), "{op:?}({p1:?}, {p2:?})");
        }
        _ => assert_eq!(r, c, "{op:?}({p1:?}, {p2:?})"),
    }
}

const INT_SAMPLES: &[i64] = &[
    0,
    1,
    -1,
    2,
    -2,
    3,
    -3,
    7,
    -7,
    255,
    -255,
    1_000_000,
    -1_000_000,
    i64::MAX / 2,
    i64::MIN / 2,
    i64::MAX,
    i64::MIN,
];

const FLOAT_SAMPLES: &[f64] = &[
    0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    -2.0,
    3.125, // deliberately not π so clippy::approx_constant stays off
    -3.125,
    10.0,
    -10.0,
    1e9,
    -1e9,
    1e15,
    -1e15,
];

// -- integer × integer ---------------------------------------------------

#[test]
fn add_matches_on_integer_matrix() {
    for &a in INT_SAMPLES {
        for &b in INT_SAMPLES {
            assert_agree(ArithOp::Add, TValue::Integer(a), TValue::Integer(b));
        }
    }
}

#[test]
fn sub_matches_on_integer_matrix() {
    for &a in INT_SAMPLES {
        for &b in INT_SAMPLES {
            assert_agree(ArithOp::Sub, TValue::Integer(a), TValue::Integer(b));
        }
    }
}

#[test]
fn mul_matches_on_integer_matrix() {
    for &a in INT_SAMPLES {
        for &b in INT_SAMPLES {
            assert_agree(ArithOp::Mul, TValue::Integer(a), TValue::Integer(b));
        }
    }
}

#[test]
fn idiv_matches_on_integer_matrix() {
    for &a in INT_SAMPLES {
        for &b in INT_SAMPLES {
            assert_agree(ArithOp::IDiv, TValue::Integer(a), TValue::Integer(b));
        }
    }
}

#[test]
fn mod_matches_on_integer_matrix() {
    for &a in INT_SAMPLES {
        for &b in INT_SAMPLES {
            assert_agree(ArithOp::Mod, TValue::Integer(a), TValue::Integer(b));
        }
    }
}

#[test]
fn bitwise_matches_on_integer_matrix() {
    for op in [ArithOp::BAnd, ArithOp::BOr, ArithOp::BXor] {
        for &a in INT_SAMPLES {
            for &b in INT_SAMPLES {
                assert_agree(op, TValue::Integer(a), TValue::Integer(b));
            }
        }
    }
}

#[test]
fn shl_shr_matches_on_integer_matrix() {
    // Include out-of-range shift amounts to exercise the NBITS gate.
    let shifts = [-65i64, -64, -63, -1, 0, 1, 63, 64, 65, 100];
    for &x in INT_SAMPLES {
        for &y in &shifts {
            assert_agree(ArithOp::Shl, TValue::Integer(x), TValue::Integer(y));
            assert_agree(ArithOp::Shr, TValue::Integer(x), TValue::Integer(y));
        }
    }
}

#[test]
fn unary_matches_on_integer_samples() {
    for &a in INT_SAMPLES {
        // For unary ops, C takes `p2` but ignores its value; we pass
        // `p1` twice to match how the VM calls raw_arith.
        assert_agree(ArithOp::Unm, TValue::Integer(a), TValue::Integer(a));
        assert_agree(ArithOp::BNot, TValue::Integer(a), TValue::Integer(a));
    }
}

// -- float × float -------------------------------------------------------

#[test]
fn add_sub_mul_div_match_on_floats() {
    for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul, ArithOp::Div] {
        for &a in FLOAT_SAMPLES {
            for &b in FLOAT_SAMPLES {
                assert_agree(op, TValue::Number(a), TValue::Number(b));
            }
        }
    }
}

#[test]
fn pow_matches_on_floats() {
    // Include b = 2.0 to exercise the square shortcut.
    let exps = [0.0, 1.0, 2.0, 3.0, 0.5, -1.0, -2.0];
    for &a in FLOAT_SAMPLES {
        for &b in &exps {
            assert_agree(ArithOp::Pow, TValue::Number(a), TValue::Number(b));
        }
    }
}

#[test]
fn float_idiv_and_mod_match() {
    for op in [ArithOp::IDiv, ArithOp::Mod] {
        for &a in FLOAT_SAMPLES {
            for &b in FLOAT_SAMPLES {
                assert_agree(op, TValue::Number(a), TValue::Number(b));
            }
        }
    }
}

// -- mixed int/float promotion --------------------------------------------

#[test]
fn mixed_int_float_promotion_matches_c() {
    let ints = [0i64, 1, -1, 7, -7];
    let floats = [0.0, 1.5, -2.25, 1e9];
    for op in [ArithOp::Add, ArithOp::Sub, ArithOp::Mul] {
        for &a in &ints {
            for &b in &floats {
                assert_agree(op, TValue::Integer(a), TValue::Number(b));
                assert_agree(op, TValue::Number(b), TValue::Integer(a));
            }
        }
    }
}

// -- metamethod fallback path (operands not numeric) ---------------------

#[test]
fn nil_operand_triggers_metamethod_fallback() {
    // Pass integers for the "encoded" operand, even though the oracle
    // will see them as proper TValue::Integer — here we're just testing
    // that the Rust side reports NonNumeric when given a TValue::Nil.
    // The C oracle shim only encodes numeric operands, so we do not
    // cross-check nil through it; this is a Rust-only sanity test.
    let r = raw_arith(ArithOp::Add, &TValue::Nil, &TValue::Integer(1));
    assert!(matches!(r, Ok(None)));
}
