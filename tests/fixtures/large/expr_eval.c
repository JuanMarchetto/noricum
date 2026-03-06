/*
 * expr_eval.c — A simple expression evaluator with variables
 *
 * Features:
 *   - Lexer: tokenizes input into numbers, identifiers, operators, strings
 *   - Parser: recursive descent for expressions with operator precedence
 *   - Evaluator: supports +, -, *, /, %, ==, !=, <, >, <=, >=, &&, ||, !
 *   - Variables: let x = expr; assigns to a variable store
 *   - Built-in functions: abs(), min(), max(), sqrt(), print(), len()
 *   - Strings: basic string support with concatenation
 *   - REPL-style: reads lines from stdin, evaluates each one
 *
 * Self-contained single file, no external dependencies.
 * Designed as a migration test case for Noricum.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <math.h>

/* ========== Token types ========== */

typedef enum {
    TOK_NUMBER,
    TOK_STRING,
    TOK_IDENT,
    TOK_PLUS,
    TOK_MINUS,
    TOK_STAR,
    TOK_SLASH,
    TOK_PERCENT,
    TOK_LPAREN,
    TOK_RPAREN,
    TOK_COMMA,
    TOK_ASSIGN,
    TOK_EQ,
    TOK_NEQ,
    TOK_LT,
    TOK_GT,
    TOK_LTE,
    TOK_GTE,
    TOK_AND,
    TOK_OR,
    TOK_NOT,
    TOK_SEMICOLON,
    TOK_LET,
    TOK_IF,
    TOK_ELSE,
    TOK_WHILE,
    TOK_EOF,
    TOK_ERROR
} TokenType;

typedef struct {
    TokenType type;
    double num_val;
    char str_val[256];
} Token;

/* ========== Lexer ========== */

typedef struct {
    const char *src;
    int pos;
    int len;
} Lexer;

void lexer_init(Lexer *lex, const char *src) {
    lex->src = src;
    lex->pos = 0;
    lex->len = (int)strlen(src);
}

static void skip_whitespace(Lexer *lex) {
    while (lex->pos < lex->len && isspace((unsigned char)lex->src[lex->pos])) {
        lex->pos++;
    }
}

static void skip_comment(Lexer *lex) {
    if (lex->pos + 1 < lex->len && lex->src[lex->pos] == '/' && lex->src[lex->pos + 1] == '/') {
        while (lex->pos < lex->len && lex->src[lex->pos] != '\n') {
            lex->pos++;
        }
    }
}

static Token make_token(TokenType type) {
    Token t;
    t.type = type;
    t.num_val = 0;
    t.str_val[0] = '\0';
    return t;
}

static Token make_number(double val) {
    Token t;
    t.type = TOK_NUMBER;
    t.num_val = val;
    t.str_val[0] = '\0';
    return t;
}

static Token make_string(const char *s, int len) {
    Token t;
    t.type = TOK_STRING;
    t.num_val = 0;
    if (len > 255) len = 255;
    memcpy(t.str_val, s, len);
    t.str_val[len] = '\0';
    return t;
}

static Token make_ident(const char *s, int len) {
    Token t;
    t.num_val = 0;
    if (len > 255) len = 255;
    memcpy(t.str_val, s, len);
    t.str_val[len] = '\0';

    /* Check keywords */
    if (strcmp(t.str_val, "let") == 0) {
        t.type = TOK_LET;
    } else if (strcmp(t.str_val, "if") == 0) {
        t.type = TOK_IF;
    } else if (strcmp(t.str_val, "else") == 0) {
        t.type = TOK_ELSE;
    } else if (strcmp(t.str_val, "while") == 0) {
        t.type = TOK_WHILE;
    } else {
        t.type = TOK_IDENT;
    }
    return t;
}

Token lexer_next(Lexer *lex) {
    skip_whitespace(lex);
    skip_comment(lex);
    skip_whitespace(lex);

    if (lex->pos >= lex->len) {
        return make_token(TOK_EOF);
    }

    char c = lex->src[lex->pos];

    /* Numbers */
    if (isdigit((unsigned char)c) || (c == '.' && lex->pos + 1 < lex->len && isdigit((unsigned char)lex->src[lex->pos + 1]))) {
        int start = lex->pos;
        while (lex->pos < lex->len && (isdigit((unsigned char)lex->src[lex->pos]) || lex->src[lex->pos] == '.')) {
            lex->pos++;
        }
        char buf[64];
        int nlen = lex->pos - start;
        if (nlen > 63) nlen = 63;
        memcpy(buf, &lex->src[start], nlen);
        buf[nlen] = '\0';
        return make_number(atof(buf));
    }

    /* Strings */
    if (c == '"') {
        lex->pos++;
        int start = lex->pos;
        while (lex->pos < lex->len && lex->src[lex->pos] != '"') {
            if (lex->src[lex->pos] == '\\' && lex->pos + 1 < lex->len) {
                lex->pos++; /* skip escaped char */
            }
            lex->pos++;
        }
        Token t = make_string(&lex->src[start], lex->pos - start);
        if (lex->pos < lex->len) lex->pos++; /* skip closing quote */
        return t;
    }

    /* Identifiers and keywords */
    if (isalpha((unsigned char)c) || c == '_') {
        int start = lex->pos;
        while (lex->pos < lex->len && (isalnum((unsigned char)lex->src[lex->pos]) || lex->src[lex->pos] == '_')) {
            lex->pos++;
        }
        return make_ident(&lex->src[start], lex->pos - start);
    }

    /* Two-char operators */
    if (lex->pos + 1 < lex->len) {
        char c2 = lex->src[lex->pos + 1];
        if (c == '=' && c2 == '=') { lex->pos += 2; return make_token(TOK_EQ); }
        if (c == '!' && c2 == '=') { lex->pos += 2; return make_token(TOK_NEQ); }
        if (c == '<' && c2 == '=') { lex->pos += 2; return make_token(TOK_LTE); }
        if (c == '>' && c2 == '=') { lex->pos += 2; return make_token(TOK_GTE); }
        if (c == '&' && c2 == '&') { lex->pos += 2; return make_token(TOK_AND); }
        if (c == '|' && c2 == '|') { lex->pos += 2; return make_token(TOK_OR); }
    }

    /* Single-char operators */
    lex->pos++;
    switch (c) {
        case '+': return make_token(TOK_PLUS);
        case '-': return make_token(TOK_MINUS);
        case '*': return make_token(TOK_STAR);
        case '/': return make_token(TOK_SLASH);
        case '%': return make_token(TOK_PERCENT);
        case '(': return make_token(TOK_LPAREN);
        case ')': return make_token(TOK_RPAREN);
        case ',': return make_token(TOK_COMMA);
        case '=': return make_token(TOK_ASSIGN);
        case '<': return make_token(TOK_LT);
        case '>': return make_token(TOK_GT);
        case '!': return make_token(TOK_NOT);
        case ';': return make_token(TOK_SEMICOLON);
        default: {
            Token t = make_token(TOK_ERROR);
            t.str_val[0] = c;
            t.str_val[1] = '\0';
            return t;
        }
    }
}

Token lexer_peek(Lexer *lex) {
    int saved_pos = lex->pos;
    Token t = lexer_next(lex);
    lex->pos = saved_pos;
    return t;
}

/* ========== Value type ========== */

typedef enum {
    VAL_NUMBER,
    VAL_STRING,
    VAL_BOOL,
    VAL_NONE
} ValueType;

typedef struct {
    ValueType type;
    double num;
    char str[256];
} Value;

static Value val_number(double n) {
    Value v;
    v.type = VAL_NUMBER;
    v.num = n;
    v.str[0] = '\0';
    return v;
}

static Value val_string(const char *s) {
    Value v;
    v.type = VAL_STRING;
    v.num = 0;
    strncpy(v.str, s, 255);
    v.str[255] = '\0';
    return v;
}

static Value val_bool(int b) {
    Value v;
    v.type = VAL_BOOL;
    v.num = b ? 1.0 : 0.0;
    v.str[0] = '\0';
    return v;
}

static Value val_none(void) {
    Value v;
    v.type = VAL_NONE;
    v.num = 0;
    v.str[0] = '\0';
    return v;
}

static int val_is_truthy(Value v) {
    switch (v.type) {
        case VAL_NUMBER: return v.num != 0.0;
        case VAL_STRING: return v.str[0] != '\0';
        case VAL_BOOL:   return v.num != 0.0;
        case VAL_NONE:   return 0;
    }
    return 0;
}

static void val_print(Value v) {
    switch (v.type) {
        case VAL_NUMBER:
            if (v.num == (int)v.num) {
                printf("%d", (int)v.num);
            } else {
                printf("%.6g", v.num);
            }
            break;
        case VAL_STRING:
            printf("%s", v.str);
            break;
        case VAL_BOOL:
            printf("%s", v.num != 0.0 ? "true" : "false");
            break;
        case VAL_NONE:
            printf("none");
            break;
    }
}

/* ========== Variable store ========== */

#define MAX_VARS 256

typedef struct {
    char name[64];
    Value value;
} Variable;

typedef struct {
    Variable vars[MAX_VARS];
    int count;
} VarStore;

static VarStore global_vars;

void var_store_init(VarStore *store) {
    store->count = 0;
}

Value var_get(VarStore *store, const char *name) {
    for (int i = 0; i < store->count; i++) {
        if (strcmp(store->vars[i].name, name) == 0) {
            return store->vars[i].value;
        }
    }
    return val_none();
}

void var_set(VarStore *store, const char *name, Value val) {
    /* Update existing */
    for (int i = 0; i < store->count; i++) {
        if (strcmp(store->vars[i].name, name) == 0) {
            store->vars[i].value = val;
            return;
        }
    }
    /* Insert new */
    if (store->count < MAX_VARS) {
        strncpy(store->vars[store->count].name, name, 63);
        store->vars[store->count].name[63] = '\0';
        store->vars[store->count].value = val;
        store->count++;
    } else {
        fprintf(stderr, "Error: variable store full\n");
    }
}

/* ========== String utilities ========== */

static char *string_repeat(const char *s, int times) {
    int slen = (int)strlen(s);
    int total = slen * times;
    char *result = (char *)malloc(total + 1);
    if (!result) return NULL;
    result[0] = '\0';
    for (int i = 0; i < times; i++) {
        strcat(result, s);
    }
    return result;
}

static char *string_reverse(const char *s) {
    int len = (int)strlen(s);
    char *result = (char *)malloc(len + 1);
    if (!result) return NULL;
    for (int i = 0; i < len; i++) {
        result[i] = s[len - 1 - i];
    }
    result[len] = '\0';
    return result;
}

static char *string_upper(const char *s) {
    int len = (int)strlen(s);
    char *result = (char *)malloc(len + 1);
    if (!result) return NULL;
    for (int i = 0; i < len; i++) {
        result[i] = (char)toupper((unsigned char)s[i]);
    }
    result[len] = '\0';
    return result;
}

static char *string_lower(const char *s) {
    int len = (int)strlen(s);
    char *result = (char *)malloc(len + 1);
    if (!result) return NULL;
    for (int i = 0; i < len; i++) {
        result[i] = (char)tolower((unsigned char)s[i]);
    }
    result[len] = '\0';
    return result;
}

static char *string_trim(const char *s) {
    int len = (int)strlen(s);
    int start = 0, end = len - 1;
    while (start < len && isspace((unsigned char)s[start])) start++;
    while (end > start && isspace((unsigned char)s[end])) end--;
    int new_len = end - start + 1;
    char *result = (char *)malloc(new_len + 1);
    if (!result) return NULL;
    memcpy(result, &s[start], new_len);
    result[new_len] = '\0';
    return result;
}

static int string_index_of(const char *haystack, const char *needle) {
    const char *found = strstr(haystack, needle);
    if (!found) return -1;
    return (int)(found - haystack);
}

static char *string_substring(const char *s, int start, int end) {
    int len = (int)strlen(s);
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) {
        char *empty = (char *)malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }
    int new_len = end - start;
    char *result = (char *)malloc(new_len + 1);
    if (!result) return NULL;
    memcpy(result, &s[start], new_len);
    result[new_len] = '\0';
    return result;
}

/* ========== Dynamic array for function args ========== */

#define MAX_ARGS 16

typedef struct {
    Value items[MAX_ARGS];
    int count;
} ArgList;

static void arglist_init(ArgList *args) {
    args->count = 0;
}

static void arglist_push(ArgList *args, Value v) {
    if (args->count < MAX_ARGS) {
        args->items[args->count++] = v;
    }
}

/* ========== Parser + Evaluator ========== */

/*
 * Grammar (simplified):
 *   statement   = let_stmt | if_stmt | while_stmt | expr_stmt
 *   let_stmt    = "let" IDENT "=" expr ";"
 *   if_stmt     = "if" "(" expr ")" statement ["else" statement]
 *   while_stmt  = "while" "(" expr ")" statement
 *   expr_stmt   = expr ";"
 *   expr        = assign_expr
 *   assign_expr = IDENT "=" assign_expr | or_expr
 *   or_expr     = and_expr ("||" and_expr)*
 *   and_expr    = eq_expr ("&&" eq_expr)*
 *   eq_expr     = cmp_expr (("==" | "!=") cmp_expr)*
 *   cmp_expr    = add_expr (("<" | ">" | "<=" | ">=") add_expr)*
 *   add_expr    = mul_expr (("+" | "-") mul_expr)*
 *   mul_expr    = unary_expr (("*" | "/" | "%") unary_expr)*
 *   unary_expr  = ("!" | "-") unary_expr | call_expr
 *   call_expr   = primary ["(" arglist ")"]
 *   primary     = NUMBER | STRING | IDENT | "(" expr ")"
 */

typedef struct {
    Lexer lex;
    Token current;
    int error;
    char error_msg[256];
} Parser;

void parser_init(Parser *p, const char *src) {
    lexer_init(&p->lex, src);
    p->current = lexer_next(&p->lex);
    p->error = 0;
    p->error_msg[0] = '\0';
}

static void parser_error(Parser *p, const char *msg) {
    if (!p->error) {
        p->error = 1;
        strncpy(p->error_msg, msg, 255);
        p->error_msg[255] = '\0';
    }
}

static void parser_advance(Parser *p) {
    p->current = lexer_next(&p->lex);
}

static int parser_expect(Parser *p, TokenType type) {
    if (p->current.type == type) {
        parser_advance(p);
        return 1;
    }
    char msg[128];
    snprintf(msg, sizeof(msg), "Expected token type %d, got %d", type, p->current.type);
    parser_error(p, msg);
    return 0;
}

/* Forward declarations */
static Value parse_expr(Parser *p);
static Value parse_statement(Parser *p);

static Value call_builtin(const char *name, ArgList *args) {
    if (strcmp(name, "abs") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_NUMBER) {
            return val_number(fabs(args->items[0].num));
        }
    }
    if (strcmp(name, "sqrt") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_NUMBER) {
            return val_number(sqrt(args->items[0].num));
        }
    }
    if (strcmp(name, "min") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_NUMBER && args->items[1].type == VAL_NUMBER) {
            double a = args->items[0].num, b = args->items[1].num;
            return val_number(a < b ? a : b);
        }
    }
    if (strcmp(name, "max") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_NUMBER && args->items[1].type == VAL_NUMBER) {
            double a = args->items[0].num, b = args->items[1].num;
            return val_number(a > b ? a : b);
        }
    }
    if (strcmp(name, "pow") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_NUMBER && args->items[1].type == VAL_NUMBER) {
            return val_number(pow(args->items[0].num, args->items[1].num));
        }
    }
    if (strcmp(name, "floor") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_NUMBER) {
            return val_number(floor(args->items[0].num));
        }
    }
    if (strcmp(name, "ceil") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_NUMBER) {
            return val_number(ceil(args->items[0].num));
        }
    }
    if (strcmp(name, "round") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_NUMBER) {
            return val_number(round(args->items[0].num));
        }
    }
    if (strcmp(name, "print") == 0) {
        for (int i = 0; i < args->count; i++) {
            if (i > 0) printf(" ");
            val_print(args->items[i]);
        }
        printf("\n");
        return val_none();
    }
    if (strcmp(name, "println") == 0) {
        for (int i = 0; i < args->count; i++) {
            if (i > 0) printf(" ");
            val_print(args->items[i]);
        }
        printf("\n");
        return val_none();
    }
    if (strcmp(name, "len") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            return val_number((double)strlen(args->items[0].str));
        }
    }
    if (strcmp(name, "type") == 0 && args->count == 1) {
        switch (args->items[0].type) {
            case VAL_NUMBER: return val_string("number");
            case VAL_STRING: return val_string("string");
            case VAL_BOOL:   return val_string("bool");
            case VAL_NONE:   return val_string("none");
        }
    }
    if (strcmp(name, "str") == 0 && args->count == 1) {
        char buf[256];
        if (args->items[0].type == VAL_NUMBER) {
            if (args->items[0].num == (int)args->items[0].num) {
                snprintf(buf, sizeof(buf), "%d", (int)args->items[0].num);
            } else {
                snprintf(buf, sizeof(buf), "%.6g", args->items[0].num);
            }
            return val_string(buf);
        }
        return args->items[0];
    }
    if (strcmp(name, "num") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            return val_number(atof(args->items[0].str));
        }
        if (args->items[0].type == VAL_NUMBER) {
            return args->items[0];
        }
    }
    if (strcmp(name, "upper") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            char *r = string_upper(args->items[0].str);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "lower") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            char *r = string_lower(args->items[0].str);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "trim") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            char *r = string_trim(args->items[0].str);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "reverse") == 0 && args->count == 1) {
        if (args->items[0].type == VAL_STRING) {
            char *r = string_reverse(args->items[0].str);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "repeat") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_NUMBER) {
            char *r = string_repeat(args->items[0].str, (int)args->items[1].num);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "index_of") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_STRING) {
            return val_number((double)string_index_of(args->items[0].str, args->items[1].str));
        }
    }
    if (strcmp(name, "substring") == 0 && args->count == 3) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_NUMBER && args->items[2].type == VAL_NUMBER) {
            char *r = string_substring(args->items[0].str, (int)args->items[1].num, (int)args->items[2].num);
            Value v = val_string(r ? r : "");
            free(r);
            return v;
        }
    }
    if (strcmp(name, "contains") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_STRING) {
            return val_bool(strstr(args->items[0].str, args->items[1].str) != NULL);
        }
    }
    if (strcmp(name, "starts_with") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_STRING) {
            return val_bool(strncmp(args->items[0].str, args->items[1].str, strlen(args->items[1].str)) == 0);
        }
    }
    if (strcmp(name, "ends_with") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_STRING) {
            int hlen = (int)strlen(args->items[0].str);
            int nlen = (int)strlen(args->items[1].str);
            if (nlen > hlen) return val_bool(0);
            return val_bool(strcmp(&args->items[0].str[hlen - nlen], args->items[1].str) == 0);
        }
    }
    if (strcmp(name, "char_at") == 0 && args->count == 2) {
        if (args->items[0].type == VAL_STRING && args->items[1].type == VAL_NUMBER) {
            int idx = (int)args->items[1].num;
            int slen = (int)strlen(args->items[0].str);
            if (idx >= 0 && idx < slen) {
                char buf[2] = { args->items[0].str[idx], '\0' };
                return val_string(buf);
            }
            return val_string("");
        }
    }

    fprintf(stderr, "Error: unknown function '%s' with %d args\n", name, args->count);
    return val_none();
}

static Value parse_primary(Parser *p) {
    if (p->error) return val_none();

    Token t = p->current;

    if (t.type == TOK_NUMBER) {
        parser_advance(p);
        return val_number(t.num_val);
    }

    if (t.type == TOK_STRING) {
        parser_advance(p);
        return val_string(t.str_val);
    }

    if (t.type == TOK_IDENT) {
        char name[256];
        strcpy(name, t.str_val);
        parser_advance(p);

        /* Function call */
        if (p->current.type == TOK_LPAREN) {
            parser_advance(p); /* skip ( */
            ArgList args;
            arglist_init(&args);

            if (p->current.type != TOK_RPAREN) {
                arglist_push(&args, parse_expr(p));
                while (p->current.type == TOK_COMMA) {
                    parser_advance(p);
                    arglist_push(&args, parse_expr(p));
                }
            }
            parser_expect(p, TOK_RPAREN);
            return call_builtin(name, &args);
        }

        /* Variable reference */
        return var_get(&global_vars, name);
    }

    if (t.type == TOK_LPAREN) {
        parser_advance(p);
        Value v = parse_expr(p);
        parser_expect(p, TOK_RPAREN);
        return v;
    }

    parser_error(p, "Unexpected token in expression");
    return val_none();
}

static Value parse_unary(Parser *p) {
    if (p->error) return val_none();

    if (p->current.type == TOK_MINUS) {
        parser_advance(p);
        Value v = parse_unary(p);
        if (v.type == VAL_NUMBER) {
            return val_number(-v.num);
        }
        parser_error(p, "Cannot negate non-number");
        return val_none();
    }

    if (p->current.type == TOK_NOT) {
        parser_advance(p);
        Value v = parse_unary(p);
        return val_bool(!val_is_truthy(v));
    }

    return parse_primary(p);
}

static Value parse_mul(Parser *p) {
    Value left = parse_unary(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_STAR || p->current.type == TOK_SLASH || p->current.type == TOK_PERCENT) {
        TokenType op = p->current.type;
        parser_advance(p);
        Value right = parse_unary(p);
        if (p->error) return val_none();

        if (left.type != VAL_NUMBER || right.type != VAL_NUMBER) {
            parser_error(p, "Arithmetic requires numbers");
            return val_none();
        }

        if (op == TOK_STAR) {
            left = val_number(left.num * right.num);
        } else if (op == TOK_SLASH) {
            if (right.num == 0.0) {
                parser_error(p, "Division by zero");
                return val_none();
            }
            left = val_number(left.num / right.num);
        } else {
            if (right.num == 0.0) {
                parser_error(p, "Modulo by zero");
                return val_none();
            }
            left = val_number((double)((int)left.num % (int)right.num));
        }
    }
    return left;
}

static Value parse_add(Parser *p) {
    Value left = parse_mul(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_PLUS || p->current.type == TOK_MINUS) {
        TokenType op = p->current.type;
        parser_advance(p);
        Value right = parse_mul(p);
        if (p->error) return val_none();

        if (op == TOK_PLUS) {
            /* String concatenation */
            if (left.type == VAL_STRING || right.type == VAL_STRING) {
                char buf[512];
                char lbuf[256], rbuf[256];

                if (left.type == VAL_STRING) {
                    strcpy(lbuf, left.str);
                } else if (left.type == VAL_NUMBER) {
                    if (left.num == (int)left.num)
                        snprintf(lbuf, sizeof(lbuf), "%d", (int)left.num);
                    else
                        snprintf(lbuf, sizeof(lbuf), "%.6g", left.num);
                } else {
                    strcpy(lbuf, "");
                }

                if (right.type == VAL_STRING) {
                    strcpy(rbuf, right.str);
                } else if (right.type == VAL_NUMBER) {
                    if (right.num == (int)right.num)
                        snprintf(rbuf, sizeof(rbuf), "%d", (int)right.num);
                    else
                        snprintf(rbuf, sizeof(rbuf), "%.6g", right.num);
                } else {
                    strcpy(rbuf, "");
                }

                snprintf(buf, sizeof(buf), "%s%s", lbuf, rbuf);
                left = val_string(buf);
                continue;
            }
            if (left.type != VAL_NUMBER || right.type != VAL_NUMBER) {
                parser_error(p, "Addition requires numbers or strings");
                return val_none();
            }
            left = val_number(left.num + right.num);
        } else {
            if (left.type != VAL_NUMBER || right.type != VAL_NUMBER) {
                parser_error(p, "Subtraction requires numbers");
                return val_none();
            }
            left = val_number(left.num - right.num);
        }
    }
    return left;
}

static Value parse_comparison(Parser *p) {
    Value left = parse_add(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_LT || p->current.type == TOK_GT ||
           p->current.type == TOK_LTE || p->current.type == TOK_GTE) {
        TokenType op = p->current.type;
        parser_advance(p);
        Value right = parse_add(p);
        if (p->error) return val_none();

        if (left.type == VAL_NUMBER && right.type == VAL_NUMBER) {
            int result = 0;
            if (op == TOK_LT)  result = left.num < right.num;
            if (op == TOK_GT)  result = left.num > right.num;
            if (op == TOK_LTE) result = left.num <= right.num;
            if (op == TOK_GTE) result = left.num >= right.num;
            left = val_bool(result);
        } else if (left.type == VAL_STRING && right.type == VAL_STRING) {
            int cmp = strcmp(left.str, right.str);
            int result = 0;
            if (op == TOK_LT)  result = cmp < 0;
            if (op == TOK_GT)  result = cmp > 0;
            if (op == TOK_LTE) result = cmp <= 0;
            if (op == TOK_GTE) result = cmp >= 0;
            left = val_bool(result);
        } else {
            parser_error(p, "Cannot compare these types");
            return val_none();
        }
    }
    return left;
}

static Value parse_equality(Parser *p) {
    Value left = parse_comparison(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_EQ || p->current.type == TOK_NEQ) {
        TokenType op = p->current.type;
        parser_advance(p);
        Value right = parse_comparison(p);
        if (p->error) return val_none();

        int equal = 0;
        if (left.type == right.type) {
            if (left.type == VAL_NUMBER || left.type == VAL_BOOL) {
                equal = left.num == right.num;
            } else if (left.type == VAL_STRING) {
                equal = strcmp(left.str, right.str) == 0;
            } else {
                equal = 1; /* both none */
            }
        }

        left = val_bool(op == TOK_EQ ? equal : !equal);
    }
    return left;
}

static Value parse_and(Parser *p) {
    Value left = parse_equality(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_AND) {
        parser_advance(p);
        if (!val_is_truthy(left)) {
            /* Short circuit: skip right side but still parse it */
            parse_equality(p);
            left = val_bool(0);
        } else {
            Value right = parse_equality(p);
            left = val_bool(val_is_truthy(right));
        }
    }
    return left;
}

static Value parse_or(Parser *p) {
    Value left = parse_and(p);
    if (p->error) return val_none();

    while (p->current.type == TOK_OR) {
        parser_advance(p);
        if (val_is_truthy(left)) {
            /* Short circuit */
            parse_and(p);
            left = val_bool(1);
        } else {
            Value right = parse_and(p);
            left = val_bool(val_is_truthy(right));
        }
    }
    return left;
}

static Value parse_expr(Parser *p) {
    if (p->error) return val_none();

    /* Check for assignment: IDENT = expr */
    if (p->current.type == TOK_IDENT) {
        /* Peek ahead to check for = (not ==) */
        int saved_pos = p->lex.pos;
        Token saved_current = p->current;
        char name[256];
        strcpy(name, p->current.str_val);

        parser_advance(p);
        if (p->current.type == TOK_ASSIGN) {
            parser_advance(p);
            Value val = parse_expr(p);
            var_set(&global_vars, name, val);
            return val;
        }

        /* Not an assignment, restore state */
        p->lex.pos = saved_pos;
        p->current = saved_current;
    }

    return parse_or(p);
}

static Value parse_statement(Parser *p) {
    if (p->error) return val_none();

    /* let statement */
    if (p->current.type == TOK_LET) {
        parser_advance(p);
        if (p->current.type != TOK_IDENT) {
            parser_error(p, "Expected variable name after 'let'");
            return val_none();
        }
        char name[256];
        strcpy(name, p->current.str_val);
        parser_advance(p);

        if (!parser_expect(p, TOK_ASSIGN)) return val_none();
        Value val = parse_expr(p);
        if (p->current.type == TOK_SEMICOLON) parser_advance(p);
        var_set(&global_vars, name, val);
        return val;
    }

    /* if statement */
    if (p->current.type == TOK_IF) {
        parser_advance(p);
        if (!parser_expect(p, TOK_LPAREN)) return val_none();
        Value cond = parse_expr(p);
        if (!parser_expect(p, TOK_RPAREN)) return val_none();

        if (val_is_truthy(cond)) {
            Value result = parse_statement(p);
            /* Skip else branch if present */
            if (p->current.type == TOK_ELSE) {
                parser_advance(p);
                /* Parse and discard else branch */
                parse_statement(p);
            }
            return result;
        } else {
            /* Skip then branch */
            parse_statement(p); /* parse and discard */
            if (p->current.type == TOK_ELSE) {
                parser_advance(p);
                return parse_statement(p);
            }
            return val_none();
        }
    }

    /* while statement */
    if (p->current.type == TOK_WHILE) {
        parser_advance(p);
        if (!parser_expect(p, TOK_LPAREN)) return val_none();

        /* Save position for loop */
        int cond_pos = p->lex.pos;
        Token cond_tok = p->current;
        Value last = val_none();
        int iterations = 0;
        int max_iterations = 10000;

        while (1) {
            /* Re-parse condition */
            p->lex.pos = cond_pos;
            p->current = cond_tok;

            Value cond = parse_expr(p);
            if (p->error) return val_none();
            if (!parser_expect(p, TOK_RPAREN)) return val_none();

            if (!val_is_truthy(cond)) {
                /* Skip body one more time to advance past it */
                parse_statement(p);
                break;
            }

            last = parse_statement(p);
            iterations++;

            if (iterations >= max_iterations) {
                parser_error(p, "While loop exceeded maximum iterations");
                return val_none();
            }

            /* Save position after body for next iteration condition */
            /* Actually we need to re-evaluate from the saved cond position */
        }
        return last;
    }

    /* Expression statement */
    Value val = parse_expr(p);
    if (p->current.type == TOK_SEMICOLON) parser_advance(p);
    return val;
}

/* ========== Array-based stack for computation history ========== */

#define HISTORY_SIZE 100

typedef struct {
    Value items[HISTORY_SIZE];
    int count;
} History;

static History global_history;

void history_init(History *h) {
    h->count = 0;
}

void history_push(History *h, Value v) {
    if (h->count < HISTORY_SIZE) {
        h->items[h->count++] = v;
    }
}

Value history_get(History *h, int index) {
    if (index >= 0 && index < h->count) {
        return h->items[index];
    }
    return val_none();
}

/* ========== Evaluate a line of input ========== */

Value eval_line(const char *line) {
    Parser p;
    parser_init(&p, line);

    Value last = val_none();
    while (p.current.type != TOK_EOF && !p.error) {
        last = parse_statement(&p);
    }

    if (p.error) {
        fprintf(stderr, "Error: %s\n", p.error_msg);
        return val_none();
    }

    return last;
}

/* ========== Statistical functions ========== */

typedef struct {
    double *data;
    int count;
    int capacity;
} DataSet;

DataSet *dataset_create(void) {
    DataSet *ds = (DataSet *)malloc(sizeof(DataSet));
    if (!ds) return NULL;
    ds->data = (double *)malloc(sizeof(double) * 16);
    if (!ds->data) { free(ds); return NULL; }
    ds->count = 0;
    ds->capacity = 16;
    return ds;
}

void dataset_push(DataSet *ds, double val) {
    if (ds->count >= ds->capacity) {
        int new_cap = ds->capacity * 2;
        double *new_data = (double *)realloc(ds->data, sizeof(double) * new_cap);
        if (!new_data) return;
        ds->data = new_data;
        ds->capacity = new_cap;
    }
    ds->data[ds->count++] = val;
}

double dataset_mean(DataSet *ds) {
    if (ds->count == 0) return 0.0;
    double sum = 0.0;
    for (int i = 0; i < ds->count; i++) {
        sum += ds->data[i];
    }
    return sum / ds->count;
}

double dataset_variance(DataSet *ds) {
    if (ds->count < 2) return 0.0;
    double mean = dataset_mean(ds);
    double sum_sq = 0.0;
    for (int i = 0; i < ds->count; i++) {
        double diff = ds->data[i] - mean;
        sum_sq += diff * diff;
    }
    return sum_sq / (ds->count - 1);
}

double dataset_stddev(DataSet *ds) {
    return sqrt(dataset_variance(ds));
}

double dataset_min(DataSet *ds) {
    if (ds->count == 0) return 0.0;
    double m = ds->data[0];
    for (int i = 1; i < ds->count; i++) {
        if (ds->data[i] < m) m = ds->data[i];
    }
    return m;
}

double dataset_max(DataSet *ds) {
    if (ds->count == 0) return 0.0;
    double m = ds->data[0];
    for (int i = 1; i < ds->count; i++) {
        if (ds->data[i] > m) m = ds->data[i];
    }
    return m;
}

static int cmp_double(const void *a, const void *b) {
    double da = *(const double *)a;
    double db = *(const double *)b;
    if (da < db) return -1;
    if (da > db) return 1;
    return 0;
}

double dataset_median(DataSet *ds) {
    if (ds->count == 0) return 0.0;

    /* Sort a copy */
    double *sorted = (double *)malloc(sizeof(double) * ds->count);
    if (!sorted) return 0.0;
    memcpy(sorted, ds->data, sizeof(double) * ds->count);
    qsort(sorted, ds->count, sizeof(double), cmp_double);

    double result;
    if (ds->count % 2 == 0) {
        result = (sorted[ds->count / 2 - 1] + sorted[ds->count / 2]) / 2.0;
    } else {
        result = sorted[ds->count / 2];
    }
    free(sorted);
    return result;
}

void dataset_free(DataSet *ds) {
    if (ds) {
        free(ds->data);
        free(ds);
    }
}

/* ========== Simple hash map (string -> int) ========== */

#define MAP_BUCKETS 64

typedef struct MapEntry {
    char *key;
    int value;
    struct MapEntry *next;
} MapEntry;

typedef struct {
    MapEntry *buckets[MAP_BUCKETS];
    int size;
} HashMap;

HashMap *hashmap_create(void) {
    HashMap *map = (HashMap *)calloc(1, sizeof(HashMap));
    return map;
}

static unsigned int hash_str(const char *s) {
    unsigned int h = 5381;
    while (*s) {
        h = ((h << 5) + h) + (unsigned char)*s;
        s++;
    }
    return h;
}

void hashmap_set(HashMap *map, const char *key, int value) {
    unsigned int idx = hash_str(key) % MAP_BUCKETS;
    MapEntry *entry = map->buckets[idx];

    /* Check existing */
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            entry->value = value;
            return;
        }
        entry = entry->next;
    }

    /* Insert new */
    MapEntry *new_entry = (MapEntry *)malloc(sizeof(MapEntry));
    if (!new_entry) return;
    new_entry->key = strdup(key);
    if (!new_entry->key) { free(new_entry); return; }
    new_entry->value = value;
    new_entry->next = map->buckets[idx];
    map->buckets[idx] = new_entry;
    map->size++;
}

int hashmap_get(HashMap *map, const char *key, int *found) {
    unsigned int idx = hash_str(key) % MAP_BUCKETS;
    MapEntry *entry = map->buckets[idx];

    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            if (found) *found = 1;
            return entry->value;
        }
        entry = entry->next;
    }
    if (found) *found = 0;
    return 0;
}

int hashmap_contains(HashMap *map, const char *key) {
    int found = 0;
    hashmap_get(map, key, &found);
    return found;
}

void hashmap_remove(HashMap *map, const char *key) {
    unsigned int idx = hash_str(key) % MAP_BUCKETS;
    MapEntry **pp = &map->buckets[idx];

    while (*pp) {
        if (strcmp((*pp)->key, key) == 0) {
            MapEntry *old = *pp;
            *pp = old->next;
            free(old->key);
            free(old);
            map->size--;
            return;
        }
        pp = &(*pp)->next;
    }
}

void hashmap_free(HashMap *map) {
    if (!map) return;
    for (int i = 0; i < MAP_BUCKETS; i++) {
        MapEntry *entry = map->buckets[i];
        while (entry) {
            MapEntry *next = entry->next;
            free(entry->key);
            free(entry);
            entry = next;
        }
    }
    free(map);
}

/* ========== Main: test program ========== */

static void test_basic_arithmetic(void) {
    printf("=== Basic Arithmetic ===\n");
    Value r;

    r = eval_line("2 + 3");
    printf("2 + 3 = "); val_print(r); printf("\n");

    r = eval_line("10 - 4 * 2");
    printf("10 - 4 * 2 = "); val_print(r); printf("\n");

    r = eval_line("(10 - 4) * 2");
    printf("(10 - 4) * 2 = "); val_print(r); printf("\n");

    r = eval_line("100 / 7");
    printf("100 / 7 = "); val_print(r); printf("\n");

    r = eval_line("17 % 5");
    printf("17 %% 5 = "); val_print(r); printf("\n");

    r = eval_line("-42");
    printf("-42 = "); val_print(r); printf("\n");

    r = eval_line("2 + 3 * 4 - 1");
    printf("2 + 3 * 4 - 1 = "); val_print(r); printf("\n");
}

static void test_comparisons(void) {
    printf("\n=== Comparisons ===\n");
    Value r;

    r = eval_line("5 > 3");
    printf("5 > 3 = "); val_print(r); printf("\n");

    r = eval_line("3 >= 3");
    printf("3 >= 3 = "); val_print(r); printf("\n");

    r = eval_line("2 < 1");
    printf("2 < 1 = "); val_print(r); printf("\n");

    r = eval_line("10 == 10");
    printf("10 == 10 = "); val_print(r); printf("\n");

    r = eval_line("10 != 5");
    printf("10 != 5 = "); val_print(r); printf("\n");

    r = eval_line("!0");
    printf("!0 = "); val_print(r); printf("\n");

    r = eval_line("!1");
    printf("!1 = "); val_print(r); printf("\n");
}

static void test_logical_ops(void) {
    printf("\n=== Logical Operations ===\n");
    Value r;

    r = eval_line("1 && 1");
    printf("1 && 1 = "); val_print(r); printf("\n");

    r = eval_line("1 && 0");
    printf("1 && 0 = "); val_print(r); printf("\n");

    r = eval_line("0 || 1");
    printf("0 || 1 = "); val_print(r); printf("\n");

    r = eval_line("0 || 0");
    printf("0 || 0 = "); val_print(r); printf("\n");

    r = eval_line("(5 > 3) && (10 < 20)");
    printf("(5 > 3) && (10 < 20) = "); val_print(r); printf("\n");
}

static void test_variables(void) {
    printf("\n=== Variables ===\n");
    Value r;

    eval_line("let x = 42;");
    r = eval_line("x");
    printf("x = "); val_print(r); printf("\n");

    eval_line("let y = x * 2;");
    r = eval_line("y");
    printf("y = x * 2 = "); val_print(r); printf("\n");

    eval_line("x = 100;");
    r = eval_line("x");
    printf("x reassigned = "); val_print(r); printf("\n");

    eval_line("let sum = x + y;");
    r = eval_line("sum");
    printf("sum = x + y = "); val_print(r); printf("\n");
}

static void test_strings(void) {
    printf("\n=== Strings ===\n");
    Value r;

    r = eval_line("\"hello\"");
    printf("literal = "); val_print(r); printf("\n");

    r = eval_line("\"hello\" + \" \" + \"world\"");
    printf("concat = "); val_print(r); printf("\n");

    r = eval_line("\"count: \" + 42");
    printf("str + num = "); val_print(r); printf("\n");

    r = eval_line("len(\"hello\")");
    printf("len(\"hello\") = "); val_print(r); printf("\n");

    r = eval_line("upper(\"hello\")");
    printf("upper(\"hello\") = "); val_print(r); printf("\n");

    r = eval_line("lower(\"WORLD\")");
    printf("lower(\"WORLD\") = "); val_print(r); printf("\n");

    r = eval_line("reverse(\"abcde\")");
    printf("reverse(\"abcde\") = "); val_print(r); printf("\n");

    r = eval_line("trim(\"  spaces  \")");
    printf("trim(\"  spaces  \") = "); val_print(r); printf("\n");

    r = eval_line("repeat(\"ab\", 3)");
    printf("repeat(\"ab\", 3) = "); val_print(r); printf("\n");

    r = eval_line("contains(\"hello world\", \"world\")");
    printf("contains(\"hello world\", \"world\") = "); val_print(r); printf("\n");

    r = eval_line("starts_with(\"hello\", \"hel\")");
    printf("starts_with(\"hello\", \"hel\") = "); val_print(r); printf("\n");

    r = eval_line("ends_with(\"hello\", \"llo\")");
    printf("ends_with(\"hello\", \"llo\") = "); val_print(r); printf("\n");

    r = eval_line("index_of(\"hello world\", \"world\")");
    printf("index_of(\"hello world\", \"world\") = "); val_print(r); printf("\n");

    r = eval_line("substring(\"hello world\", 0, 5)");
    printf("substring(\"hello world\", 0, 5) = "); val_print(r); printf("\n");

    r = eval_line("char_at(\"abcde\", 2)");
    printf("char_at(\"abcde\", 2) = "); val_print(r); printf("\n");

    r = eval_line("\"abc\" < \"def\"");
    printf("\"abc\" < \"def\" = "); val_print(r); printf("\n");

    r = eval_line("\"hello\" == \"hello\"");
    printf("\"hello\" == \"hello\" = "); val_print(r); printf("\n");
}

static void test_builtins(void) {
    printf("\n=== Built-in Functions ===\n");
    Value r;

    r = eval_line("abs(-7)");
    printf("abs(-7) = "); val_print(r); printf("\n");

    r = eval_line("sqrt(144)");
    printf("sqrt(144) = "); val_print(r); printf("\n");

    r = eval_line("min(3, 7)");
    printf("min(3, 7) = "); val_print(r); printf("\n");

    r = eval_line("max(3, 7)");
    printf("max(3, 7) = "); val_print(r); printf("\n");

    r = eval_line("pow(2, 10)");
    printf("pow(2, 10) = "); val_print(r); printf("\n");

    r = eval_line("floor(3.7)");
    printf("floor(3.7) = "); val_print(r); printf("\n");

    r = eval_line("ceil(3.2)");
    printf("ceil(3.2) = "); val_print(r); printf("\n");

    r = eval_line("round(3.5)");
    printf("round(3.5) = "); val_print(r); printf("\n");

    r = eval_line("type(42)");
    printf("type(42) = "); val_print(r); printf("\n");

    r = eval_line("type(\"hi\")");
    printf("type(\"hi\") = "); val_print(r); printf("\n");

    r = eval_line("str(123)");
    printf("str(123) = "); val_print(r); printf("\n");

    r = eval_line("num(\"456\")");
    printf("num(\"456\") = "); val_print(r); printf("\n");
}

static void test_conditionals(void) {
    printf("\n=== Conditionals ===\n");

    var_store_init(&global_vars);
    eval_line("let x = 10;");

    eval_line("if (x > 5) print(\"x is big\");");
    eval_line("if (x < 5) print(\"x is small\"); else print(\"x is not small\");");

    eval_line("let grade = 85;");
    eval_line("if (grade >= 90) print(\"A\"); else if (grade >= 80) print(\"B\"); else if (grade >= 70) print(\"C\"); else print(\"F\");");
}

static void test_complex_expressions(void) {
    printf("\n=== Complex Expressions ===\n");
    Value r;

    var_store_init(&global_vars);

    /* Chained assignments */
    eval_line("let a = 5;");
    eval_line("let b = a * 2 + 3;");
    eval_line("let c = b - a;");
    r = eval_line("c");
    printf("c = (5*2+3) - 5 = "); val_print(r); printf("\n");

    /* Nested function calls */
    r = eval_line("max(min(10, 20), min(5, 15))");
    printf("max(min(10,20), min(5,15)) = "); val_print(r); printf("\n");

    /* String + number expressions */
    eval_line("let name = \"world\";");
    r = eval_line("\"hello \" + name + \" #\" + 42");
    printf("string concat = "); val_print(r); printf("\n");

    /* Factorial via manual unrolling */
    eval_line("let f = 1 * 2 * 3 * 4 * 5 * 6 * 7 * 8 * 9 * 10;");
    r = eval_line("f");
    printf("10! = "); val_print(r); printf("\n");

    /* Boolean chains */
    r = eval_line("(5 > 3) && (10 != 11) && (\"abc\" < \"def\")");
    printf("complex bool = "); val_print(r); printf("\n");

    /* Nested conditionals */
    eval_line("let score = 92;");
    eval_line("if (score >= 90) print(\"Grade: A\"); else if (score >= 80) print(\"Grade: B\");");

    /* Type checking */
    r = eval_line("type(3.14)");
    printf("type(3.14) = "); val_print(r); printf("\n");
    r = eval_line("type(\"hi\")");
    printf("type(\"hi\") = "); val_print(r); printf("\n");
    r = eval_line("type(5 > 3)");
    printf("type(5 > 3) = "); val_print(r); printf("\n");
}

static void test_dataset(void) {
    printf("\n=== Dataset Statistics ===\n");

    DataSet *ds = dataset_create();
    double values[] = {4.0, 8.0, 15.0, 16.0, 23.0, 42.0};
    int n = sizeof(values) / sizeof(values[0]);

    for (int i = 0; i < n; i++) {
        dataset_push(ds, values[i]);
    }

    printf("Data: ");
    for (int i = 0; i < ds->count; i++) {
        if (i > 0) printf(", ");
        printf("%.0f", ds->data[i]);
    }
    printf("\n");

    printf("Count: %d\n", ds->count);
    printf("Mean: %.2f\n", dataset_mean(ds));
    printf("Variance: %.2f\n", dataset_variance(ds));
    printf("Stddev: %.2f\n", dataset_stddev(ds));
    printf("Min: %.0f\n", dataset_min(ds));
    printf("Max: %.0f\n", dataset_max(ds));
    printf("Median: %.1f\n", dataset_median(ds));

    /* Test with odd count */
    dataset_push(ds, 50.0);
    printf("Median (7 items): %.1f\n", dataset_median(ds));

    dataset_free(ds);
}

static void test_hashmap(void) {
    printf("\n=== HashMap ===\n");

    HashMap *map = hashmap_create();

    hashmap_set(map, "alice", 95);
    hashmap_set(map, "bob", 87);
    hashmap_set(map, "charlie", 72);
    hashmap_set(map, "diana", 91);
    hashmap_set(map, "eve", 88);

    int found;
    printf("alice: %d\n", hashmap_get(map, "alice", &found));
    printf("bob: %d\n", hashmap_get(map, "bob", &found));
    printf("charlie: %d\n", hashmap_get(map, "charlie", &found));

    /* Update */
    hashmap_set(map, "charlie", 78);
    printf("charlie (updated): %d\n", hashmap_get(map, "charlie", &found));

    /* Contains */
    printf("contains(diana): %d\n", hashmap_contains(map, "diana"));
    printf("contains(frank): %d\n", hashmap_contains(map, "frank"));

    /* Remove */
    hashmap_remove(map, "bob");
    printf("contains(bob) after remove: %d\n", hashmap_contains(map, "bob"));
    printf("size after remove: %d\n", map->size);

    /* Many insertions to test collision handling */
    char key[32];
    for (int i = 0; i < 100; i++) {
        snprintf(key, sizeof(key), "key_%d", i);
        hashmap_set(map, key, i * 10);
    }
    printf("size after 100 inserts: %d\n", map->size);

    /* Verify some values */
    printf("key_0: %d\n", hashmap_get(map, "key_0", &found));
    printf("key_50: %d\n", hashmap_get(map, "key_50", &found));
    printf("key_99: %d\n", hashmap_get(map, "key_99", &found));

    hashmap_free(map);
}

int main(void) {
    var_store_init(&global_vars);
    history_init(&global_history);

    test_basic_arithmetic();
    test_comparisons();
    test_logical_ops();
    test_variables();
    test_strings();
    test_builtins();
    test_conditionals();
    test_complex_expressions();
    test_dataset();
    test_hashmap();

    printf("\nAll tests completed.\n");
    return 0;
}
