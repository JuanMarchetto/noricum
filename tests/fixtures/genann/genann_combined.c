/*
 * GENANN - Minimal C Artificial Neural Network
 * Combined standalone file for Noricum migration testing
 *
 * Original: https://github.com/codeplea/genann (2,246 stars)
 * Copyright (c) 2015-2018 Lewis Van Winkle - zlib license
 *
 * Migration challenges:
 * - Function pointer typedef (genann_actfun) -> trait objects or enum dispatch
 * - Single-malloc with internal pointer offsets -> separate Vec<f64> fields
 * - Global sigmoid lookup table -> struct field or module-level
 * - FILE I/O for persistence -> std::io::Read/Write
 * - C bool-to-double cast in act_threshold -> explicit if/else
 */

#include <assert.h>
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ============================================================
 * genann.h - inlined
 * ============================================================ */

struct genann;

typedef double (*genann_actfun)(const struct genann *ann, double a);

typedef struct genann {
    int inputs, hidden_layers, hidden, outputs;
    genann_actfun activation_hidden;
    genann_actfun activation_output;
    int total_weights;
    int total_neurons;
    double *weight;
    double *output;
    double *delta;
} genann;

#define GENANN_RANDOM() (((double)rand())/RAND_MAX)

genann *genann_init(int inputs, int hidden_layers, int hidden, int outputs);
genann *genann_read(FILE *in);
void genann_randomize(genann *ann);
genann *genann_copy(genann const *ann);
void genann_free(genann *ann);
double const *genann_run(genann const *ann, double const *inputs);
void genann_train(genann const *ann, double const *inputs, double const *desired_outputs, double learning_rate);
void genann_write(genann const *ann, FILE *out);
void genann_init_sigmoid_lookup(const genann *ann);
double genann_act_sigmoid(const genann *ann, double a);
double genann_act_sigmoid_cached(const genann *ann, double a);
double genann_act_threshold(const genann *ann, double a);
double genann_act_linear(const genann *ann, double a);

/* ============================================================
 * genann.c - inlined
 * ============================================================ */

#define genann_act_hidden genann_act_hidden_indirect
#define genann_act_output genann_act_output_indirect

#define LOOKUP_SIZE 4096

double genann_act_hidden_indirect(const struct genann *ann, double a) {
    return ann->activation_hidden(ann, a);
}

double genann_act_output_indirect(const struct genann *ann, double a) {
    return ann->activation_output(ann, a);
}

const double sigmoid_dom_min = -15.0;
const double sigmoid_dom_max = 15.0;
double interval;
double lookup[LOOKUP_SIZE];

double genann_act_sigmoid(const genann *ann, double a) {
    (void)ann;
    if (a < -45.0) return 0;
    if (a > 45.0) return 1;
    return 1.0 / (1 + exp(-a));
}

void genann_init_sigmoid_lookup(const genann *ann) {
    const double f = (sigmoid_dom_max - sigmoid_dom_min) / LOOKUP_SIZE;
    int i;
    interval = LOOKUP_SIZE / (sigmoid_dom_max - sigmoid_dom_min);
    for (i = 0; i < LOOKUP_SIZE; ++i) {
        lookup[i] = genann_act_sigmoid(ann, sigmoid_dom_min + f * i);
    }
}

double genann_act_sigmoid_cached(const genann *ann, double a) {
    (void)ann;
    assert(!isnan(a));

    if (a < sigmoid_dom_min) return lookup[0];
    if (a >= sigmoid_dom_max) return lookup[LOOKUP_SIZE - 1];

    size_t j = (size_t)((a-sigmoid_dom_min)*interval+0.5);

    if (j >= LOOKUP_SIZE) return lookup[LOOKUP_SIZE - 1];

    return lookup[j];
}

double genann_act_linear(const struct genann *ann, double a) {
    (void)ann;
    return a;
}

double genann_act_threshold(const struct genann *ann, double a) {
    (void)ann;
    return a > 0;
}

genann *genann_init(int inputs, int hidden_layers, int hidden, int outputs) {
    if (hidden_layers < 0) return 0;
    if (inputs < 1) return 0;
    if (outputs < 1) return 0;
    if (hidden_layers > 0 && hidden < 1) return 0;

    const int hidden_weights = hidden_layers ? (inputs+1) * hidden + (hidden_layers-1) * (hidden+1) * hidden : 0;
    const int output_weights = (hidden_layers ? (hidden+1) : (inputs+1)) * outputs;
    const int total_weights = (hidden_weights + output_weights);
    const int total_neurons = (inputs + hidden * hidden_layers + outputs);

    const int size = sizeof(genann) + sizeof(double) * (total_weights + total_neurons + (total_neurons - inputs));
    genann *ret = malloc(size);
    if (!ret) return 0;

    ret->inputs = inputs;
    ret->hidden_layers = hidden_layers;
    ret->hidden = hidden;
    ret->outputs = outputs;

    ret->total_weights = total_weights;
    ret->total_neurons = total_neurons;

    ret->weight = (double*)((char*)ret + sizeof(genann));
    ret->output = ret->weight + ret->total_weights;
    ret->delta = ret->output + ret->total_neurons;

    genann_randomize(ret);

    ret->activation_hidden = genann_act_sigmoid_cached;
    ret->activation_output = genann_act_sigmoid_cached;

    genann_init_sigmoid_lookup(ret);

    return ret;
}

genann *genann_read(FILE *in) {
    int inputs, hidden_layers, hidden, outputs;
    int rc;

    errno = 0;
    rc = fscanf(in, "%d %d %d %d", &inputs, &hidden_layers, &hidden, &outputs);
    if (rc < 4 || errno != 0) {
        perror("fscanf");
        return NULL;
    }

    genann *ann = genann_init(inputs, hidden_layers, hidden, outputs);

    int i;
    for (i = 0; i < ann->total_weights; ++i) {
        errno = 0;
        rc = fscanf(in, " %le", ann->weight + i);
        if (rc < 1 || errno != 0) {
            perror("fscanf");
            genann_free(ann);
            return NULL;
        }
    }

    return ann;
}

genann *genann_copy(genann const *ann) {
    const int size = sizeof(genann) + sizeof(double) * (ann->total_weights + ann->total_neurons + (ann->total_neurons - ann->inputs));
    genann *ret = malloc(size);
    if (!ret) return 0;

    memcpy(ret, ann, size);

    ret->weight = (double*)((char*)ret + sizeof(genann));
    ret->output = ret->weight + ret->total_weights;
    ret->delta = ret->output + ret->total_neurons;

    return ret;
}

void genann_randomize(genann *ann) {
    int i;
    for (i = 0; i < ann->total_weights; ++i) {
        double r = GENANN_RANDOM();
        ann->weight[i] = r - 0.5;
    }
}

void genann_free(genann *ann) {
    free(ann);
}

double const *genann_run(genann const *ann, double const *inputs) {
    double const *w = ann->weight;
    double *o = ann->output + ann->inputs;
    double const *i = ann->output;

    memcpy(ann->output, inputs, sizeof(double) * ann->inputs);

    int h, j, k;

    if (!ann->hidden_layers) {
        double *ret = o;
        for (j = 0; j < ann->outputs; ++j) {
            double sum = *w++ * -1.0;
            for (k = 0; k < ann->inputs; ++k) {
                sum += *w++ * i[k];
            }
            *o++ = genann_act_output(ann, sum);
        }
        return ret;
    }

    /* Figure input layer */
    for (j = 0; j < ann->hidden; ++j) {
        double sum = *w++ * -1.0;
        for (k = 0; k < ann->inputs; ++k) {
            sum += *w++ * i[k];
        }
        *o++ = genann_act_hidden(ann, sum);
    }

    i += ann->inputs;

    /* Figure hidden layers, if any. */
    for (h = 1; h < ann->hidden_layers; ++h) {
        for (j = 0; j < ann->hidden; ++j) {
            double sum = *w++ * -1.0;
            for (k = 0; k < ann->hidden; ++k) {
                sum += *w++ * i[k];
            }
            *o++ = genann_act_hidden(ann, sum);
        }
        i += ann->hidden;
    }

    double const *ret = o;

    /* Figure output layer. */
    for (j = 0; j < ann->outputs; ++j) {
        double sum = *w++ * -1.0;
        for (k = 0; k < ann->hidden; ++k) {
            sum += *w++ * i[k];
        }
        *o++ = genann_act_output(ann, sum);
    }

    assert(w - ann->weight == ann->total_weights);
    assert(o - ann->output == ann->total_neurons);

    return ret;
}

void genann_train(genann const *ann, double const *inputs, double const *desired_outputs, double learning_rate) {
    genann_run(ann, inputs);

    int h, j, k;

    /* First set the output layer deltas. */
    {
        double const *o = ann->output + ann->inputs + ann->hidden * ann->hidden_layers;
        double *d = ann->delta + ann->hidden * ann->hidden_layers;
        double const *t = desired_outputs;

        if (genann_act_output == genann_act_linear ||
                ann->activation_output == genann_act_linear) {
            for (j = 0; j < ann->outputs; ++j) {
                *d++ = *t++ - *o++;
            }
        } else {
            for (j = 0; j < ann->outputs; ++j) {
                *d++ = (*t - *o) * *o * (1.0 - *o);
                ++o; ++t;
            }
        }
    }

    /* Set hidden layer deltas, start on last layer and work backwards. */
    for (h = ann->hidden_layers - 1; h >= 0; --h) {
        double const *o = ann->output + ann->inputs + (h * ann->hidden);
        double *d = ann->delta + (h * ann->hidden);
        double const * const dd = ann->delta + ((h+1) * ann->hidden);
        double const * const ww = ann->weight + ((ann->inputs+1) * ann->hidden) + ((ann->hidden+1) * ann->hidden * (h));

        for (j = 0; j < ann->hidden; ++j) {
            double delta = 0;
            for (k = 0; k < (h == ann->hidden_layers-1 ? ann->outputs : ann->hidden); ++k) {
                const double forward_delta = dd[k];
                const int windex = k * (ann->hidden + 1) + (j + 1);
                const double forward_weight = ww[windex];
                delta += forward_delta * forward_weight;
            }
            *d = *o * (1.0-*o) * delta;
            ++d; ++o;
        }
    }

    /* Train the outputs. */
    {
        double const *d = ann->delta + ann->hidden * ann->hidden_layers;
        double *w = ann->weight + (ann->hidden_layers
                ? ((ann->inputs+1) * ann->hidden + (ann->hidden+1) * ann->hidden * (ann->hidden_layers-1))
                : (0));
        double const * const i = ann->output + (ann->hidden_layers
                ? (ann->inputs + (ann->hidden) * (ann->hidden_layers-1))
                : 0);

        for (j = 0; j < ann->outputs; ++j) {
            *w++ += *d * learning_rate * -1.0;
            for (k = 1; k < (ann->hidden_layers ? ann->hidden : ann->inputs) + 1; ++k) {
                *w++ += *d * learning_rate * i[k-1];
            }
            ++d;
        }
        assert(w - ann->weight == ann->total_weights);
    }

    /* Train the hidden layers. */
    for (h = ann->hidden_layers - 1; h >= 0; --h) {
        double const *d = ann->delta + (h * ann->hidden);
        double const *i = ann->output + (h
                ? (ann->inputs + ann->hidden * (h-1))
                : 0);
        double *w = ann->weight + (h
                ? ((ann->inputs+1) * ann->hidden + (ann->hidden+1) * (ann->hidden) * (h-1))
                : 0);

        for (j = 0; j < ann->hidden; ++j) {
            *w++ += *d * learning_rate * -1.0;
            for (k = 1; k < (h == 0 ? ann->inputs : ann->hidden) + 1; ++k) {
                *w++ += *d * learning_rate * i[k-1];
            }
            ++d;
        }
    }
}

void genann_write(genann const *ann, FILE *out) {
    fprintf(out, "%d %d %d %d", ann->inputs, ann->hidden_layers, ann->hidden, ann->outputs);

    int i;
    for (i = 0; i < ann->total_weights; ++i) {
        fprintf(out, " %.20e", ann->weight[i]);
    }
}

/* ============================================================
 * Test harness - deterministic output (no clock() timing)
 * ============================================================ */

#define LTEST_FLOAT_TOLERANCE 0.001

static int ltests = 0;
static int lfails = 0;

#define lok(test) do {\
    ++ltests;\
    if (!(test)) {\
        ++lfails;\
        printf("FAIL: %s:%d\n", __FILE__, __LINE__);\
    }} while (0)

#define lequal(a, b) do {\
    ++ltests;\
    if ((a) != (b)) {\
        ++lfails;\
        printf("FAIL: %s:%d (%d != %d)\n", __FILE__, __LINE__, (a), (b));\
    }} while (0)

#define lfequal(a, b) do {\
    ++ltests;\
    if (fabs((double)(a)-(double)(b)) > LTEST_FLOAT_TOLERANCE) {\
        ++lfails;\
        printf("FAIL: %s:%d (%f != %f)\n", __FILE__, __LINE__, (double)(a), (double)(b));\
    }} while (0)


void test_basic() {
    genann *ann = genann_init(1, 0, 0, 1);

    lequal(ann->total_weights, 2);
    double a;

    a = 0;
    ann->weight[0] = 0;
    ann->weight[1] = 0;
    lfequal(0.5, *genann_run(ann, &a));

    a = 1;
    lfequal(0.5, *genann_run(ann, &a));

    a = 11;
    lfequal(0.5, *genann_run(ann, &a));

    a = 1;
    ann->weight[0] = 1;
    ann->weight[1] = 1;
    lfequal(0.5, *genann_run(ann, &a));

    a = 10;
    ann->weight[0] = 1;
    ann->weight[1] = 1;
    lfequal(1.0, *genann_run(ann, &a));

    a = -10;
    lfequal(0.0, *genann_run(ann, &a));

    genann_free(ann);
    printf("test_basic passed\n");
}


void test_xor() {
    genann *ann = genann_init(2, 1, 2, 1);
    ann->activation_hidden = genann_act_threshold;
    ann->activation_output = genann_act_threshold;

    lequal(ann->total_weights, 9);

    /* First hidden. */
    ann->weight[0] = .5;
    ann->weight[1] = 1;
    ann->weight[2] = 1;

    /* Second hidden. */
    ann->weight[3] = 1;
    ann->weight[4] = 1;
    ann->weight[5] = 1;

    /* Output. */
    ann->weight[6] = .5;
    ann->weight[7] = 1;
    ann->weight[8] = -1;

    double input[4][2] = {{0, 0}, {0, 1}, {1, 0}, {1, 1}};
    double output[4] = {0, 1, 1, 0};

    lfequal(output[0], *genann_run(ann, input[0]));
    lfequal(output[1], *genann_run(ann, input[1]));
    lfequal(output[2], *genann_run(ann, input[2]));
    lfequal(output[3], *genann_run(ann, input[3]));

    genann_free(ann);
    printf("test_xor passed\n");
}


void test_backprop() {
    genann *ann = genann_init(1, 0, 0, 1);

    double input, output;
    input = .5;
    output = 1;

    double first_try = *genann_run(ann, &input);
    genann_train(ann, &input, &output, .5);
    double second_try = *genann_run(ann, &input);
    lok(fabs(first_try - output) > fabs(second_try - output));

    genann_free(ann);
    printf("test_backprop passed\n");
}


void test_train_and() {
    double input[4][2] = {{0, 0}, {0, 1}, {1, 0}, {1, 1}};
    double output[4] = {0, 0, 0, 1};

    genann *ann = genann_init(2, 0, 0, 1);

    int i, j;
    for (i = 0; i < 50; ++i) {
        for (j = 0; j < 4; ++j) {
            genann_train(ann, input[j], output + j, .8);
        }
    }

    ann->activation_output = genann_act_threshold;
    lfequal(output[0], *genann_run(ann, input[0]));
    lfequal(output[1], *genann_run(ann, input[1]));
    lfequal(output[2], *genann_run(ann, input[2]));
    lfequal(output[3], *genann_run(ann, input[3]));

    genann_free(ann);
    printf("test_train_and passed\n");
}


void test_train_or() {
    double input[4][2] = {{0, 0}, {0, 1}, {1, 0}, {1, 1}};
    double output[4] = {0, 1, 1, 1};

    genann *ann = genann_init(2, 0, 0, 1);
    genann_randomize(ann);

    int i, j;
    for (i = 0; i < 50; ++i) {
        for (j = 0; j < 4; ++j) {
            genann_train(ann, input[j], output + j, .8);
        }
    }

    ann->activation_output = genann_act_threshold;
    lfequal(output[0], *genann_run(ann, input[0]));
    lfequal(output[1], *genann_run(ann, input[1]));
    lfequal(output[2], *genann_run(ann, input[2]));
    lfequal(output[3], *genann_run(ann, input[3]));

    genann_free(ann);
    printf("test_train_or passed\n");
}


void test_train_xor() {
    double input[4][2] = {{0, 0}, {0, 1}, {1, 0}, {1, 1}};
    double output[4] = {0, 1, 1, 0};

    genann *ann = genann_init(2, 1, 2, 1);

    int i, j;
    for (i = 0; i < 500; ++i) {
        for (j = 0; j < 4; ++j) {
            genann_train(ann, input[j], output + j, 3);
        }
    }

    ann->activation_output = genann_act_threshold;
    lfequal(output[0], *genann_run(ann, input[0]));
    lfequal(output[1], *genann_run(ann, input[1]));
    lfequal(output[2], *genann_run(ann, input[2]));
    lfequal(output[3], *genann_run(ann, input[3]));

    genann_free(ann);
    printf("test_train_xor passed\n");
}


void test_persist() {
    genann *first = genann_init(1000, 5, 50, 10);

    FILE *out = fopen("persist.txt", "w");
    genann_write(first, out);
    fclose(out);

    FILE *in = fopen("persist.txt", "r");
    genann *second = genann_read(in);
    fclose(in);

    lequal(first->inputs, second->inputs);
    lequal(first->hidden_layers, second->hidden_layers);
    lequal(first->hidden, second->hidden);
    lequal(first->outputs, second->outputs);
    lequal(first->total_weights, second->total_weights);

    int i;
    for (i = 0; i < first->total_weights; ++i) {
        lok(first->weight[i] == second->weight[i]);
    }

    genann_free(first);
    genann_free(second);
    printf("test_persist passed\n");
}


void test_copy() {
    genann *first = genann_init(1000, 5, 50, 10);

    genann *second = genann_copy(first);

    lequal(first->inputs, second->inputs);
    lequal(first->hidden_layers, second->hidden_layers);
    lequal(first->hidden, second->hidden);
    lequal(first->outputs, second->outputs);
    lequal(first->total_weights, second->total_weights);

    int i;
    for (i = 0; i < first->total_weights; ++i) {
        lfequal(first->weight[i], second->weight[i]);
    }

    genann_free(first);
    genann_free(second);
    printf("test_copy passed\n");
}


void test_sigmoid() {
    double i = -20;
    const double max = 20;
    const double d = .0001;

    while (i < max) {
        lfequal(genann_act_sigmoid(NULL, i), genann_act_sigmoid_cached(NULL, i));
        i += d;
    }
    printf("test_sigmoid passed\n");
}


int main(int argc, char *argv[]) {
    (void)argc; (void)argv;

    printf("GENANN TEST SUITE\n");

    srand(100); /* Repeatable test results. */

    test_basic();
    test_xor();
    test_backprop();
    test_train_and();
    test_train_or();
    test_train_xor();
    test_persist();
    test_copy();
    test_sigmoid();

    if (lfails == 0) {
        printf("ALL TESTS PASSED (%d/%d)\n", ltests, ltests);
    } else {
        printf("SOME TESTS FAILED (%d/%d)\n", ltests-lfails, ltests);
    }

    return lfails != 0;
}
