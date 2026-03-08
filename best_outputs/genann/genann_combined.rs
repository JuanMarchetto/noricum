use std::io::{self, BufRead, Read, Write};

const LOOKUP_SIZE: usize = 4096;
const SIGMOID_DOM_MIN: f64 = -15.0;
const SIGMOID_DOM_MAX: f64 = 15.0;
const LTEST_FLOAT_TOLERANCE: f64 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ActivationFn {
    Sigmoid,
    SigmoidCached,
    Threshold,
    Linear,
}

impl ActivationFn {
    fn apply(&self, ann: &Genann, a: f64) -> f64 {
        match self {
            ActivationFn::Sigmoid => genann_act_sigmoid(a),
            ActivationFn::SigmoidCached => ann.act_sigmoid_cached(a),
            ActivationFn::Threshold => genann_act_threshold(a),
            ActivationFn::Linear => genann_act_linear(a),
        }
    }
}

fn genann_act_sigmoid(a: f64) -> f64 {
    if a < -45.0 {
        return 0.0;
    }
    if a > 45.0 {
        return 1.0;
    }
    1.0 / (1.0 + (-a).exp())
}

fn genann_act_threshold(a: f64) -> f64 {
    if a > 0.0 { 1.0 } else { 0.0 }
}

fn genann_act_linear(a: f64) -> f64 {
    a
}

#[derive(Clone)]
struct Genann {
    inputs: usize,
    hidden_layers: usize,
    hidden: usize,
    outputs: usize,
    activation_hidden: ActivationFn,
    activation_output: ActivationFn,
    total_weights: usize,
    total_neurons: usize,
    weight: Vec<f64>,
    output: Vec<f64>,
    delta: Vec<f64>,
    lookup: [f64; LOOKUP_SIZE],
    interval: f64,
}

/// glibc TYPE_3 random number generator (degree 31)
struct CRng {
    state: [i32; 31],
    fptr: usize,
    rptr: usize,
}

impl CRng {
    fn new(seed: u32) -> Self {
        let mut state = [0i32; 31];
        state[0] = seed as i32;
        for n in 1..31 {
            let prev = state[n - 1] as i64;
            state[n] = (16807i64.wrapping_mul(prev) % 2147483647) as i32;
        }
        let mut rng = CRng {
            state,
            fptr: 3,
            rptr: 0,
        };
        for _ in 0..310 {
            rng.next_int();
        }
        rng
    }

    fn next_int(&mut self) -> i32 {
        let val = self.state[self.fptr].wrapping_add(self.state[self.rptr]);
        self.state[self.fptr] = val;
        let result = ((val as u32) >> 1) as i32;
        self.fptr = (self.fptr + 1) % 31;
        self.rptr = (self.rptr + 1) % 31;
        result
    }

    fn next_double(&mut self) -> f64 {
        self.next_int() as f64 / 2147483647.0
    }
}

impl Genann {
    fn new(inputs: usize, hidden_layers: usize, hidden: usize, outputs: usize, rng: &mut CRng) -> Option<Self> {
        if inputs < 1 || outputs < 1 {
            return None;
        }
        if hidden_layers > 0 && hidden < 1 {
            return None;
        }

        let hidden_weights = if hidden_layers != 0 {
            (inputs + 1) * hidden + (hidden_layers - 1) * (hidden + 1) * hidden
        } else {
            0
        };
        let output_weights = if hidden_layers != 0 {
            (hidden + 1) * outputs
        } else {
            (inputs + 1) * outputs
        };
        let total_weights = hidden_weights + output_weights;
        let total_neurons = inputs + hidden * hidden_layers + outputs;

        let mut ann = Genann {
            inputs,
            hidden_layers,
            hidden,
            outputs,
            activation_hidden: ActivationFn::SigmoidCached,
            activation_output: ActivationFn::SigmoidCached,
            total_weights,
            total_neurons,
            weight: vec![0.0; total_weights],
            output: vec![0.0; total_neurons],
            delta: vec![0.0; total_neurons - inputs],
            lookup: [0.0; LOOKUP_SIZE],
            interval: 0.0,
        };

        ann.randomize(rng);
        ann.init_sigmoid_lookup();
        Some(ann)
    }

    fn init_sigmoid_lookup(&mut self) {
        let f = (SIGMOID_DOM_MAX - SIGMOID_DOM_MIN) / LOOKUP_SIZE as f64;
        self.interval = LOOKUP_SIZE as f64 / (SIGMOID_DOM_MAX - SIGMOID_DOM_MIN);
        for (idx, slot) in self.lookup.iter_mut().enumerate() {
            *slot = genann_act_sigmoid(SIGMOID_DOM_MIN + f * idx as f64);
        }
    }

    fn act_sigmoid_cached(&self, a: f64) -> f64 {
        debug_assert!(!a.is_nan());
        if a < SIGMOID_DOM_MIN {
            return self.lookup[0];
        }
        if a >= SIGMOID_DOM_MAX {
            return self.lookup[LOOKUP_SIZE - 1];
        }
        let j = ((a - SIGMOID_DOM_MIN) * self.interval + 0.5) as usize;
        if j >= LOOKUP_SIZE {
            return self.lookup[LOOKUP_SIZE - 1];
        }
        self.lookup[j]
    }

    fn randomize(&mut self, rng: &mut CRng) {
        for w in &mut self.weight {
            *w = rng.next_double() - 0.5;
        }
    }

    fn run(&mut self, inputs: &[f64]) -> &[f64] {
        self.output[..self.inputs].copy_from_slice(&inputs[..self.inputs]);

        let mut w_pos: usize = 0;
        let mut o_pos: usize = self.inputs;

        if self.hidden_layers == 0 {
            let ret_start = o_pos;
            for _ in 0..self.outputs {
                let mut sum = self.weight[w_pos] * -1.0;
                w_pos += 1;
                for k in 0..self.inputs {
                    sum += self.weight[w_pos] * self.output[k];
                    w_pos += 1;
                }
                let val = self.activation_output.apply(self, sum);
                self.output[o_pos] = val;
                o_pos += 1;
            }
            return &self.output[ret_start..ret_start + self.outputs];
        }

        let mut i_start: usize = 0;
        for _ in 0..self.hidden {
            let mut sum = self.weight[w_pos] * -1.0;
            w_pos += 1;
            for k in 0..self.inputs {
                sum += self.weight[w_pos] * self.output[i_start + k];
                w_pos += 1;
            }
            let val = self.activation_hidden.apply(self, sum);
            self.output[o_pos] = val;
            o_pos += 1;
        }

        i_start += self.inputs;

        for _ in 1..self.hidden_layers {
            for _ in 0..self.hidden {
                let mut sum = self.weight[w_pos] * -1.0;
                w_pos += 1;
                for k in 0..self.hidden {
                    sum += self.weight[w_pos] * self.output[i_start + k];
                    w_pos += 1;
                }
                let val = self.activation_hidden.apply(self, sum);
                self.output[o_pos] = val;
                o_pos += 1;
            }
            i_start += self.hidden;
        }

        let ret_start = o_pos;

        for _ in 0..self.outputs {
            let mut sum = self.weight[w_pos] * -1.0;
            w_pos += 1;
            for k in 0..self.hidden {
                sum += self.weight[w_pos] * self.output[i_start + k];
                w_pos += 1;
            }
            let val = self.activation_output.apply(self, sum);
            self.output[o_pos] = val;
            o_pos += 1;
        }

        debug_assert_eq!(w_pos, self.total_weights);
        debug_assert_eq!(o_pos, self.total_neurons);

        &self.output[ret_start..ret_start + self.outputs]
    }

    fn train(&mut self, inputs: &[f64], desired_outputs: &[f64], learning_rate: f64) {
        self.run(inputs);

        // Set output layer deltas
        {
            let o_start = self.inputs + self.hidden * self.hidden_layers;
            let d_start = self.hidden * self.hidden_layers;
            let is_linear = self.activation_output == ActivationFn::Linear;

            for n in 0..self.outputs {
                let o = self.output[o_start + n];
                let t = desired_outputs[n];
                self.delta[d_start + n] = if is_linear {
                    t - o
                } else {
                    (t - o) * o * (1.0 - o)
                };
            }
        }

        // Set hidden layer deltas (backwards)
        for h in (0..self.hidden_layers).rev() {
            let o_start = self.inputs + h * self.hidden;
            let d_start = h * self.hidden;
            let dd_start = (h + 1) * self.hidden;
            let ww_start = (self.inputs + 1) * self.hidden + (self.hidden + 1) * self.hidden * h;

            let next_layer_size = if h == self.hidden_layers - 1 {
                self.outputs
            } else {
                self.hidden
            };

            for j in 0..self.hidden {
                let mut delta_val = 0.0;
                for k in 0..next_layer_size {
                    let forward_delta = self.delta[dd_start + k];
                    let windex = k * (self.hidden + 1) + (j + 1);
                    delta_val += forward_delta * self.weight[ww_start + windex];
                }
                let o = self.output[o_start + j];
                self.delta[d_start + j] = o * (1.0 - o) * delta_val;
            }
        }

        // Train the outputs
        {
            let d_start = self.hidden * self.hidden_layers;
            let w_start = if self.hidden_layers != 0 {
                (self.inputs + 1) * self.hidden + (self.hidden + 1) * self.hidden * (self.hidden_layers - 1)
            } else {
                0
            };
            let i_start = if self.hidden_layers != 0 {
                self.inputs + self.hidden * (self.hidden_layers - 1)
            } else {
                0
            };
            let input_count = if self.hidden_layers != 0 { self.hidden } else { self.inputs };

            let mut w_pos = w_start;
            for j in 0..self.outputs {
                let d = self.delta[d_start + j];
                self.weight[w_pos] += d * learning_rate * -1.0;
                w_pos += 1;
                for k in 0..input_count {
                    self.weight[w_pos] += d * learning_rate * self.output[i_start + k];
                    w_pos += 1;
                }
            }
            debug_assert_eq!(w_pos, self.total_weights);
        }

        // Train the hidden layers
        for h in (0..self.hidden_layers).rev() {
            let d_start = h * self.hidden;
            let i_start = if h != 0 { self.inputs + self.hidden * (h - 1) } else { 0 };
            let mut w_pos = if h != 0 {
                (self.inputs + 1) * self.hidden + (self.hidden + 1) * self.hidden * (h - 1)
            } else {
                0
            };
            let input_count = if h == 0 { self.inputs } else { self.hidden };

            for j in 0..self.hidden {
                let d = self.delta[d_start + j];
                self.weight[w_pos] += d * learning_rate * -1.0;
                w_pos += 1;
                for k in 0..input_count {
                    self.weight[w_pos] += d * learning_rate * self.output[i_start + k];
                    w_pos += 1;
                }
            }
        }
    }

    fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(out, "{} {} {} {}", self.inputs, self.hidden_layers, self.hidden, self.outputs)?;
        for w in &self.weight {
            write!(out, " {w:.20e}")?;
        }
        Ok(())
    }

    fn read_from<R: BufRead>(reader: &mut R, rng: &mut CRng) -> Option<Self> {
        let mut all_content = String::new();
        reader.read_to_string(&mut all_content).ok()?;
        let mut tokens = all_content.split_whitespace();

        let inputs: usize = tokens.next()?.parse().ok()?;
        let hidden_layers: usize = tokens.next()?.parse().ok()?;
        let hidden: usize = tokens.next()?.parse().ok()?;
        let outputs: usize = tokens.next()?.parse().ok()?;

        let mut ann = Genann::new(inputs, hidden_layers, hidden, outputs, rng)?;
        for w in &mut ann.weight {
            *w = tokens.next()?.parse().ok()?;
        }
        Some(ann)
    }
}

struct TestState {
    tests: i32,
    fails: i32,
}

impl TestState {
    fn new() -> Self {
        TestState { tests: 0, fails: 0 }
    }

    fn lok(&mut self, test: bool, file: &str, line: u32) {
        self.tests += 1;
        if !test {
            self.fails += 1;
            println!("FAIL: {file}:{line}");
        }
    }

    fn lequal_usize(&mut self, a: usize, b: usize, file: &str, line: u32) {
        self.tests += 1;
        if a != b {
            self.fails += 1;
            println!("FAIL: {file}:{line} ({a} != {b})");
        }
    }

    fn lfequal(&mut self, a: f64, b: f64, file: &str, line: u32) {
        self.tests += 1;
        if (a - b).abs() > LTEST_FLOAT_TOLERANCE {
            self.fails += 1;
            println!("FAIL: {file}:{line} ({a} != {b})");
        }
    }
}

macro_rules! lok {
    ($state:expr, $test:expr) => {
        $state.lok($test, file!(), line!())
    };
}

macro_rules! lequal {
    ($state:expr, $a:expr, $b:expr) => {
        $state.lequal_usize($a, $b, file!(), line!())
    };
}

macro_rules! lfequal {
    ($state:expr, $a:expr, $b:expr) => {
        $state.lfequal($a, $b, file!(), line!())
    };
}

fn test_basic(ts: &mut TestState, rng: &mut CRng) {
    let mut ann = Genann::new(1, 0, 0, 1, rng).expect("init failed");

    lequal!(ts, ann.total_weights, 2);

    ann.weight[0] = 0.0;
    ann.weight[1] = 0.0;
    lfequal!(ts, 0.5, ann.run(&[0.0])[0]);
    lfequal!(ts, 0.5, ann.run(&[1.0])[0]);
    lfequal!(ts, 0.5, ann.run(&[11.0])[0]);

    ann.weight[0] = 1.0;
    ann.weight[1] = 1.0;
    lfequal!(ts, 0.5, ann.run(&[1.0])[0]);

    ann.weight[0] = 1.0;
    ann.weight[1] = 1.0;
    lfequal!(ts, 1.0, ann.run(&[10.0])[0]);
    lfequal!(ts, 0.0, ann.run(&[-10.0])[0]);

    println!("test_basic passed");
}

fn test_xor(ts: &mut TestState, rng: &mut CRng) {
    let mut ann = Genann::new(2, 1, 2, 1, rng).expect("init failed");
    ann.activation_hidden = ActivationFn::Threshold;
    ann.activation_output = ActivationFn::Threshold;

    lequal!(ts, ann.total_weights, 9);

    ann.weight[0] = 0.5;
    ann.weight[1] = 1.0;
    ann.weight[2] = 1.0;
    ann.weight[3] = 1.0;
    ann.weight[4] = 1.0;
    ann.weight[5] = 1.0;
    ann.weight[6] = 0.5;
    ann.weight[7] = 1.0;
    ann.weight[8] = -1.0;

    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let expected: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

    for (inp, exp) in input.iter().zip(expected.iter()) {
        lfequal!(ts, *exp, ann.run(inp)[0]);
    }

    println!("test_xor passed");
}

fn test_backprop(ts: &mut TestState, rng: &mut CRng) {
    let mut ann = Genann::new(1, 0, 0, 1, rng).expect("init failed");

    let input = 0.5;
    let output = 1.0;

    let first_try = ann.run(&[input])[0];
    ann.train(&[input], &[output], 0.5);
    let second_try = ann.run(&[input])[0];
    lok!(ts, (first_try - output).abs() > (second_try - output).abs());

    println!("test_backprop passed");
}

fn test_train_and(ts: &mut TestState, rng: &mut CRng) {
    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let expected: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

    let mut ann = Genann::new(2, 0, 0, 1, rng).expect("init failed");

    for _ in 0..50 {
        for (inp, exp) in input.iter().zip(expected.iter()) {
            ann.train(inp, &[*exp], 0.8);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    for (inp, exp) in input.iter().zip(expected.iter()) {
        lfequal!(ts, *exp, ann.run(inp)[0]);
    }

    println!("test_train_and passed");
}

fn test_train_or(ts: &mut TestState, rng: &mut CRng) {
    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let expected: [f64; 4] = [0.0, 1.0, 1.0, 1.0];

    let mut ann = Genann::new(2, 0, 0, 1, rng).expect("init failed");
    ann.randomize(rng);

    for _ in 0..50 {
        for (inp, exp) in input.iter().zip(expected.iter()) {
            ann.train(inp, &[*exp], 0.8);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    for (inp, exp) in input.iter().zip(expected.iter()) {
        lfequal!(ts, *exp, ann.run(inp)[0]);
    }

    println!("test_train_or passed");
}

fn test_train_xor(ts: &mut TestState, rng: &mut CRng) {
    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let expected: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

    let mut ann = Genann::new(2, 1, 2, 1, rng).expect("init failed");

    for _ in 0..500 {
        for (inp, exp) in input.iter().zip(expected.iter()) {
            ann.train(inp, &[*exp], 3.0);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    for (inp, exp) in input.iter().zip(expected.iter()) {
        lfequal!(ts, *exp, ann.run(inp)[0]);
    }

    println!("test_train_xor passed");
}

fn test_persist(ts: &mut TestState, rng: &mut CRng) {
    let first = Genann::new(1000, 5, 50, 10, rng).expect("init failed");

    let mut buf = Vec::new();
    first.write_to(&mut buf).expect("write failed");

    let mut cursor = io::BufReader::new(&buf[..]);
    let mut dummy_rng = CRng::new(0);
    let second = Genann::read_from(&mut cursor, &mut dummy_rng).expect("read failed");

    lequal!(ts, first.inputs, second.inputs);
    lequal!(ts, first.hidden_layers, second.hidden_layers);
    lequal!(ts, first.hidden, second.hidden);
    lequal!(ts, first.outputs, second.outputs);
    lequal!(ts, first.total_weights, second.total_weights);

    for (a, b) in first.weight.iter().zip(second.weight.iter()) {
        lok!(ts, *a == *b);
    }

    println!("test_persist passed");
}

fn test_copy(ts: &mut TestState, rng: &mut CRng) {
    let first = Genann::new(1000, 5, 50, 10, rng).expect("init failed");
    let second = first.clone();

    lequal!(ts, first.inputs, second.inputs);
    lequal!(ts, first.hidden_layers, second.hidden_layers);
    lequal!(ts, first.hidden, second.hidden);
    lequal!(ts, first.outputs, second.outputs);
    lequal!(ts, first.total_weights, second.total_weights);

    for (a, b) in first.weight.iter().zip(second.weight.iter()) {
        lfequal!(ts, *a, *b);
    }

    println!("test_copy passed");
}

fn test_sigmoid(ts: &mut TestState, rng: &mut CRng) {
    let ann = Genann::new(1, 0, 0, 1, rng).expect("init failed");

    let mut val = -20.0f64;
    let max = 20.0;
    let d = 0.0001;

    while val < max {
        lfequal!(ts, genann_act_sigmoid(val), ann.act_sigmoid_cached(val));
        val += d;
    }
    println!("test_sigmoid passed");
}

fn main() {
    println!("GENANN TEST SUITE");

    let mut rng = CRng::new(100);
    let mut ts = TestState::new();

    test_basic(&mut ts, &mut rng);
    test_xor(&mut ts, &mut rng);
    test_backprop(&mut ts, &mut rng);
    test_train_and(&mut ts, &mut rng);
    test_train_or(&mut ts, &mut rng);
    test_train_xor(&mut ts, &mut rng);
    test_persist(&mut ts, &mut rng);
    test_copy(&mut ts, &mut rng);
    test_sigmoid(&mut ts, &mut rng);

    if ts.fails == 0 {
        println!("ALL TESTS PASSED ({}/{})", ts.tests, ts.tests);
    } else {
        println!("SOME TESTS FAILED ({}/{})", ts.tests - ts.fails, ts.tests);
    }

    std::process::exit(if ts.fails != 0 { 1 } else { 0 });
}
