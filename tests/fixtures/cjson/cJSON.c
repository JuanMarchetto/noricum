/*
 * Minimal cJSON implementation — subset of the MIT-licensed cJSON library
 * by Dave Gamble (https://github.com/DaveGamble/cJSON).
 * Stripped down to support Noricum test fixtures only.
 */
#include "cJSON.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

/* Internal helpers */
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

/* Append item to array/object child list */
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

/* --- Creation --- */

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

/* --- Add to object --- */

cJSON *cJSON_AddStringToObject(cJSON *object, const char *name, const char *string) {
    return add_item_to_object(object, name, cJSON_CreateString(string));
}

cJSON *cJSON_AddNumberToObject(cJSON *object, const char *name, double number) {
    return add_item_to_object(object, name, cJSON_CreateNumber(number));
}

cJSON *cJSON_AddBoolToObject(cJSON *object, const char *name, int boolean) {
    return add_item_to_object(object, name, cJSON_CreateBool(boolean));
}

/* --- Access --- */

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

/* Forward declaration */
static char *print_value(const cJSON *item);

static char *print_string(const char *str) {
    if (!str) return cJSON_strdup("\"\"");
    size_t len = strlen(str);
    /* Allocate for quotes + escapes (worst case: every char escaped) */
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
    /* Calculate total size needed */
    size_t total = 3; /* [ ] \0 */
    cJSON *child = item->child;

    /* First pass: print children to get sizes */
    char **children = NULL;
    int count = cJSON_GetArraySize(item);
    if (count > 0) {
        children = (char **)malloc(sizeof(char *) * (size_t)count);
        int i = 0;
        child = item->child;
        while (child) {
            children[i] = print_value(child);
            total += strlen(children[i]) + 1; /* +1 for comma */
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
            total += strlen(keys[i]) + 1 + strlen(vals[i]) + 1; /* key:val, */
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
    s = skip_whitespace(s + 1); /* skip [ */
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
    s = skip_whitespace(s + 1); /* skip { */
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

/* --- Cleanup --- */

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
