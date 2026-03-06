/*
 * Extended cJSON test — single-file version for deep behavioral verification.
 * Inlines cJSON.h + cJSON.c + extended test harness.
 *
 * Minimal cJSON implementation — subset of the MIT-licensed cJSON library
 * by Dave Gamble (https://github.com/DaveGamble/cJSON).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

/* ========== cJSON types (from cJSON.h) ========== */

#define cJSON_Invalid (0)
#define cJSON_False   (1 << 0)
#define cJSON_True    (1 << 1)
#define cJSON_NULL    (1 << 2)
#define cJSON_Number  (1 << 3)
#define cJSON_String  (1 << 4)
#define cJSON_Array   (1 << 5)
#define cJSON_Object  (1 << 6)
#define cJSON_Raw     (1 << 7)

typedef struct cJSON {
    struct cJSON *next;
    struct cJSON *prev;
    struct cJSON *child;
    int type;
    char *valuestring;
    int valueint;
    double valuedouble;
    char *string;  /* key name */
} cJSON;

/* Forward declarations */
cJSON *cJSON_CreateObject(void);
cJSON *cJSON_CreateString(const char *string);
cJSON *cJSON_CreateNumber(double num);
cJSON *cJSON_CreateBool(int boolean);
cJSON *cJSON_CreateIntArray(const int *numbers, int count);
cJSON *cJSON_AddStringToObject(cJSON *object, const char *name, const char *string);
cJSON *cJSON_AddNumberToObject(cJSON *object, const char *name, double number);
cJSON *cJSON_AddBoolToObject(cJSON *object, const char *name, int boolean);
cJSON *cJSON_Parse(const char *value);
cJSON *cJSON_GetObjectItem(const cJSON *object, const char *string);
cJSON *cJSON_GetArrayItem(const cJSON *array, int index);
int cJSON_GetArraySize(const cJSON *array);
char *cJSON_PrintUnformatted(const cJSON *item);
void cJSON_Delete(cJSON *item);

/* ========== cJSON implementation (from cJSON.c) ========== */

static cJSON *cJSON_New_Item(void) {
    cJSON *node = (cJSON *)calloc(1, sizeof(cJSON));
    return node;
}

static char *cJSON_strdup(const char *str) {
    size_t len = strlen(str) + 1;
    char *copy = (char *)malloc(len);
    if (copy) memcpy(copy, str, len);
    return copy;
}

static void suffix_object(cJSON *prev, cJSON *item) {
    prev->next = item;
    item->prev = prev;
}

static cJSON *add_item_to_object(cJSON *object, const char *name, cJSON *item) {
    if (!object || !item) return NULL;
    item->string = cJSON_strdup(name);
    if (!object->child) {
        object->child = item;
    } else {
        cJSON *child = object->child;
        while (child->next) child = child->next;
        suffix_object(child, item);
    }
    return item;
}

cJSON *cJSON_CreateObject(void) {
    cJSON *item = cJSON_New_Item();
    if (item) item->type = cJSON_Object;
    return item;
}

cJSON *cJSON_CreateString(const char *string) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = cJSON_String;
        item->valuestring = cJSON_strdup(string ? string : "");
    }
    return item;
}

cJSON *cJSON_CreateNumber(double num) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = cJSON_Number;
        item->valuedouble = num;
        item->valueint = (int)num;
    }
    return item;
}

cJSON *cJSON_CreateBool(int boolean) {
    cJSON *item = cJSON_New_Item();
    if (item) {
        item->type = boolean ? cJSON_True : cJSON_False;
    }
    return item;
}

cJSON *cJSON_CreateIntArray(const int *numbers, int count) {
    cJSON *arr = cJSON_New_Item();
    if (!arr) return NULL;
    arr->type = cJSON_Array;
    for (int i = 0; i < count; i++) {
        cJSON *n = cJSON_CreateNumber((double)numbers[i]);
        if (i == 0) {
            arr->child = n;
        } else {
            cJSON *child = arr->child;
            while (child->next) child = child->next;
            suffix_object(child, n);
        }
    }
    return arr;
}

cJSON *cJSON_AddStringToObject(cJSON *object, const char *name, const char *string) {
    return add_item_to_object(object, name, cJSON_CreateString(string));
}

cJSON *cJSON_AddNumberToObject(cJSON *object, const char *name, double number) {
    return add_item_to_object(object, name, cJSON_CreateNumber(number));
}

cJSON *cJSON_AddBoolToObject(cJSON *object, const char *name, int boolean) {
    return add_item_to_object(object, name, cJSON_CreateBool(boolean));
}

cJSON *cJSON_GetObjectItem(const cJSON *object, const char *string) {
    if (!object) return NULL;
    cJSON *child = object->child;
    while (child) {
        if (child->string && strcmp(child->string, string) == 0) {
            return child;
        }
        child = child->next;
    }
    return NULL;
}

cJSON *cJSON_GetArrayItem(const cJSON *array, int index) {
    if (!array) return NULL;
    cJSON *child = array->child;
    while (child && index > 0) {
        child = child->next;
        index--;
    }
    return child;
}

int cJSON_GetArraySize(const cJSON *array) {
    if (!array) return 0;
    int size = 0;
    cJSON *child = array->child;
    while (child) {
        size++;
        child = child->next;
    }
    return size;
}

/* --- Printing (unformatted) --- */

static char *print_value(const cJSON *item);

static char *print_string(const char *str) {
    if (!str) return cJSON_strdup("\"\"");
    size_t len = strlen(str);
    char *buf = (char *)malloc(len * 2 + 3);
    if (!buf) return NULL;
    char *ptr = buf;
    *ptr++ = '"';
    for (size_t i = 0; i < len; i++) {
        char c = str[i];
        if (c == '"' || c == '\\') { *ptr++ = '\\'; *ptr++ = c; }
        else if (c == '\n') { *ptr++ = '\\'; *ptr++ = 'n'; }
        else if (c == '\t') { *ptr++ = '\\'; *ptr++ = 't'; }
        else *ptr++ = c;
    }
    *ptr++ = '"';
    *ptr = '\0';
    return buf;
}

static char *print_number(const cJSON *item) {
    char *buf = (char *)malloc(64);
    if (!buf) return NULL;
    double d = item->valuedouble;
    if (d == 0) {
        strcpy(buf, "0");
    } else if (fabs(((double)item->valueint) - d) <= 1e-10 && d <= 2147483647.0 && d >= -2147483648.0) {
        sprintf(buf, "%d", item->valueint);
    } else {
        sprintf(buf, "%g", d);
    }
    return buf;
}

static char *print_array(const cJSON *item) {
    size_t total = 3;
    cJSON *child = item->child;
    char **children = NULL;
    int count = cJSON_GetArraySize(item);
    if (count > 0) {
        children = (char **)malloc(sizeof(char *) * (size_t)count);
        int i = 0;
        child = item->child;
        while (child) {
            children[i] = print_value(child);
            total += strlen(children[i]) + 1;
            child = child->next;
            i++;
        }
    }

    char *buf = (char *)malloc(total + 1);
    if (!buf) { free(children); return NULL; }
    char *ptr = buf;
    *ptr++ = '[';
    for (int i = 0; i < count; i++) {
        if (i > 0) *ptr++ = ',';
        size_t len = strlen(children[i]);
        memcpy(ptr, children[i], len);
        ptr += len;
        free(children[i]);
    }
    *ptr++ = ']';
    *ptr = '\0';
    free(children);
    return buf;
}

static char *print_object(const cJSON *item) {
    size_t total = 3;
    cJSON *child = item->child;

    int count = 0;
    while (child) { count++; child = child->next; }

    char **keys = NULL;
    char **vals = NULL;
    if (count > 0) {
        keys = (char **)malloc(sizeof(char *) * (size_t)count);
        vals = (char **)malloc(sizeof(char *) * (size_t)count);
        child = item->child;
        for (int i = 0; i < count; i++) {
            keys[i] = print_string(child->string);
            vals[i] = print_value(child);
            total += strlen(keys[i]) + 1 + strlen(vals[i]) + 1;
            child = child->next;
        }
    }

    char *buf = (char *)malloc(total + 1);
    if (!buf) { free(keys); free(vals); return NULL; }
    char *ptr = buf;
    *ptr++ = '{';
    for (int i = 0; i < count; i++) {
        if (i > 0) *ptr++ = ',';
        size_t klen = strlen(keys[i]);
        memcpy(ptr, keys[i], klen);
        ptr += klen;
        *ptr++ = ':';
        size_t vlen = strlen(vals[i]);
        memcpy(ptr, vals[i], vlen);
        ptr += vlen;
        free(keys[i]);
        free(vals[i]);
    }
    *ptr++ = '}';
    *ptr = '\0';
    free(keys);
    free(vals);
    return buf;
}

static char *print_value(const cJSON *item) {
    if (!item) return cJSON_strdup("null");
    switch (item->type) {
        case cJSON_False:  return cJSON_strdup("false");
        case cJSON_True:   return cJSON_strdup("true");
        case cJSON_NULL:   return cJSON_strdup("null");
        case cJSON_Number: return print_number(item);
        case cJSON_String: return print_string(item->valuestring);
        case cJSON_Array:  return print_array(item);
        case cJSON_Object: return print_object(item);
        default:           return cJSON_strdup("");
    }
}

char *cJSON_PrintUnformatted(const cJSON *item) {
    return print_value(item);
}

/* --- Parsing (minimal) --- */

static const char *skip_whitespace(const char *s) {
    while (s && *s && (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r')) s++;
    return s;
}

static const char *parse_value(const char *s, cJSON *item);

static const char *parse_string(const char *s, char **out) {
    if (*s != '"') return NULL;
    s++;
    const char *start = s;
    size_t len = 0;
    while (*s && *s != '"') {
        if (*s == '\\') s++;
        s++;
        len++;
    }
    char *buf = (char *)malloc(len + 1);
    char *ptr = buf;
    s = start;
    while (*s && *s != '"') {
        if (*s == '\\') {
            s++;
            switch (*s) {
                case 'n': *ptr++ = '\n'; break;
                case 't': *ptr++ = '\t'; break;
                case '"': *ptr++ = '"'; break;
                case '\\': *ptr++ = '\\'; break;
                default: *ptr++ = *s; break;
            }
        } else {
            *ptr++ = *s;
        }
        s++;
    }
    *ptr = '\0';
    if (*s == '"') s++;
    *out = buf;
    return s;
}

static const char *parse_number(const char *s, cJSON *item) {
    double n = 0;
    int sign = 1;
    if (*s == '-') { sign = -1; s++; }
    while (*s >= '0' && *s <= '9') {
        n = n * 10 + (*s - '0');
        s++;
    }
    if (*s == '.') {
        s++;
        double frac = 0.1;
        while (*s >= '0' && *s <= '9') {
            n += (*s - '0') * frac;
            frac *= 0.1;
            s++;
        }
    }
    n *= sign;
    item->type = cJSON_Number;
    item->valuedouble = n;
    item->valueint = (int)n;
    return s;
}

static const char *parse_array(const char *s, cJSON *item) {
    item->type = cJSON_Array;
    s = skip_whitespace(s + 1);
    if (*s == ']') return s + 1;

    cJSON *child = cJSON_New_Item();
    item->child = child;
    s = skip_whitespace(parse_value(skip_whitespace(s), child));
    if (!s) return NULL;

    while (*s == ',') {
        cJSON *new_item = cJSON_New_Item();
        child->next = new_item;
        new_item->prev = child;
        child = new_item;
        s = skip_whitespace(parse_value(skip_whitespace(s + 1), child));
        if (!s) return NULL;
    }

    if (*s == ']') return s + 1;
    return NULL;
}

static const char *parse_object(const char *s, cJSON *item) {
    item->type = cJSON_Object;
    s = skip_whitespace(s + 1);
    if (*s == '}') return s + 1;

    cJSON *child = cJSON_New_Item();
    item->child = child;
    s = skip_whitespace(s);
    s = parse_string(s, &child->string);
    if (!s) return NULL;
    s = skip_whitespace(s);
    if (*s != ':') return NULL;
    s = skip_whitespace(s + 1);
    s = parse_value(s, child);
    if (!s) return NULL;
    s = skip_whitespace(s);

    while (*s == ',') {
        cJSON *new_item = cJSON_New_Item();
        child->next = new_item;
        new_item->prev = child;
        child = new_item;
        s = skip_whitespace(s + 1);
        s = parse_string(s, &child->string);
        if (!s) return NULL;
        s = skip_whitespace(s);
        if (*s != ':') return NULL;
        s = skip_whitespace(s + 1);
        s = parse_value(s, child);
        if (!s) return NULL;
        s = skip_whitespace(s);
    }

    if (*s == '}') return s + 1;
    return NULL;
}

static const char *parse_value(const char *s, cJSON *item) {
    if (!s) return NULL;
    s = skip_whitespace(s);
    if (!*s) return NULL;

    if (*s == '"') {
        item->type = cJSON_String;
        return parse_string(s, &item->valuestring);
    }
    if (*s == '-' || (*s >= '0' && *s <= '9')) {
        return parse_number(s, item);
    }
    if (*s == '[') return parse_array(s, item);
    if (*s == '{') return parse_object(s, item);
    if (strncmp(s, "true", 4) == 0)  { item->type = cJSON_True;  return s + 4; }
    if (strncmp(s, "false", 5) == 0) { item->type = cJSON_False; return s + 5; }
    if (strncmp(s, "null", 4) == 0)  { item->type = cJSON_NULL;  return s + 4; }

    return NULL;
}

cJSON *cJSON_Parse(const char *value) {
    if (!value) return NULL;
    cJSON *item = cJSON_New_Item();
    if (!item) return NULL;
    const char *end = parse_value(skip_whitespace(value), item);
    if (!end) {
        cJSON_Delete(item);
        return NULL;
    }
    return item;
}

void cJSON_Delete(cJSON *item) {
    cJSON *next;
    while (item) {
        next = item->next;
        if (item->child) cJSON_Delete(item->child);
        if (item->valuestring) free(item->valuestring);
        if (item->string) free(item->string);
        free(item);
        item = next;
    }
}

/* ========== Extended test harness ========== */

int main(void) {
    /* Test 1: Empty object */
    cJSON *empty_obj = cJSON_CreateObject();
    char *json = cJSON_PrintUnformatted(empty_obj);
    printf("empty_obj: %s\n", json);
    free(json);
    cJSON_Delete(empty_obj);

    /* Test 2: Nested objects */
    cJSON *outer = cJSON_CreateObject();
    cJSON *inner = cJSON_CreateObject();
    cJSON_AddNumberToObject(inner, "b", 1);
    add_item_to_object(outer, "a", inner);
    json = cJSON_PrintUnformatted(outer);
    printf("nested: %s\n", json);
    free(json);
    cJSON_Delete(outer);

    /* Test 3: Escaped strings */
    cJSON *esc_obj = cJSON_CreateObject();
    cJSON_AddStringToObject(esc_obj, "quote", "say \"hello\"");
    cJSON_AddStringToObject(esc_obj, "backslash", "path\\to\\file");
    cJSON_AddStringToObject(esc_obj, "newline", "line1\nline2");
    cJSON_AddStringToObject(esc_obj, "tab", "col1\tcol2");
    json = cJSON_PrintUnformatted(esc_obj);
    printf("escaped: %s\n", json);
    free(json);
    cJSON_Delete(esc_obj);

    /* Test 4: Parse and re-print roundtrip */
    const char *input = "{\"x\":10,\"y\":[1,2,3],\"z\":{\"w\":true}}";
    cJSON *parsed = cJSON_Parse(input);
    json = cJSON_PrintUnformatted(parsed);
    printf("roundtrip: %s\n", json);
    free(json);
    cJSON_Delete(parsed);

    /* Test 5: Parse failure returns NULL */
    cJSON *bad = cJSON_Parse("invalid json");
    printf("parse_fail: %s\n", bad == NULL ? "NULL" : "NOT_NULL");
    if (bad) cJSON_Delete(bad);

    bad = cJSON_Parse("{missing_quote: 1}");
    printf("parse_fail2: %s\n", bad == NULL ? "NULL" : "NOT_NULL");
    if (bad) cJSON_Delete(bad);

    /* Test 6: Array iteration */
    int nums[] = {10, 20, 30, 40, 50};
    cJSON *arr = cJSON_CreateIntArray(nums, 5);
    int size = cJSON_GetArraySize(arr);
    printf("arr_size: %d\n", size);
    for (int i = 0; i < size; i++) {
        printf("arr[%d]=%d\n", i, cJSON_GetArrayItem(arr, i)->valueint);
    }
    cJSON_Delete(arr);

    /* Test 7: Number edge cases */
    cJSON *zero = cJSON_CreateNumber(0);
    json = cJSON_PrintUnformatted(zero);
    printf("zero: %s\n", json);
    free(json);
    cJSON_Delete(zero);

    cJSON *negative = cJSON_CreateNumber(-42);
    json = cJSON_PrintUnformatted(negative);
    printf("negative: %s\n", json);
    free(json);
    cJSON_Delete(negative);

    cJSON *large = cJSON_CreateNumber(1000000);
    json = cJSON_PrintUnformatted(large);
    printf("large: %s\n", json);
    free(json);
    cJSON_Delete(large);

    /* Test 8: Boolean values */
    cJSON *bool_obj = cJSON_CreateObject();
    cJSON_AddBoolToObject(bool_obj, "t", 1);
    cJSON_AddBoolToObject(bool_obj, "f", 0);
    json = cJSON_PrintUnformatted(bool_obj);
    printf("bools: %s\n", json);
    free(json);
    cJSON_Delete(bool_obj);

    /* Test 9: Null value */
    parsed = cJSON_Parse("null");
    json = cJSON_PrintUnformatted(parsed);
    printf("null_val: %s\n", json);
    free(json);
    cJSON_Delete(parsed);

    /* Test 10: Parse negative number */
    parsed = cJSON_Parse("-99");
    printf("neg_parse: %d\n", parsed->valueint);
    cJSON_Delete(parsed);

    /* Test 11: Parse with whitespace */
    parsed = cJSON_Parse("  {  \"a\" : 1 , \"b\" : 2  }  ");
    json = cJSON_PrintUnformatted(parsed);
    printf("whitespace: %s\n", json);
    free(json);
    cJSON_Delete(parsed);

    /* Test 12: Empty array */
    parsed = cJSON_Parse("[]");
    json = cJSON_PrintUnformatted(parsed);
    printf("empty_arr: %s\n", json);
    printf("empty_arr_size: %d\n", cJSON_GetArraySize(parsed));
    free(json);
    cJSON_Delete(parsed);

    printf("extended_done\n");
    return 0;
}
