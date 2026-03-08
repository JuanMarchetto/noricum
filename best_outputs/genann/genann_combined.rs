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
    if a > 0.0 {
        1.0
    } else {
        0.0
    }
}

fn genann_act_linear(a: f64) -> f64 {
    a
}

#[derive(Clone)]
struct Genann {
    inputs: i32,
    hidden_layers: i32,
    hidden: i32,
    outputs: i32,
    activation_hidden: ActivationFn,
    activation_output: ActivationFn,
    total_weights: i32,
    total_neurons: i32,
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
        for i in 1..31 {
            let prev = state[i - 1] as i64;
            let val = (16807i64.wrapping_mul(prev)) % 2147483647;
            state[i] = val as i32;
        }
        let mut rng = CRng {
            state,
            fptr: 3,
            rptr: 0,
        };
        // glibc does 310 iterations to warm up
        for _ in 0..310 {
            rng.next_int();
        }
        rng
    }

    fn next_int(&mut self) -> i32 {
        let val = self.state[self.fptr].wrapping_add(self.state[self.rptr]);
        self.state[self.fptr] = val;
        let result = ((val as u32) >> 1) as i32;
        self.fptr += 1;
        if self.fptr >= 31 {
            self.fptr = 0;
        }
        self.rptr += 1;
        if self.rptr >= 31 {
            self.rptr = 0;
        }
        result
    }

    fn next_double(&mut self) -> f64 {
        let val = self.next_int();
        (val as f64) / 2147483647.0
    }
}

impl Genann {
    fn new(inputs: i32, hidden_layers: i32, hidden: i32, outputs: i32, rng: &mut CRng) -> Option<Self> {
        if hidden_layers < 0 {
            return None;
        }
        if inputs < 1 {
            return None;
        }
        if outputs < 1 {
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

        let weight = vec![0.0f64; total_weights as usize];
        let output = vec![0.0f64; total_neurons as usize];
        let delta = vec![0.0f64; (total_neurons - inputs) as usize];

        let mut ann = Genann {
            inputs,
            hidden_layers,
            hidden,
            outputs,
            activation_hidden: ActivationFn::SigmoidCached,
            activation_output: ActivationFn::SigmoidCached,
            total_weights,
            total_neurons,
            weight,
            output,
            delta,
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
        for i in 0..LOOKUP_SIZE {
            self.lookup[i] = genann_act_sigmoid(SIGMOID_DOM_MIN + f * i as f64);
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
        for i in 0..self.total_weights as usize {
            let r = rng.next_double();
            self.weight[i] = r - 0.5;
        }
    }

    fn run(&mut self, inputs: &[f64]) -> &[f64] {
        let ann_inputs = self.inputs as usize;
        let ann_hidden = self.hidden as usize;
        let ann_outputs = self.outputs as usize;
        let ann_hidden_layers = self.hidden_layers as usize;

        self.output[..ann_inputs].copy_from_slice(&inputs[..ann_inputs]);

        let mut w_idx: usize = 0;
        let mut o_idx: usize = ann_inputs;

        if ann_hidden_layers == 0 {
            let ret_start = o_idx;
            for _j in 0..ann_outputs {
                let mut sum = self.weight[w_idx] * -1.0;
                w_idx += 1;
                for k in 0..ann_inputs {
                    sum += self.weight[w_idx] * self.output[k];
                    w_idx += 1;
                }
                let val = self.activation_output.apply(self, sum);
                self.output[o_idx] = val;
                o_idx += 1;
            }
            return &self.output[ret_start..ret_start + ann_outputs];
        }

        let mut i_start: usize = 0;
        for _j in 0..ann_hidden {
            let mut sum = self.weight[w_idx] * -1.0;
            w_idx += 1;
            for k in 0..ann_inputs {
                sum += self.weight[w_idx] * self.output[i_start + k];
                w_idx += 1;
            }
            let val = self.activation_hidden.apply(self, sum);
            self.output[o_idx] = val;
            o_idx += 1;
        }

        i_start += ann_inputs;

        for _h in 1..ann_hidden_layers {
            for _j in 0..ann_hidden {
                let mut sum = self.weight[w_idx] * -1.0;
                w_idx += 1;
                for k in 0..ann_hidden {
                    sum += self.weight[w_idx] * self.output[i_start + k];
                    w_idx += 1;
                }
                let val = self.activation_hidden.apply(self, sum);
                self.output[o_idx] = val;
                o_idx += 1;
            }
            i_start += ann_hidden;
        }

        let ret_start = o_idx;

        for _j in 0..ann_outputs {
            let mut sum = self.weight[w_idx] * -1.0;
            w_idx += 1;
            for k in 0..ann_hidden {
                sum += self.weight[w_idx] * self.output[i_start + k];
                w_idx += 1;
            }
            let val = self.activation_output.apply(self, sum);
            self.output[o_idx] = val;
            o_idx += 1;
        }

        debug_assert_eq!(w_idx, self.total_weights as usize);
        debug_assert_eq!(o_idx, self.total_neurons as usize);

        &self.output[ret_start..ret_start + ann_outputs]
    }

    fn train(&mut self, inputs: &[f64], desired_outputs: &[f64], learning_rate: f64) {
        self.run(inputs);

        let ann_inputs = self.inputs as usize;
        let ann_hidden = self.hidden as usize;
        let ann_outputs = self.outputs as usize;
        let ann_hidden_layers = self.hidden_layers as usize;

        {
            let o_start = ann_inputs + ann_hidden * ann_hidden_layers;
            let d_start = ann_hidden * ann_hidden_layers;

            let is_linear = self.activation_output == ActivationFn::Linear;

            if is_linear {
                for j in 0..ann_outputs {
                    self.delta[d_start + j] = desired_outputs[j] - self.output[o_start + j];
                }
            } else {
                for j in 0..ann_outputs {
                    let o = self.output[o_start + j];
                    let t = desired_outputs[j];
                    self.delta[d_start + j] = (t - o) * o * (1.0 - o);
                }
            }
        }

        for h in (0..ann_hidden_layers).rev() {
            let o_start = ann_inputs + h * ann_hidden;
            let d_start = h * ann_hidden;
            let dd_start = (h + 1) * ann_hidden;
            let ww_start = (ann_inputs + 1) * ann_hidden + (ann_hidden + 1) * ann_hidden * h;

            let next_layer_size = if h == ann_hidden_layers - 1 {
                ann_outputs
            } else {
                ann_hidden
            };

            for j in 0..ann_hidden {
                let mut delta_val = 0.0;
                for k in 0..next_layer_size {
                    let forward_delta = self.delta[dd_start + k];
                    let windex = k * (ann_hidden + 1) + (j + 1);
                    let forward_weight = self.weight[ww_start + windex];
                    delta_val += forward_delta * forward_weight;
                }
                let o = self.output[o_start + j];
                self.delta[d_start + j] = o * (1.0 - o) * delta_val;
            }
        }

        {
            let d_start = ann_hidden * ann_hidden_layers;
            let w_start = if ann_hidden_layers != 0 {
                (ann_inputs + 1) * ann_hidden + (ann_hidden + 1) * ann_hidden * (ann_hidden_layers - 1)
            } else {
                0
            };
            let i_start = if ann_hidden_layers != 0 {
                ann_inputs + ann_hidden * (ann_hidden_layers - 1)
            } else {
                0
            };

            let input_count = if ann_hidden_layers != 0 {
                ann_hidden
            } else {
                ann_inputs
            };

            let mut w_idx = w_start;
            for j in 0..ann_outputs {
                let d = self.delta[d_start + j];
                self.weight[w_idx] += d * learning_rate * -1.0;
                w_idx += 1;
                for k in 0..input_count {
                    self.weight[w_idx] += d * learning_rate * self.output[i_start + k];
                    w_idx += 1;
                }
            }
            debug_assert_eq!(w_idx, self.total_weights as usize);
        }

        for h in (0..ann_hidden_layers).rev() {
            let d_start = h * ann_hidden;
            let i_start = if h != 0 {
                ann_inputs + ann_hidden * (h - 1)
            } else {
                0
            };
            let mut w_idx = if h != 0 {
                (ann_inputs + 1) * ann_hidden + (ann_hidden + 1) * ann_hidden * (h - 1)
            } else {
                0
            };

            let input_count = if h == 0 { ann_inputs } else { ann_hidden };

            for j in 0..ann_hidden {
                let d = self.delta[d_start + j];
                self.weight[w_idx] += d * learning_rate * -1.0;
                w_idx += 1;
                for k in 0..input_count {
                    self.weight[w_idx] += d * learning_rate * self.output[i_start + k];
                    w_idx += 1;
                }
            }
        }
    }

    fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            "{} {} {} {}",
            self.inputs, self.hidden_layers, self.hidden, self.outputs
        )?;

        for i in 0..self.total_weights as usize {
            write!(out, " {:.20e}", self.weight[i])?;
        }

        Ok(())
    }

    fn read_from<R: BufRead>(reader: &mut R, rng: &mut CRng) -> Option<Self> {
        let mut all_content = String::new();
        reader.read_to_string(&mut all_content).ok()?;

        let mut tokens = all_content.split_whitespace();

        let inputs: i32 = tokens.next()?.parse().ok()?;
        let hidden_layers: i32 = tokens.next()?.parse().ok()?;
        let hidden: i32 = tokens.next()?.parse().ok()?;
        let outputs: i32 = tokens.next()?.parse().ok()?;

        let mut ann = Genann::new(inputs, hidden_layers, hidden, outputs, rng)?;

        for i in 0..ann.total_weights as usize {
            let val: f64 = tokens.next()?.parse().ok()?;
            ann.weight[i] = val;
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
            println!("FAIL: {}:{}", file, line);
        }
    }

    fn lequal(&mut self, a: i32, b: i32, file: &str, line: u32) {
        self.tests += 1;
        if a != b {
            self.fails += 1;
            println!("FAIL: {}:{} ({} != {})", file, line, a, b);
        }
    }

    fn lfequal(&mut self, a: f64, b: f64, file: &str, line: u32) {
        self.tests += 1;
        if (a - b).abs() > LTEST_FLOAT_TOLERANCE {
            self.fails += 1;
            println!("FAIL: {}:{} ({} != {})", file, line, a, b);
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
        $state.lequal($a, $b, file!(), line!())
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

    let mut a: f64;

    a = 0.0;
    ann.weight[0] = 0.0;
    ann.weight[1] = 0.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 0.5, result);

    a = 1.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 0.5, result);

    a = 11.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 0.5, result);

    a = 1.0;
    ann.weight[0] = 1.0;
    ann.weight[1] = 1.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 0.5, result);

    a = 10.0;
    ann.weight[0] = 1.0;
    ann.weight[1] = 1.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 1.0, result);

    a = -10.0;
    let result = ann.run(&[a])[0];
    lfequal!(ts, 0.0, result);

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
    let output: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

    let r = ann.run(&input[0])[0];
    lfequal!(ts, output[0], r);
    let r = ann.run(&input[1])[0];
    lfequal!(ts, output[1], r);
    let r = ann.run(&input[2])[0];
    lfequal!(ts, output[2], r);
    let r = ann.run(&input[3])[0];
    lfequal!(ts, output[3], r);

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
    let output: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

    let mut ann = Genann::new(2, 0, 0, 1, rng).expect("init failed");

    for _i in 0..50 {
        for j in 0..4 {
            ann.train(&input[j], &[output[j]], 0.8);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    let r = ann.run(&input[0])[0];
    lfequal!(ts, output[0], r);
    let r = ann.run(&input[1])[0];
    lfequal!(ts, output[1], r);
    let r = ann.run(&input[2])[0];
    lfequal!(ts, output[2], r);
    let r = ann.run(&input[3])[0];
    lfequal!(ts, output[3], r);

    println!("test_train_and passed");
}

fn test_train_or(ts: &mut TestState, rng: &mut CRng) {
    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let output: [f64; 4] = [0.0, 1.0, 1.0, 1.0];

    let mut ann = Genann::new(2, 0, 0, 1, rng).expect("init failed");
    ann.randomize(rng);

    for _i in 0..50 {
        for j in 0..4 {
            ann.train(&input[j], &[output[j]], 0.8);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    let r = ann.run(&input[0])[0];
    lfequal!(ts, output[0], r);
    let r = ann.run(&input[1])[0];
    lfequal!(ts, output[1], r);
    let r = ann.run(&input[2])[0];
    lfequal!(ts, output[2], r);
    let r = ann.run(&input[3])[0];
    lfequal!(ts, output[3], r);

    println!("test_train_or passed");
}

fn test_train_xor(ts: &mut TestState, rng: &mut CRng) {
    let input: [[f64; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];
    let output: [f64; 4] = [0.0, 1.0, 1.0, 0.0];

    let mut ann = Genann::new(2, 1, 2, 1, rng).expect("init failed");

    for _i in 0..500 {
        for j in 0..4 {
            ann.train(&input[j], &[output[j]], 3.0);
        }
    }

    ann.activation_output = ActivationFn::Threshold;
    let r = ann.run(&input[0])[0];
    lfequal!(ts, output[0], r);
    let r = ann.run(&input[1])[0];
    lfequal!(ts, output[1], r);
    let r = ann.run(&input[2])[0];
    lfequal!(ts, output[2], r);
    let r = ann.run(&input[3])[0];
    lfequal!(ts, output[3], r);

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

    for i in 0..first.total_weights as usize {
        lok!(ts, first.weight[i] == second.weight[i]);
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

    for i in 0..first.total_weights as usize {
        lfequal!(ts, first.weight[i], second.weight[i]);
    }

    println!("test_copy passed");
}

fn test_sigmoid(ts: &mut TestState, rng: &mut CRng) {
    let ann = Genann::new(1, 0, 0, 1, rng).expect("init failed");

    let mut i = -20.0f64;
    let max = 20.0;
    let d = 0.0001;

    while i < max {
        lfequal!(ts, genann_act_sigmoid(i), ann.act_sigmoid_cached(i));
        i += d;
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